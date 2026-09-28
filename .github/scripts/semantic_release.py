#!/usr/bin/env python3
import argparse
import os
import pathlib
import re
import subprocess

STABLE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
RC = re.compile(r"^v(\d+)\.(\d+)\.(\d+)-rc\.(\d+)$")
CONVENTIONAL = re.compile(
    r"^(feat|fix|perf|revert|refactor|docs|test|build|ci|chore)"
    r"(?:\(([^)]+)\))?(!)?:\s+(.+)$"
)
TRAILER = re.compile(
    r"^(?P<token>[A-Za-z][A-Za-z0-9-]*(?: [A-Za-z][A-Za-z0-9-]*)*):\s+(?P<value>\S.*)$"
)
RANK = {None: 0, "patch": 1, "minor": 2, "major": 3}


def git(*args, check=True):
    """Run a Git command and return its completed process result."""
    result = subprocess.run(
        ["git", *args],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "git command failed")
    return result


def semver_key(tag):
    """Return a sortable semantic-version tuple for a stable tag."""
    match = STABLE.fullmatch(tag)
    if not match:
        return None
    return tuple(int(part) for part in match.groups())


def prerelease_key(tag):
    """Return a sortable semantic-version tuple including the RC sequence."""
    match = RC.fullmatch(tag)
    if not match:
        return None
    major, minor, patch, sequence = match.groups()
    return (int(major), int(minor), int(patch), int(sequence))


def is_ancestor(ref, head="HEAD"):
    """Return whether ref is an ancestor of the selected head."""
    return git("merge-base", "--is-ancestor", ref, head, check=False).returncode == 0


def write_output(path, name, value):
    """Append a name/value pair to a GitHub Actions output file when configured."""
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(f"{name}={value}\n")


def parse_args():
    """Parse command-line options for release calculation."""
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--forced-bump",
        choices=("auto", "patch", "minor", "major", "promote"),
        default="auto",
    )
    parser.add_argument("--channel", choices=("stable", "rc"), default="stable")
    parser.add_argument("--notes", required=True)
    parser.add_argument("--output")
    return parser.parse_args()


def release_line(subject, body):
    """Select the Conventional Commit line used to classify a commit."""
    candidates = [subject.strip()]
    if subject.startswith("Merge pull request #"):
        candidates.extend(line.strip() for line in body.splitlines() if line.strip())
    for candidate in candidates:
        if CONVENTIONAL.fullmatch(candidate):
            return candidate
    return subject.strip()


def commit_trailers(body):
    """Return the contiguous Git trailer block at the end of a commit body."""
    trailers = {}
    for raw in reversed(body.rstrip().splitlines()):
        line = raw.strip()
        if not line:
            if trailers:
                break
            continue
        match = TRAILER.fullmatch(line)
        if not match:
            break
        trailers[match.group("token")] = match.group("value").strip()
    return trailers


def commit_record(sha, subject, body):
    """Normalize commit metadata used by release classification."""
    line = release_line(subject, body)
    match = CONVENTIONAL.fullmatch(line)
    commit_type = match.group(1) if match else None
    scope = match.group(2) if match else None
    trailers = commit_trailers(body)
    breaking = bool(match and match.group(3)) or bool(
        trailers.get("BREAKING CHANGE") or trailers.get("BREAKING-CHANGE")
    )
    release_as_value = trailers.get("Release-As", "").lower()
    release_as = (
        release_as_value
        if release_as_value in {"major", "minor", "patch", "none"}
        else None
    )
    return {
        "sha": sha,
        "subject": line,
        "type": commit_type,
        "scope": scope,
        "breaking": breaking,
        "release_as": release_as,
    }


def commits_in(revision_range):
    """Return normalized first-parent commits in a revision range."""
    raw = git(
        "log",
        "--first-parent",
        "--format=%H%x1f%s%x1f%b%x1e",
        revision_range,
    ).stdout
    commits = []
    for block in raw.split("\x1e"):
        block = block.strip("\n")
        if not block:
            continue
        fields = block.split("\x1f", 2)
        if len(fields) == 3:
            commits.append(commit_record(*fields))
    return commits


def default_bump(item):
    """Return the default SemVer bump implied by a normalized commit."""
    if item["breaking"]:
        return "major"
    if item["release_as"]:
        return None if item["release_as"] == "none" else item["release_as"]
    if item["type"] == "feat":
        return "minor"
    if item["type"] in {"fix", "perf", "revert"}:
        return "patch"
    return None


def bump_version(current, bump):
    """Apply a major, minor, or patch bump to a semantic version tuple."""
    major, minor, patch = current
    if bump == "major":
        return (major + 1, 0, 0)
    if bump == "minor":
        return (major, minor + 1, 0)
    if bump == "patch":
        return (major, minor, patch + 1)
    raise ValueError(f"unsupported bump: {bump}")


def format_core(core):
    """Format a semantic-version core tuple as a v-prefixed tag."""
    return f"v{core[0]}.{core[1]}.{core[2]}"


def category_for(item):
    """Return the changelog category for a normalized commit."""
    if (item["scope"] or "").lower() == "security":
        return "Security"
    mapping = {
        "feat": "Features",
        "fix": "Fixes",
        "perf": "Performance",
        "revert": "Reverts",
        "refactor": "Refactoring",
        "docs": "Documentation",
        "build": "Build",
        "ci": "CI",
        "test": "Tests",
        "chore": "Chores",
    }
    if item["type"] in mapping:
        return mapping[item["type"]]
    if item["breaking"]:
        return "Breaking changes"
    return "Other changes"


def main():
    """Calculate release metadata and changelog output for the requested channel."""
    args = parse_args()
    all_tags = git("tag", "--list", "v*").stdout.splitlines()

    stable_tags = [tag for tag in all_tags if STABLE.fullmatch(tag)]
    reachable_stable = [tag for tag in stable_tags if is_ancestor(tag)]
    stable_tags.sort(key=semver_key, reverse=True)
    reachable_stable.sort(key=semver_key, reverse=True)

    highest_global = stable_tags[0] if stable_tags else None
    last_tag = reachable_stable[0] if reachable_stable else None
    if highest_global and (not last_tag or semver_key(highest_global) > semver_key(last_tag)):
        raise SystemExit(
            f"Latest stable tag {highest_global} is not in HEAD history; refusing a stale/divergent release."
        )

    current = semver_key(last_tag) if last_tag else (0, 0, 0)
    reachable_pre = [
        tag for tag in all_tags
        if RC.fullmatch(tag) and is_ancestor(tag)
    ]
    reachable_pre.sort(key=prerelease_key, reverse=True)
    active_pre = next(
        (
            tag for tag in reachable_pre
            if prerelease_key(tag)[:3] > current
        ),
        None,
    )

    forced = args.forced_bump
    release_ref = active_pre if forced == "promote" and active_pre else "HEAD"

    if forced == "promote":
        if args.channel != "stable":
            raise SystemExit("promote is only valid for the stable channel.")
        if not active_pre:
            raise SystemExit("No active prerelease exists to promote.")

        deferred_bump = None
        for item in commits_in(f"{active_pre}..HEAD"):
            candidate = default_bump(item)
            if RANK[candidate] > RANK[deferred_bump]:
                deferred_bump = candidate
        if deferred_bump:
            raise SystemExit(
                f"Release-worthy commits exist after {active_pre}; "
                "create a new release candidate before promotion."
            )

    if last_tag:
        if not is_ancestor(last_tag, release_ref):
            raise SystemExit(
                f"Stable tag {last_tag} is not an ancestor of release target {release_ref}."
            )
        revision_range = f"{last_tag}..{release_ref}"
        base_label = last_tag
    else:
        baseline = pathlib.Path(".github/release-baseline")
        if not baseline.exists():
            raise SystemExit(
                "No stable release exists and .github/release-baseline is missing; "
                "refusing to publish repository history implicitly."
            )
        baseline_sha = baseline.read_text(encoding="utf-8").strip()
        if not baseline_sha or not is_ancestor(baseline_sha, release_ref):
            raise SystemExit("Configured release baseline is not an ancestor of the release target.")
        revision_range = f"{baseline_sha}..{release_ref}"
        base_label = "repository release baseline"

    commits = commits_in(revision_range)

    calculated_bump = None
    for item in commits:
        candidate = default_bump(item)
        if RANK[candidate] > RANK[calculated_bump]:
            calculated_bump = candidate

    bump = None
    if forced in {"major", "minor", "patch"}:
        if RANK[forced] < RANK[calculated_bump]:
            raise SystemExit(
                f"Forced {forced} bump is below required {calculated_bump} bump."
            )
        bump = forced
    elif forced != "promote":
        bump = calculated_bump

    prerelease = False
    if args.channel == "stable":
        if active_pre and forced != "promote":
            if forced == "auto":
                print(
                    f"Active prerelease {active_pre} exists; automatic stable publication is paused."
                )
                write_output(args.output, "release", "false")
                write_output(args.output, "active_prerelease_tag", active_pre)
                return
            raise SystemExit(
                f"Active prerelease {active_pre} exists; promote it before forcing another stable bump."
            )
        if forced == "promote":
            target_core = prerelease_key(active_pre)[:3]
            bump_label = "promote"
        else:
            if not bump:
                print("No release-worthy Conventional Commit since the release base.")
                write_output(args.output, "release", "false")
                return
            target_core = bump_version(current, bump)
            bump_label = bump
        tag = format_core(target_core)
    else:
        prerelease = True
        if forced == "promote":
            raise SystemExit("promote cannot create a prerelease.")

        desired_core = bump_version(current, bump) if bump else None
        if active_pre:
            active_key = prerelease_key(active_pre)
            active_core = active_key[:3]
            if not git("rev-list", f"{active_pre}..HEAD").stdout.strip():
                print(f"No commits since {active_pre}; no new release candidate.")
                write_output(args.output, "release", "false")
                return

            if forced == "auto":
                post_rc_bump = None
                for item in commits_in(f"{active_pre}..HEAD"):
                    candidate = default_bump(item)
                    if RANK[candidate] > RANK[post_rc_bump]:
                        post_rc_bump = candidate
                if not post_rc_bump:
                    print("No release-worthy change since the active release candidate.")
                    write_output(args.output, "release", "false")
                    return

            if forced in {"major", "minor", "patch"} and desired_core < active_core:
                raise SystemExit(
                    f"Forced {forced} bump targets {format_core(desired_core)}, "
                    f"which is behind active prerelease {active_pre}."
                )

            if desired_core and desired_core > active_core:
                target_core = desired_core
                next_rc = 1
                bump_label = bump
            else:
                target_core = active_core
                next_rc = active_key[3] + 1
                bump_label = "prerelease"
        else:
            if not bump:
                print("No release-worthy change for a release candidate.")
                write_output(args.output, "release", "false")
                return
            target_core = desired_core
            next_rc = 1
            bump_label = bump
        tag = f"{format_core(target_core)}-rc.{next_rc}"

    if tag in all_tags:
        raise SystemExit(f"Tag {tag} already exists.")

    repository = os.environ.get("GITHUB_REPOSITORY", "repository")
    category_order = [
        "Breaking changes",
        "Security",
        "Features",
        "Fixes",
        "Performance",
        "Reverts",
        "Refactoring",
        "Documentation",
        "Build",
        "CI",
        "Tests",
        "Chores",
        "Other changes",
    ]
    grouped = {heading: [] for heading in category_order}
    for item in commits:
        grouped[category_for(item)].append(item)

    lines = [
        f"# {tag}",
        "",
        f"Changes since {base_label}.",
        "",
    ]
    for heading in category_order:
        selected = grouped[heading]
        if not selected:
            continue
        lines.extend([f"## {heading}", ""])
        for item in selected:
            short = item["sha"][:7]
            url = f"https://github.com/{repository}/commit/{item['sha']}"
            breaking_prefix = "**Breaking:** " if item["breaking"] else ""
            lines.append(f"- {breaking_prefix}{item['subject']} ([{short}]({url}))")
        lines.append("")

    pathlib.Path(args.notes).write_text(
        "\n".join(lines).rstrip() + "\n",
        encoding="utf-8",
    )

    target_sha = git("rev-parse", "--verify", f"{release_ref}^{{commit}}").stdout.strip()
    print(f"Release bump: {bump_label}")
    print(f"Previous stable tag: {last_tag or 'none'}")
    print(f"Next tag: {tag}")
    print(f"Release target: {target_sha}")
    write_output(args.output, "release", "true")
    write_output(args.output, "tag", tag)
    write_output(args.output, "bump", bump_label)
    write_output(args.output, "base_tag", last_tag or "")
    write_output(args.output, "active_prerelease_tag", active_pre or "")
    write_output(args.output, "prerelease", "true" if prerelease else "false")
    write_output(args.output, "target_sha", target_sha)


if __name__ == "__main__":
    main()
