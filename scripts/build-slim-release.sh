#!/usr/bin/env bash
# Build a slim, official-style release binary (release-lto profile).
#
# The daily selfdev build (used for hot-reload in this repo) is intentionally
# unoptimized (opt-level = 0, no LTO) so it compiles in seconds, but that makes
# the binary ~374MB. Official releases build with the `release-lto` profile
# (thin LTO, codegen-units = 16, no debug info), which typically halves the
# binary size (~154MB here) at the cost of a much slower build.
#
# This script is a thin convenience wrapper around that profile: it builds the
# slim binary into target/release-lto (via scripts/dev_cargo.sh so the embedded
# git hash matches HEAD) and reports where it is. It does NOT touch the dev
# selfdev build, the installed channels, or the launcher.
#
# Usage:
#   scripts/build-slim-release.sh              # build release-lto, print size
#   scripts/build-slim-release.sh --copy DEST  # build, then copy the binary to DEST
#
# To install the slim build through the normal channels instead (stable/current
# + launcher), use scripts/install_release.sh (which also defaults to release-lto).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

COPY_DEST=""
while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --copy)
      COPY_DEST="${2:?--copy requires a destination path}"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: $0 [--copy DEST]" >&2
      exit 1
      ;;
  esac
done

echo "▸ Building release-lto (thin LTO; this takes a few minutes)..."
JCODE_REMOTE_CARGO=0 scripts/dev_cargo.sh build --profile release-lto -p jcode --bin jcode

bin="$repo_root/target/release-lto/jcode"
[[ -f "$bin" ]] || { echo "Error: release-lto binary not found: $bin" >&2; exit 1; }

size_bytes="$(stat -c %s "$bin" 2>/dev/null || stat -f %z "$bin")"
size_mb="$(awk "BEGIN { printf \"%.1f\", $size_bytes/1048576 }")"
version="$("$bin" --version 2>/dev/null || echo "unknown")"

echo ""
echo "✅ Slim release binary built:"
echo "  path:    $bin"
echo "  size:    ${size_mb} MB"
echo "  version: $version"
echo ""
echo "  (daily selfdev build is unaffected and still used for hot-reload.)"

if [[ -n "$COPY_DEST" ]]; then
  mkdir -p "$(dirname "$COPY_DEST")"
  cp "$bin" "$COPY_DEST"
  echo "  copied to: $COPY_DEST"
fi
