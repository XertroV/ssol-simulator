#!/usr/bin/env bash
# Residual SAC curriculum ladder: 1 → 3 → 7 orbs with parallel envs.
# Logs to data/sac_ladder/ladder.log
#
# Resume-safe: stages with $OUT_ROOT/<tag>/sac_model.zip are skipped.
# Force re-run a stage: FORCE=1 bash scripts/run_sac_ladder.sh
# Or delete that stage dir.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Live line-buffered logs (tee still works; Python dumps flush each update).
export PYTHONUNBUFFERED=1
export PYTHONIOENCODING=utf-8

N_ENVS="${N_ENVS:-4}"
SPEED="${SPEED:-100}"
BC="${BC:-data/bc_policy.pt}"
BIN="${BIN:-target/release/ssol_simulator}"
OUT_ROOT="${OUT_ROOT:-data/sac_ladder}"
FORCE="${FORCE:-0}"
PY="${PY:-python/venv/bin/python}"
if [[ ! -x "$PY" ]]; then
  PY="python/.venv/bin/python"
fi

mkdir -p "$OUT_ROOT"
LOG="$OUT_ROOT/ladder.log"
# stdbuf -oL: line-buffer tee so PYTHONUNBUFFERED output hits the log promptly
if command -v stdbuf >/dev/null 2>&1; then
  exec > >(stdbuf -oL -eL tee -a "$LOG") 2>&1
else
  exec > >(tee -a "$LOG") 2>&1
fi

echo "=== SAC ladder start $(date -Is) host=$(hostname) n_envs=$N_ENVS speed=$SPEED force=$FORCE ==="
if [[ ! -x "$BIN" ]]; then
  cargo build --release
fi
if [[ ! -f "$BC" ]]; then
  echo "Missing BC policy $BC — run just bc-train first"
  exit 1
fi

stage_done() {
  local tag="$1"
  [[ -f "$OUT_ROOT/${tag}/sac_model.zip" ]]
}

run_stage() {
  local n="$1" steps="$2" route="$3" tag="$4"
  local out="$OUT_ROOT/${tag}"
  if [[ "$FORCE" != "1" ]] && stage_done "$tag"; then
    echo "=== Stage $tag: SKIP (found $out/sac_model.zip) $(date -Is) ==="
    return 0
  fi
  echo ""
  echo "=== Stage $tag: num_orbs=$n steps=$steps route=$route $(date -Is) ==="
  PYTHONUNBUFFERED=1 PYTHONPATH=python/src "$PY" -u -m ssol_training.phase1_sac \
    --sim-bin "$BIN" \
    --bc-policy "$BC" \
    --num-orbs "$n" \
    --route-mode "$route" \
    --max-episode-secs 60 \
    --act-hz 10 \
    --speed "$SPEED" \
    --timesteps "$steps" \
    --n-envs "$N_ENVS" \
    --seed 0 \
    --out "$out"
  if [[ ! -f "$out/sac_model.zip" ]]; then
    echo "ERROR: stage $tag finished without $out/sac_model.zip $(date -Is)" >&2
    exit 1
  fi
  echo "=== Stage $tag done $(date -Is) → $out ==="
}

# Stage A: master 1-orb go-to + arch
run_stage 1 30000 wr n1_wr_30k

# Stage B: short multi-goal, mixed routes
run_stage 3 100000 mix n3_mix_100k

# Stage C: Phase 1 gate difficulty
run_stage 7 300000 mix n7_mix_300k

echo "=== SAC ladder complete $(date -Is) host=$(hostname) ==="
