#!/usr/bin/env bash
# Frozen Phase-1 gate eval: residual SAC @ 7 orbs, wr + greedy, 20 seeds.
# Logs to data/eval_n7_gate/eval.log ; episodes JSONL + summary.json beside it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export PYTHONUNBUFFERED=1
export PYTHONIOENCODING=utf-8

SAC="${SAC:-data/sac_ladder/n7_mix_300k/sac_model.zip}"
VEC="${VEC:-data/sac_ladder/n7_mix_300k/vecnormalize.pkl}"
BC="${BC:-data/bc_policy.pt}"
BIN="${BIN:-target/release/ssol_simulator}"
OUT="${OUT:-data/eval_n7_gate}"
SEEDS="${SEEDS:-0-19}"
ROUTES="${ROUTES:-wr greedy}"
ORBS="${ORBS:-7}"
SPEED="${SPEED:-200}"
SECS="${SECS:-60}"
POLICY="${POLICY:-sac}"
# Optional early abort if first N eps of a route are terrible
EARLY_AFTER="${EARLY_AFTER:-6}"
EARLY_RATE="${EARLY_RATE:-0.15}"

PY="${PY:-python/venv/bin/python}"
if [[ ! -x "$PY" ]]; then
  PY="python/.venv/bin/python"
fi

mkdir -p "$OUT"
LOG="$OUT/eval.log"
if command -v stdbuf >/dev/null 2>&1; then
  exec > >(stdbuf -oL -eL tee -a "$LOG") 2>&1
else
  exec > >(tee -a "$LOG") 2>&1
fi

echo "=== phase1 eval start $(date -Is) host=$(hostname) policy=$POLICY orbs=$ORBS ==="
echo "sac=$SAC vec=$VEC bc=$BC seeds=$SEEDS routes=$ROUTES speed=$SPEED"

if [[ ! -x "$BIN" ]]; then
  cargo build --release
fi
if [[ "$POLICY" == "sac" && ! -f "$SAC" ]]; then
  echo "Missing SAC model $SAC"
  exit 1
fi
if [[ ! -f "$BC" && "$POLICY" != "zero" ]]; then
  echo "WARN: missing BC $BC"
fi

# shellcheck disable=SC2086
PYTHONUNBUFFERED=1 PYTHONPATH=python/src "$PY" -u -m ssol_training.phase1_eval \
  --sim-bin "$BIN" \
  --policy "$POLICY" \
  --sac-model "$SAC" \
  --vecnormalize "$VEC" \
  --bc-policy "$BC" \
  --num-orbs "$ORBS" \
  --routes $ROUTES \
  --seeds "$SEEDS" \
  --speed "$SPEED" \
  --max-episode-secs "$SECS" \
  --early-fail-after "$EARLY_AFTER" \
  --early-fail-rate "$EARLY_RATE" \
  --out "$OUT"

echo "=== phase1 eval done $(date -Is) exit=$? ==="
