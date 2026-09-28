#!/usr/bin/env python3
import argparse
import os
import pathlib
import re
import subprocess

SEMVER = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
CONVENTIONAL = re.compile(
    r"^(feat|fix|perf|revert|refactor|docs|test|build|ci|chore)"
    r"(?:\([^)]+\))?(!)?:\s+(.+)$"
)

CATEGORIES = [
    ("Breaking changes", lambda c: c["breaking"]),
    ("Features", lambda c: not c["breaking"] and c["type"] == "feat"),
    ("Fixes", lambda c: not c["breaking"] and c["type"] == "fix"),
    ("Performance", lambda c: not c["breaking"] and c["type"] == "perf"),
    ("Reverts", lambda c: not c["breaking"] and c["type"] == "revert"),
    ("Refactoring", lambda c: not c["breaking"] and c["type"] == "refactor"),
    ("Documentation", lambda c: not c["breaking"] and c["type"] == "docs"),
    ("Build", lambda c: not c["breaking"] and c["type"] == "build"),
    ("CI", lambda c: not c["breaking"] and c["type"] == "ci"),
    ("Tests", lambda c: not c["breaking"] and c["type"] == "test"),
    ("Chores", lambda c: not c["breaking"] and c["type"] == "chore"),
    ("Other changes", lambda c: not c["breaking"] and c["type"] is None),
]


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def write_output(path, name, value):
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(f"{name}={value}\n")


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--forced-bump",
        choices=("auto", "patch", "minor", "major"),
        default="auto",
    )
    parser.add_argument("--notes", required=True)
    parser.add_argument("--output")
    return parser.parse_args()


def main():
    args = parse_args()
    tags = git(
        "tag",
        "--list",
        "v[0-9]*.[0-9]*.[0-9]*",
        "--sort=-v:refname",
    ).splitlines()
    last_tag = next((tag for tag in tags if SEMVER.fullmatch(tag)), None)

    if last_tag:
        match = SEMVER.fullmatch(last_tag)
        current = tuple(int(part) for part in match.groups())
        revision_range = f"{last_tag}..HEAD"
    else:
        current = (0, 0, 0)
        revision_range = "HEAD"

    raw = subprocess.check_output(
        [
            "git",
            "log",
            "--no-merges",
            "--format=%H%x1f%s%x1f%b%x1e",
            revision_range,
        ],
        text=True,
    )

    commits = []
    for record in raw.split("\x1e"):
        record = record.strip("\n")
        if not record:
            continue
        fields = record.split("\x1f", 2)
        if len(fields) != 3:
            continue
        sha, subject, body = fields
        match = CONVENTIONAL.fullmatch(subject.strip())
        commit_type = match.group(1) if match else None
        breaking = bool(match and match.group(2)) or bool(
            re.search(r"(?im)^BREAKING[ -]CHANGE:\s*", body)
        )
        commits.append(
            {
                "sha": sha,
                "subject": subject.strip(),
                "type": commit_type,
                "breaking": breaking,
            }
        )

    if args.forced_bump != "auto":
        bump = args.forced_bump
    elif any(item["breaking"] for item in commits):
        bump = "major"
    elif any(item["type"] == "feat" for item in commits):
        bump = "minor"
    elif any(item["type"] in {"fix", "perf", "revert"} for item in commits):
        bump = "patch"
    else:
        print("No release-worthy Conventional Commit since the latest SemVer tag.")
        write_output(args.output, "release", "false")
        return

    major, minor, patch = current
    if bump == "major":
        major, minor, patch = major + 1, 0, 0
    elif bump == "minor":
        minor, patch = minor + 1, 0
    else:
        patch += 1

    tag = f"v{major}.{minor}.{patch}"
    repository = os.environ.get("GITHUB_REPOSITORY", "repository")

    lines = [
        f"# {tag}",
        "",
        f"Changes since {last_tag}." if last_tag else "Initial automated release.",
        "",
    ]
    for heading, predicate in CATEGORIES:
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

    print(f"Release bump: {bump}")
    print(f"Previous tag: {last_tag or 'none'}")
    print(f"Next tag: {tag}")
    write_output(args.output, "release", "true")
    write_output(args.output, "tag", tag)
    write_output(args.output, "bump", bump)


if __name__ == "__main__":
    main()
