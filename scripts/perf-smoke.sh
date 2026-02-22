#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run_check() {
  local label="$1"
  shift

  local start=$SECONDS
  echo "==> ${label}"
  "$@"
  local elapsed=$((SECONDS - start))
  echo "<== ${label} (${elapsed}s)"
  echo
}

run_check "Agent loop regression" cargo test -q run_agent_loop
run_check "Shared tool strategy regression" cargo test -q execute_tool_uses_with_strategy
run_check "Context compaction regression" cargo test -q context::store
run_check "Background compaction startup regression" cargo test -q test_session_start_background_compaction

echo "Perf smoke complete."
