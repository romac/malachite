#!/usr/bin/env bash
# Replay the same public driver inputs against two committed revisions.
# Run from any checkout containing the fixture: scripts/compare-driver.sh BASE [CANDIDATE]
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: $0 BASE [CANDIDATE]" >&2
    exit 2
fi

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
base=$(git -C "$repo" rev-parse --verify --end-of-options "$1^{commit}")
candidate=$(git -C "$repo" rev-parse --verify --end-of-options "${2:-HEAD}^{commit}")
replay_dir=$(mktemp -d)
trap 'rm -rf -- "$replay_dir"' EXIT
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/code/target}"
if [[ "$CARGO_TARGET_DIR" != /* ]]; then
    CARGO_TARGET_DIR="$PWD/$CARGO_TARGET_DIR"
fi

record() {
    local revision=$1 checkout="$replay_dir/$2"
    mkdir "$checkout"
    git -C "$repo" archive "$revision" | tar -x -C "$checkout"
    local tests=code/crates/core-driver/tests
    cp "$repo/$tests/rule_replay.rs" "$checkout/$tests/rule_replay.rs"
    cp "$repo/$tests/it/utils.rs" "$checkout/$tests/it/utils.rs"
    (
        cd "$checkout/code"
        MALACHITE_DRIVER_TRACE="$replay_dir/$2.trace" cargo test --locked \
            -p arc-malachitebft-core-driver --all-features --test rule_replay -- --ignored
    )
}

record "$base" base
record "$candidate" candidate
python3 - "$replay_dir/base.trace" "$replay_dir/candidate.trace" <<'PY'
from itertools import zip_longest
from sys import argv

schedules = inputs = 0
schedule = event = ""
with open(argv[1]) as old, open(argv[2]) as new:
    for before, after in zip_longest(old, new, fillvalue="<end of trace>"):
        if before.startswith("schedule "):
            schedules += 1
            schedule = before.strip()
        if before.startswith("input "):
            inputs += 1
            event = before.strip()
        if before != after:
            raise SystemExit(f"{schedule}\n{event}\nBASE      {before.rstrip()}\nCANDIDATE {after.rstrip()}")
if not schedules or not inputs:
    raise SystemExit("Empty driver replay")
print(f"Matched {schedules:,} schedules and {inputs:,} driver inputs.")
PY
