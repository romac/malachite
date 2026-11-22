#!/usr/bin/env bash
# Save the first trace reaching a declared observation. Quint returns nonzero on a miss.
set -euo pipefail
spec=${1:?usage: gen-traces.sh SPEC OBSERVATION [MAX_STEPS]}
goal=${2:?missing observation name}
steps=${3:-24}
module=$(basename "$spec" .qnt)
dir="traces/$module"
mkdir -p "$dir"
# Keep earlier traces even if several searches run at the same time.
run_dir=$(mktemp -d "$dir/$goal.XXXXXXXX")
trace="$run_dir/trace.itf.json"
if quint run "$spec" --backend="${QUINT_BACKEND:-rust}" --seed="${QUINT_SEED:-118}" \
    --observation="$goal" --max-steps="$steps" --max-samples=512 --out-itf="$trace" --verbosity=1; then
    test -s "$trace"
    printf 'Trace: %s\n' "$trace"
else
    status=$?
    # Keep any counterexample emitted for an invariant violation.
    rmdir "$run_dir" 2>/dev/null || true
    exit "$status"
fi
