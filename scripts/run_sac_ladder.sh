#!/usr/bin/env bash
# Residual SAC curriculum ladder: 1 → 3 → 7 orbs with parallel envs.
# Logs to data/sac_ladder/ladder.log
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

N_ENVS="${N_ENVS:-4}"
SPEED="${SPEED:-100}"
BC="${BC:-data/bc_policy.pt}"
BIN="${BIN:-target/release/ssol_simulator}"
OUT_ROOT="${OUT_ROOT:-data/sac_ladder}"
PY="${PY:-python/venv/bin/python}"
if [[ ! -x "$PY" ]]; then
  PY="python/.venv/bin/python"
fi

mkdir -p "$OUT_ROOT"
LOG="$OUT_ROOT/ladder.log"
exec > >(tee -a "$LOG") 2>&1

echo "=== SAC ladder start $(date -Is) n_envs=$N_ENVS speed=$SPEED ==="
if [[ ! -x "$BIN" ]]; then
  cargo build --release
fi
if [[ ! -f "$BC" ]]; then
  echo "Missing BC policy $BC — run just bc-train first"
  exit 1
fi

run_stage() {
  local n="$1" steps="$2" route="$3" tag="$4"
  local out="$OUT_ROOT/${tag}"
  echo ""
  echo "=== Stage $tag: num_orbs=$n steps=$steps route=$route $(date -Is) ==="
  PYTHONPATH=python/src "$PY" -m ssol_training.phase1_sac \
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
  echo "=== Stage $tag done $(date -Is) → $out ==="
}

# Stage A: master 1-orb go-to + arch
run_stage 1 30000 wr n1_wr_30k

# Stage B: short multi-goal, mixed routes
run_stage 3 100000 mix n3_mix_100k

# Stage C: Phase 1 gate difficulty
run_stage 7 300000 mix n7_mix_300k

echo "=== SAC ladder complete $(date -Is) ==="
