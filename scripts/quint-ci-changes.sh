#!/usr/bin/env bash
# PRs and merge groups compare their base and head. Main pushes and manual runs always test.
set -euo pipefail

changed=true
if [[ -n "$BASE_SHA" ]]; then
    files=$(git diff --name-only "$BASE_SHA...$HEAD_SHA" -- "$@")
    [[ -n "$files" ]] || changed=false
fi
echo "changed=$changed" >> "$GITHUB_OUTPUT"
