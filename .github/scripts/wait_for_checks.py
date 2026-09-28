#!/usr/bin/env python3
import argparse
import json
import os
import time
import urllib.request


PASS = {"success", "neutral", "skipped"}
FAIL = {"failure", "cancelled", "timed_out", "action_required", "stale", "startup_failure"}


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--run-id", required=True)
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


def main():
    args = parse_args()
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
        checks = [
            item for item in checks_data.get("check_runs", [])
            if own_run_fragment not in (item.get("details_url") or "")
        ]

        status_data = api(args.repository, f"/commits/{args.sha}/status")
        statuses = status_data.get("statuses", [])

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
            if item.get("state") not in {"success"}
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
                item.get("context", ""),
                item.get("state", ""),
                item.get("target_url", ""),
                "",
            )
            for item in statuses
        ))

        if now < grace_until or pending_checks or pending_statuses:
            stable_since = None
        elif signature != last_signature:
            stable_since = now
        elif stable_since is not None and now - stable_since >= args.settle:
            print(
                f"Repository checks passed: {len(checks)} check-runs, "
                f"{len(statuses)} status contexts."
            )
            return

        last_signature = signature
        waiting = [
            item.get("name", "check") for item in pending_checks
        ] + [
            item.get("context", "status") for item in pending_statuses
        ]
        if waiting:
            print("Waiting for checks: " + ", ".join(waiting))
        else:
            print("Checks complete; waiting for registration set to stabilize.")
        time.sleep(args.poll)


if __name__ == "__main__":
    main()
