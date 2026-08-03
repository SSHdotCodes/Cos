#!/bin/zsh
set -euo pipefail

ROOT_DIR="${0:A:h:h}"
BUILD_DIR="/tmp/cos-debug-build"
swift build --package-path "$ROOT_DIR" --scratch-path "$BUILD_DIR"
exec "$BUILD_DIR/out/Products/Debug/Cos"
