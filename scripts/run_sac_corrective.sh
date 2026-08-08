#!/usr/bin/env bash
# Corrective fine-tune after Phase-1 gate fail (greedy weak / arch miss).
# Continues from n7_mix_300k with greedy-heavy + mix stages, then re-eval.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export PYTHONUNBUFFERED=1
export PYTHONIOENCODING=utf-8

N_ENVS="${N_ENVS:-4}"
SPEED="${SPEED:-100}"
BC="${BC:-data/bc_policy.pt}"
BIN="${BIN:-target/release/ssol_simulator}"
SRC="${SRC:-data/sac_ladder/n7_mix_300k}"
OUT_ROOT="${OUT_ROOT:-data/sac_corrective}"
PY="${PY:-python/venv/bin/python}"
if [[ ! -x "$PY" ]]; then
  PY="python/.venv/bin/python"
fi

mkdir -p "$OUT_ROOT"
LOG="$OUT_ROOT/corrective.log"
if command -v stdbuf >/dev/null 2>&1; then
  exec > >(stdbuf -oL -eL tee -a "$LOG") 2>&1
else
  exec > >(tee -a "$LOG") 2>&1
fi

echo "=== SAC corrective start $(date -Is) host=$(hostname) src=$SRC n_envs=$N_ENVS ==="
if [[ ! -f "$SRC/sac_model.zip" ]]; then
  echo "Missing $SRC/sac_model.zip"
  exit 1
fi
if [[ ! -x "$BIN" ]]; then
  cargo build --release
fi

run_ft() {
  local n="$1" steps="$2" route="$3" tag="$4" lr="$5"
  local out="$OUT_ROOT/${tag}"
  local load_model load_vec
  # First stage loads SRC; later stages load previous out dir if present
  if [[ -f "$OUT_ROOT/.last_model_dir" ]]; then
    load_model="$(cat "$OUT_ROOT/.last_model_dir")/sac_model.zip"
    load_vec="$(cat "$OUT_ROOT/.last_model_dir")/vecnormalize.pkl"
  else
    load_model="$SRC/sac_model.zip"
    load_vec="$SRC/vecnormalize.pkl"
  fi
  if [[ -f "$out/sac_model.zip" && "${FORCE:-0}" != "1" ]]; then
    echo "=== Stage $tag: SKIP (found $out/sac_model.zip) $(date -Is) ==="
    echo "$out" >"$OUT_ROOT/.last_model_dir"
    return 0
  fi
  echo ""
  echo "=== Stage $tag: num_orbs=$n steps=$steps route=$route lr=$lr load=$load_model $(date -Is) ==="
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
    --seed 1 \
    --learning-rate "$lr" \
    --load-model "$load_model" \
    --load-vecnormalize "$load_vec" \
    --out "$out"
  echo "$out" >"$OUT_ROOT/.last_model_dir"
  echo "=== Stage $tag done $(date -Is) → $out ==="
}

# A: greedy specialization (main gap from gate: 60% greedy)
run_ft 7 150000 greedy n7_greedy_150k 1e-4

# B: mix polish so WR doesn't regress
run_ft 7 80000 mix n7_mix_80k 1e-4

echo "=== SAC corrective train complete $(date -Is) ==="
LAST="$(cat "$OUT_ROOT/.last_model_dir")"
echo "Last model: $LAST"

# Re-run Phase-1 gate on the corrected model
export SAC="$LAST/sac_model.zip"
export VEC="$LAST/vecnormalize.pkl"
export OUT="${OUT_ROOT}/eval_gate"
export SEEDS="${SEEDS:-0-19}"
export ROUTES="wr greedy"
export ORBS=7
export SPEED=200
export EARLY_AFTER=0
bash scripts/run_phase1_eval.sh

echo "=== SAC corrective + re-eval complete $(date -Is) ==="
