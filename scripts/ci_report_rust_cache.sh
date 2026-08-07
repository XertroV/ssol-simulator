#!/usr/bin/env bash
# Print cache / target stats after a CI build (best-effort).
set -euo pipefail

log() { printf '%s\n' "$*"; }

log "=== Rust cache report ==="
log "CARGO_HOME=${CARGO_HOME:-"(default)"}"
log "CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"(default target/)"}"
log "RUSTC_WRAPPER=${RUSTC_WRAPPER:-"(none)"}"
log "SCCACHE_DIR=${SCCACHE_DIR:-"(none)"}"
log "SCCACHE_BUCKET=${SCCACHE_BUCKET:-"(none)"}"

if [ -n "${CARGO_TARGET_DIR:-}" ] && [ -d "${CARGO_TARGET_DIR}" ]; then
  du -sh "${CARGO_TARGET_DIR}" || true
elif [ -d target ]; then
  du -sh target || true
fi

if [ -n "${CARGO_HOME:-}" ] && [ -d "${CARGO_HOME}" ]; then
  du -sh "${CARGO_HOME}" || true
fi

if command -v sccache >/dev/null 2>&1; then
  sccache --show-stats || true
fi

log "=== end cache report ==="
