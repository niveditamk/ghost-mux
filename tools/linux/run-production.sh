#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BIN_NAME="$(awk -F'"' '/^name[[:space:]]*=[[:space:]]*"/ { print $2; exit }' "$PROJECT_ROOT/Cargo.toml")"
if [[ -z "$BIN_NAME" ]]; then
  echo "error: unable to read binary name from Cargo.toml" >&2
  exit 1
fi

APP_DIR="$PROJECT_ROOT/dist/$BIN_NAME"
BIN_PATH="$APP_DIR/$BIN_NAME"
RUNNER_PATH="$APP_DIR/run.sh"

if [[ "$(uname -s)" == CYGWIN* ]] || [[ "$(uname -s)" == MINGW* ]] || [[ "$(uname -s)" == MSYS* ]]; then
  BIN_PATH="$BIN_PATH.exe"
fi

if [[ -x "$RUNNER_PATH" ]]; then
  exec "$RUNNER_PATH" "$@"
fi

if [[ ! -f "$BIN_PATH" ]]; then
  echo "error: binary not found at $BIN_PATH. Build first with ./tools/linux/build-production.sh" >&2
  exit 1
fi

exec "$BIN_PATH" "$@"
