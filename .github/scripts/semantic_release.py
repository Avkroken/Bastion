#!/usr/bin/env python3
import argparse
import os
import pathlib
import re
import subprocess

STABLE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
PRERELEASE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)-([0-9A-Za-z.-]+)$")
CONVENTIONAL = re.compile(
    r"^(feat|fix|perf|revert|refactor|docs|test|build|ci|chore)"
    r"(?:\(([^)]+)\))?(!)?:\s+(.+)$"
)
BREAKING_FOOTER = re.compile(r"(?m)^BREAKING(?: CHANGE|-CHANGE):\s+\S")
RELEASE_AS = re.compile(r"(?m)^Release-As:\s*(major|minor|patch|none)\s*$")
RANK = {None: 0, "patch": 1, "minor": 2, "major": 3}


def git(*args, check=True):
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
    match = STABLE.fullmatch(tag)
    if not match:
        return None
    return tuple(int(part) for part in match.groups())


def prerelease_key(tag):
    match = PRERELEASE.fullmatch(tag)
    if not match:
        return None
    major, minor, patch, suffix = match.groups()
    return (int(major), int(minor), int(patch), suffix)


def is_ancestor(ref):
    return git("merge-base", "--is-ancestor", ref, "HEAD", check=False).returncode == 0


def write_output(path, name, value):
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(f"{name}={value}\n")


def parse_args():
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
    candidates = [subject.strip()]
    if subject.startswith("Merge pull request #"):
        candidates.extend(line.strip() for line in body.splitlines() if line.strip())
    for candidate in candidates:
        if CONVENTIONAL.fullmatch(candidate):
            return candidate
    return subject.strip()


def commit_record(sha, subject, body):
    line = release_line(subject, body)
    match = CONVENTIONAL.fullmatch(line)
    commit_type = match.group(1) if match else None
    scope = match.group(2) if match else None
    breaking = bool(match and match.group(3)) or bool(BREAKING_FOOTER.search(body))
    release_as_match = RELEASE_AS.search(body)
    release_as = release_as_match.group(1) if release_as_match else None
    return {
        "sha": sha,
        "subject": line,
        "type": commit_type,
        "scope": scope,
        "breaking": breaking,
        "release_as": release_as,
    }


def default_bump(item):
    if item["release_as"]:
        return None if item["release_as"] == "none" else item["release_as"]
    if item["breaking"]:
        return "major"
    if item["type"] == "feat":
        return "minor"
    if item["type"] in {"fix", "perf", "revert"}:
        return "patch"
    return None


def bump_version(current, bump):
    major, minor, patch = current
    if bump == "major":
        return (major + 1, 0, 0)
    if bump == "minor":
        return (major, minor + 1, 0)
    if bump == "patch":
        return (major, minor, patch + 1)
    raise ValueError(f"unsupported bump: {bump}")


def format_core(core):
    return f"v{core[0]}.{core[1]}.{core[2]}"


def main():
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
        if PRERELEASE.fullmatch(tag) and is_ancestor(tag)
    ]
    reachable_pre.sort(key=prerelease_key, reverse=True)
    active_pre = next(
        (
            tag for tag in reachable_pre
            if prerelease_key(tag)[:3] > current
        ),
        None,
    )

    if last_tag:
        revision_range = f"{last_tag}..HEAD"
    else:
        baseline = pathlib.Path(".github/release-baseline")
        if baseline.exists():
            baseline_sha = baseline.read_text(encoding="utf-8").strip()
            if not baseline_sha or not is_ancestor(baseline_sha):
                raise SystemExit("Configured release baseline is not an ancestor of HEAD.")
            revision_range = f"{baseline_sha}..HEAD"
        else:
            revision_range = "HEAD"

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
        if len(fields) != 3:
            continue
        commits.append(commit_record(*fields))

    forced = args.forced_bump
    bump = None
    if forced in {"major", "minor", "patch"}:
        bump = forced
    elif forced == "promote":
        if args.channel != "stable":
            raise SystemExit("promote is only valid for the stable channel.")
        if not active_pre:
            raise SystemExit("No active prerelease exists to promote.")
    else:
        for item in commits:
            candidate = default_bump(item)
            if RANK[candidate] > RANK[bump]:
                bump = candidate

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
        if active_pre:
            active_key = prerelease_key(active_pre)
            target_core = active_key[:3]
            if not git("rev-list", f"{active_pre}..HEAD").stdout.strip():
                print(f"No commits since {active_pre}; no new release candidate.")
                write_output(args.output, "release", "false")
                return
            suffix_match = re.fullmatch(r"rc\.(\d+)", active_key[3])
            next_rc = int(suffix_match.group(1)) + 1 if suffix_match else 1
            bump_label = "prerelease"
        else:
            if forced == "promote":
                raise SystemExit("promote cannot create a prerelease.")
            if not bump:
                print("No release-worthy change for a release candidate.")
                write_output(args.output, "release", "false")
                return
            target_core = bump_version(current, bump)
            next_rc = 1
            bump_label = bump
        tag = f"{format_core(target_core)}-rc.{next_rc}"

    if tag in all_tags:
        raise SystemExit(f"Tag {tag} already exists.")

    repository = os.environ.get("GITHUB_REPOSITORY", "repository")
    categories = [
        ("Breaking changes", lambda c: c["breaking"]),
        ("Security", lambda c: (c["scope"] or "").lower() == "security"),
        ("Features", lambda c: c["type"] == "feat"),
        ("Fixes", lambda c: c["type"] == "fix"),
        ("Performance", lambda c: c["type"] == "perf"),
        ("Reverts", lambda c: c["type"] == "revert"),
        ("Refactoring", lambda c: c["type"] == "refactor"),
        ("Documentation", lambda c: c["type"] == "docs"),
        ("Build", lambda c: c["type"] == "build"),
        ("CI", lambda c: c["type"] == "ci"),
        ("Tests", lambda c: c["type"] == "test"),
        ("Chores", lambda c: c["type"] == "chore"),
        ("Other changes", lambda c: c["type"] is None),
    ]

    lines = [
        f"# {tag}",
        "",
        f"Changes since {last_tag}." if last_tag else "Changes since the repository release baseline.",
        "",
    ]
    for heading, predicate in categories:
        selected = [item for item in commits if predicate(item)]
        if not selected:
            continue
        lines.extend([f"## {heading}", ""])
        for item in selected:
            short = item["sha"][:7]
            url = f"https://github.com/{repository}/commit/{item['sha']}"
            lines.append(f"- {item['subject']} ([{short}]({url}))")
        lines.append("")

    pathlib.Path(args.notes).write_text(
        "\n".join(lines).rstrip() + "\n",
        encoding="utf-8",
    )

    print(f"Release bump: {bump_label}")
    print(f"Previous stable tag: {last_tag or 'none'}")
    print(f"Next tag: {tag}")
    write_output(args.output, "release", "true")
    write_output(args.output, "tag", tag)
    write_output(args.output, "bump", bump_label)
    write_output(args.output, "base_tag", last_tag or "")
    write_output(args.output, "active_prerelease_tag", active_pre or "")
    write_output(args.output, "prerelease", "true" if prerelease else "false")


if __name__ == "__main__":
    main()
