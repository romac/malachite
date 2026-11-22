#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if (( $# == 0 )); then set -- --max-samples=100; fi
find . -name '*Test.qnt' | ../../../../scripts/quint-forall.sh test "$@"
