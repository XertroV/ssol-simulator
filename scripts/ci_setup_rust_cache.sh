#!/usr/bin/env bash
# Configure Rust build caching for CI.
#
# GitHub.com: no-op here — workflows use Swatinem/rust-cache against the GHA cache API.
# Forgejo / self-hosted: prefer a host bind-mount at /cache (act_runner), optionally
# fall back to sccache with an S3-compatible backend (TrueNAS MinIO).
#
# Usage: scripts/ci_setup_rust_cache.sh <rustc-target-triple>
set -euo pipefail

TARGET="${1:?usage: $0 <target-triple>}"
SERVER_URL="${GITHUB_SERVER_URL:-}"

log() { printf '%s\n' "$*"; }
warn() { printf '::warning::%s\n' "$*"; }

if [ "${SERVER_URL}" = "https://github.com" ]; then
  log "Host is GitHub.com — rust-cache action owns GHA cache; nothing to configure."
  exit 0
fi

# --- Forgejo / self-hosted ----------------------------------------------------

install_sccache() {
  if command -v sccache >/dev/null 2>&1; then
    sccache --version
    return 0
  fi
  local ver="v0.10.0"
  local arch
  arch="$(uname -m)"
  case "${arch}" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)
      warn "No sccache binary for arch=${arch}; skipping sccache install."
      return 1
      ;;
  esac
  local url="https://github.com/mozilla/sccache/releases/download/${ver}/sccache-${ver}-${arch}-unknown-linux-musl.tar.gz"
  log "Installing sccache ${ver} (${arch}) from ${url}"
  curl -fsSL "${url}" | tar -xz -C /tmp
  if command -v sudo >/dev/null 2>&1; then
    sudo mv "/tmp/sccache-${ver}-${arch}-unknown-linux-musl/sccache" /usr/local/bin/sccache
  else
    mkdir -p "${HOME}/.local/bin"
    mv "/tmp/sccache-${ver}-${arch}-unknown-linux-musl/sccache" "${HOME}/.local/bin/sccache"
    echo "${HOME}/.local/bin" >> "${GITHUB_PATH}"
  fi
  chmod +x "$(command -v sccache)"
  sccache --version
}

configure_sccache_s3() {
  # Optional secrets/env (set in workflow env from repository secrets):
  #   CI_S3_ENDPOINT, CI_S3_BUCKET, CI_S3_ACCESS_KEY, CI_S3_SECRET_KEY, CI_S3_REGION
  if [ -z "${CI_S3_ENDPOINT:-}" ] || [ -z "${CI_S3_BUCKET:-}" ]; then
    return 1
  fi
  if ! install_sccache; then
    return 1
  fi

  {
    echo "RUSTC_WRAPPER=sccache"
    echo "SCCACHE_BUCKET=${CI_S3_BUCKET}"
    echo "SCCACHE_ENDPOINT=${CI_S3_ENDPOINT}"
    echo "SCCACHE_S3_USE_SSL=${CI_S3_USE_SSL:-true}"
    echo "SCCACHE_REGION=${CI_S3_REGION:-us-east-1}"
    # sccache reads standard AWS env for credentials
    echo "AWS_ACCESS_KEY_ID=${CI_S3_ACCESS_KEY:-}"
    echo "AWS_SECRET_ACCESS_KEY=${CI_S3_SECRET_KEY:-}"
    # Avoid mixing host-local sccache dir with S3 backend
    echo "SCCACHE_DIR="
  } >> "${GITHUB_ENV}"

  log "sccache configured for S3-compatible backend: endpoint=${CI_S3_ENDPOINT} bucket=${CI_S3_BUCKET}"
  return 0
}

CACHE_ROOT="${FORGEJO_CARGO_CACHE_ROOT:-/cache/ssol-simulator}"

if [ -d /cache ]; then
  mkdir -p "${CACHE_ROOT}/cargo" "${CACHE_ROOT}/target/${TARGET}" "${CACHE_ROOT}/sccache"
  {
    echo "CARGO_HOME=${CACHE_ROOT}/cargo"
    echo "CARGO_TARGET_DIR=${CACHE_ROOT}/target/${TARGET}"
  } >> "${GITHUB_ENV}"

  # Local sccache on the same mount accelerates rebuilds when target is cleaned.
  if install_sccache; then
    {
      echo "RUSTC_WRAPPER=sccache"
      echo "SCCACHE_DIR=${CACHE_ROOT}/sccache"
    } >> "${GITHUB_ENV}"
  fi

  log "Forgejo host cache mount active:"
  log "  CARGO_HOME=${CACHE_ROOT}/cargo"
  log "  CARGO_TARGET_DIR=${CACHE_ROOT}/target/${TARGET}"
  log "  SCCACHE_DIR=${CACHE_ROOT}/sccache (if sccache installed)"
  du -sh "${CACHE_ROOT}/cargo" "${CACHE_ROOT}/target/${TARGET}" "${CACHE_ROOT}/sccache" 2>/dev/null || true

  # S3 is optional extra when /cache exists (usually unnecessary).
  exit 0
fi

warn "No /cache mount in the job container — Bevy will cold-build every run (~10–20m)."
warn "Preferred fix on TrueNAS: bind-mount a dataset to /cache in act_runner (see RELEASING.md)."

if configure_sccache_s3; then
  log "Using sccache + S3/MinIO as fallback (no host /cache mount)."
  exit 0
fi

warn "No CI_S3_ENDPOINT/CI_S3_BUCKET either — builds will not reuse work across jobs."
exit 0
