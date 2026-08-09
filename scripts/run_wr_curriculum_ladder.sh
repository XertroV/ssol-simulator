#!/usr/bin/env bash
# Incremental WR truncated curriculum: 22 → 50 → 100 from a n7-capable checkpoint.
# Resume-safe: skips stages that already have sac_model.zip.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PYTHONUNBUFFERED=1

SRC="${SRC:-data/sac_corrective/n7_mix_80k}"
OUT_ROOT="${OUT_ROOT:-data/sac_ladder_ext}"
BC="${BC:-data/bc_policy.pt}"
BIN="${BIN:-target/release/ssol_simulator}"
N_ENVS="${N_ENVS:-4}"
SPEED="${SPEED:-100}"
PY="${PY:-python/.venv/bin/python}"
[[ -x "$PY" ]] || PY="python/venv/bin/python"

mkdir -p "$OUT_ROOT"
LOG="$OUT_ROOT/wr_ladder.log"
if command -v stdbuf >/dev/null 2>&1; then
  exec > >(stdbuf -oL -eL tee -a "$LOG") 2>&1
else
  exec > >(tee -a "$LOG") 2>&1
fi

echo "=== WR curriculum ladder start $(date -Is) src=$SRC ==="
echo "$SRC" >"$OUT_ROOT/.last_model_dir"

run_stage() {
  local n="$1" steps="$2" secs="$3" tag="$4"
  local out="$OUT_ROOT/${tag}"
  local load_dir
  load_dir="$(cat "$OUT_ROOT/.last_model_dir")"
  if [[ -f "$out/sac_model.zip" && "${FORCE:-0}" != "1" ]]; then
    echo "=== SKIP $tag (model exists) $(date -Is) ==="
    echo "$out" >"$OUT_ROOT/.last_model_dir"
    return 0
  fi
  echo "=== Stage $tag num_orbs=$n steps=$steps secs=$secs load=$load_dir $(date -Is) ==="
  PYTHONPATH=python/src "$PY" -u -m ssol_training.phase1_sac \
    --sim-bin "$BIN" \
    --bc-policy "$BC" \
    --load-model "$load_dir/sac_model.zip" \
    --load-vecnormalize "$load_dir/vecnormalize.pkl" \
    --num-orbs "$n" \
    --route-mode mix \
    --seed 0 \
    --max-episode-secs "$secs" \
    --act-hz 10 \
    --speed "$SPEED" \
    --timesteps "$steps" \
    --n-envs "$N_ENVS" \
    --learning-rate 1e-4 \
    --out "$out"
  echo "$out" >"$OUT_ROOT/.last_model_dir"
  echo "=== Stage $tag done $(date -Is) ==="
}

# If n22 already running/done as n22_mix_200k, continue from there
if [[ -f "$OUT_ROOT/n22_mix_200k/sac_model.zip" ]]; then
  echo "$OUT_ROOT/n22_mix_200k" >"$OUT_ROOT/.last_model_dir"
else
  run_stage 22 200000 120 n22_mix_200k
fi

run_stage 50 300000 180 n50_mix_300k
run_stage 100 400000 300 n100_mix_400k

echo "=== WR curriculum ladder complete $(date -Is) last=$(cat "$OUT_ROOT/.last_model_dir") ==="
