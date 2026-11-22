#!/usr/bin/env bash
# Run the installed Quint CLI on one file per line of stdin.
set -u
if (( $# == 0 )); then
    echo "Usage: $0 <command> [options...]" >&2
    exit 1
fi
files=0
failed=()
while IFS= read -r file; do
    ((files += 1))
    printf 'Running quint %s %s\n' "$*" "$file"
    quint "$@" "$file" || failed+=("$file")
done
if (( files == 0 )); then
    echo "No Quint files supplied" >&2
    exit 1
fi
if (( ${#failed[@]} )); then
    printf 'Failed: %s\n' "${failed[@]}" >&2
    exit 1
fi
