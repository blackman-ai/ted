#!/usr/bin/env bash
set -euo pipefail

# Basic runtime diagnostics for release artifacts.
# Usage: scripts/release-smoke.sh /path/to/ted

bin="${1:-ted}"

if [[ ! -x "$bin" ]]; then
  echo "release-smoke: binary not executable: $bin" >&2
  exit 1
fi

echo "release-smoke: checking $bin"

"$bin" --version
"$bin" --help >/dev/null
"$bin" chat --help >/dev/null
"$bin" ask --help >/dev/null
"$bin" settings --help >/dev/null
"$bin" history --help >/dev/null
"$bin" context --help >/dev/null
"$bin" caps --help >/dev/null

echo "release-smoke: ok"
