#!/usr/bin/env python3
import argparse
import json
import os
import pathlib
import time
import urllib.request

PASS = {"success", "neutral", "skipped"}
FAIL = {"failure", "cancelled", "timed_out", "action_required", "stale", "startup_failure"}
IGNORED_CHECK_NAMES = {"Semantic release", "Validate semantic release"}


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument(
        "--required-checks-file",
        default=".github/release-required-checks",
    )
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument("--registration-grace", type=int, default=30)
    parser.add_argument("--settle", type=int, default=30)
    parser.add_argument("--poll", type=int, default=10)
    return parser.parse_args()


def api(repository, path):
    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN is required.")
    request = urllib.request.Request(
        f"https://api.github.com/repos/{repository}{path}",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "repository-release-gate",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def load_required(path):
    config = pathlib.Path(path)
    if not config.exists():
        raise SystemExit(f"Required-check configuration is missing: {config}")
    names = {
        line.strip()
        for line in config.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }
    if not names:
        raise SystemExit(f"Required-check configuration is empty: {config}")
    return names


def latest_by_name(items, key):
    latest = {}
    for item in items:
        name = item.get(key, "")
        if not name:
            continue
        current = latest.get(name)
        if current is None or int(item.get("id") or 0) > int(current.get("id") or 0):
            latest[name] = item
    return latest


def main():
    args = parse_args()
    required = load_required(args.required_checks_file)
    started = time.monotonic()
    grace_until = started + args.registration_grace
    stable_since = None
    last_signature = None
    own_run_fragment = f"/actions/runs/{args.run_id}/"

    while True:
        now = time.monotonic()
        if now - started > args.timeout:
            raise SystemExit("Timed out waiting for repository checks.")

        checks_data = api(
            args.repository,
            f"/commits/{args.sha}/check-runs?per_page=100",
        )
        raw_checks = [
            item for item in checks_data.get("check_runs", [])
            if own_run_fragment not in (item.get("details_url") or "")
            and item.get("name") not in IGNORED_CHECK_NAMES
        ]
        checks_by_name = latest_by_name(raw_checks, "name")
        checks = list(checks_by_name.values())

        status_data = api(args.repository, f"/commits/{args.sha}/status")
        raw_statuses = status_data.get("statuses", [])
        statuses_by_name = latest_by_name(raw_statuses, "context")
        statuses = list(statuses_by_name.values())

        observed = set(checks_by_name) | set(statuses_by_name)
        missing_required = sorted(required - observed)

        failed_checks = [
            item for item in checks
            if item.get("status") == "completed"
            and item.get("conclusion") in FAIL
        ]
        failed_statuses = [
            item for item in statuses
            if item.get("state") in {"failure", "error"}
        ]
        if failed_checks or failed_statuses:
            names = [item.get("name", "check") for item in failed_checks]
            names += [item.get("context", "status") for item in failed_statuses]
            raise SystemExit("Release blocked by failed checks: " + ", ".join(names))

        pending_checks = [
            item for item in checks
            if item.get("status") != "completed"
            or item.get("conclusion") not in PASS
        ]
        pending_statuses = [
            item for item in statuses
            if item.get("state") != "success"
        ]

        signature = tuple(sorted(
            (
                str(item.get("id")),
                item.get("name", ""),
                item.get("status", ""),
                str(item.get("conclusion")),
            )
            for item in checks
        )) + tuple(sorted(
            (
                str(item.get("id")),
                item.get("context", ""),
                item.get("state", ""),
                item.get("target_url", ""),
            )
            for item in statuses
        ))

        waiting = [
            item.get("name", "check") for item in pending_checks
        ] + [
            item.get("context", "status") for item in pending_statuses
        ] + [
            f"{name} (not observed)" for name in missing_required
        ]

        if now < grace_until or waiting:
            stable_since = None
        else:
            if signature != last_signature or stable_since is None:
                stable_since = now
            elif now - stable_since >= args.settle:
                print(
                    f"Repository checks passed: {len(checks)} latest check-runs, "
                    f"{len(statuses)} latest status contexts; "
                    f"required checks observed: {', '.join(sorted(required))}."
                )
                return

        last_signature = signature
        if waiting:
            print("Waiting for checks: " + ", ".join(waiting))
        else:
            print("Checks complete; waiting for registration set to stabilize.")
        time.sleep(args.poll)


if __name__ == "__main__":
    main()
