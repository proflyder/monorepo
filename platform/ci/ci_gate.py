#!/usr/bin/env python3
"""Decide whether the aggregate `ci-ok` job passes.

`ci-ok` is the single required status check. It must be green when every
dispatched job succeeded *or was legitimately skipped* (docs-only change, or
no unit of that platform was affected), and red for any failure or
cancellation.

Reads the `${{ toJSON(needs) }}` payload from the `NEEDS` environment variable.
"""
from __future__ import annotations

import json
import os
import sys

OK_RESULTS = {"success", "skipped"}


def evaluate(needs: dict[str, dict[str, str]]) -> tuple[bool, dict[str, str], dict[str, str]]:
    results = {name: value.get("result", "missing") for name, value in needs.items()}
    failing = {name: result for name, result in results.items() if result not in OK_RESULTS}
    return (not failing and bool(results)), results, failing


def main() -> int:
    raw = os.environ.get("NEEDS", "")
    if not raw:
        print("NEEDS is empty: ci-ok must depend on every workflow job", file=sys.stderr)
        return 1
    try:
        needs = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(f"cannot parse NEEDS: {exc}", file=sys.stderr)
        return 1

    ok, results, failing = evaluate(needs)
    print("job results: " + json.dumps(results, indent=2, sort_keys=True))
    if not ok:
        print("failing jobs: " + json.dumps(failing, sort_keys=True), file=sys.stderr)
        return 1
    print("all jobs succeeded or were legitimately skipped")
    return 0


if __name__ == "__main__":
    sys.exit(main())
