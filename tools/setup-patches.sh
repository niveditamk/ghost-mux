#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

GPUI_DIR="$PROJECT_ROOT/patches/gpui-component"
PATCH_FILE="$PROJECT_ROOT/patches/gpui-component.patch"
REPO_URL="https://github.com/longbridge/gpui-component.git"
COMMIT_SHA="196b9259b562c26be97c92f88c798bbeefa9cb3d"

if [ ! -d "$GPUI_DIR" ]; then
  echo "==> Cloning gpui-component from upstream..."
  git clone "$REPO_URL" "$GPUI_DIR"
  cd "$GPUI_DIR"
  echo "==> Checking out specific commit: $COMMIT_SHA..."
  git checkout "$COMMIT_SHA"
  echo "==> Applying patch gpui-component.patch..."
  git apply "$PATCH_FILE"
  echo "==> gpui-component setup successfully!"
else
  echo "==> gpui-component is already present."
fi
