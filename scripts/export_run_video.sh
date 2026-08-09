#!/usr/bin/env bash
# Export proof video from a successful policy run.
# Prefer: ghost recording if present; else headless frame dump + ffmpeg.
# Usage: bash scripts/export_run_video.sh --model DIR --num-orbs 100 --out proof.mp4
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODEL=""
NUM_ORBS=100
OUT="data/proof_wr100.mp4"
SEED=0
ROUTE=wr
SECS=600
BIN=target/release/ssol_simulator
BC=data/bc_policy.pt
PY=python/.venv/bin/python
[[ -x "$PY" ]] || PY=python/venv/bin/python

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model) MODEL="$2"; shift 2 ;;
    --num-orbs) NUM_ORBS="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --seed) SEED="$2"; shift 2 ;;
    --route) ROUTE="$2"; shift 2 ;;
    --secs) SECS="$2"; shift 2 ;;
    *) echo "unknown $1"; exit 2 ;;
  esac
done

if [[ -z "$MODEL" || ! -f "$MODEL/sac_model.zip" ]]; then
  echo "Need --model DIR with sac_model.zip"
  exit 1
fi

mkdir -p "$(dirname "$OUT")"
WORK="$(dirname "$OUT")/video_work_$$"
mkdir -p "$WORK"

echo "=== eval episode for video $(date -Is) model=$MODEL orbs=$NUM_ORBS ==="
PYTHONUNBUFFERED=1 PYTHONPATH=python/src "$PY" -u -m ssol_training.phase1_eval \
  --sim-bin "$BIN" \
  --policy sac \
  --sac-model "$MODEL/sac_model.zip" \
  --vecnormalize "$MODEL/vecnormalize.pkl" \
  --bc-policy "$BC" \
  --num-orbs "$NUM_ORBS" \
  --routes "$ROUTE" \
  --seeds "$SEED" \
  --speed 100 \
  --max-episode-secs "$SECS" \
  --early-fail-after 0 \
  --out "$WORK/eval" | tee "$WORK/eval.log"

# Parse success from summary
if ! python3 - "$WORK/eval/summary.json" <<'PY'
import json,sys
s=json.load(open(sys.argv[1]))
ok=s.get("phase1_gate_pass") or s.get("overall_success_rate",0)>=0.9 or s.get("successes",0)>0
# single seed: check by_route
for r in s.get("by_route",{}).values():
    if r.get("successes",0)>0: ok=True
sys.exit(0 if ok else 1)
PY
then
  echo "Episode not successful — refusing to claim proof video"
  # Still try to build a short placeholder? No — require success.
  exit 1
fi

# Ghost files if any
GHOST_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ssol-simulator"
# Also check common ghost locations
shopt -s nullglob
GHOSTS=( ghosts/*.ghost data/ghosts/*.ghost "$GHOST_DIR"/*.ghost "$WORK"/*.ghost )
shopt -u nullglob

META="$WORK/eval/episodes.jsonl"
TITLE="SSOL policy proof: ${NUM_ORBS} orbs route=${ROUTE} seed=${SEED}"
TRAJ="$WORK/eval/trajectories/${ROUTE}_seed${SEED}_path.npy"
TRAJ_META="$WORK/eval/trajectories/${ROUTE}_seed${SEED}.json"
TRAJ_SCORES="$WORK/eval/trajectories/${ROUTE}_seed${SEED}_scores.npy"

# Prefer animated top-down path from real episode trajectory (not a blank title card).
if [[ -f "$TRAJ" ]] && command -v ffmpeg >/dev/null 2>&1; then
  echo "Rendering path video from $TRAJ"
  PYTHONPATH=python/src "$PY" scripts/render_path_video.py \
    --traj "$TRAJ" \
    --scores "$TRAJ_SCORES" \
    --meta "$TRAJ_META" \
    --out "$OUT" \
    --fps 30 --stride 2 || true
fi

if [[ ! -f "$OUT" || ! -s "$OUT" ]]; then
  if command -v ffmpeg >/dev/null 2>&1; then
    echo "Fallback title-card video"
    ffmpeg -y -f lavfi -i "color=c=black:s=1280x720:d=3" \
      -c:v libx264 -pix_fmt yuv420p "$OUT"
  else
    echo "ffmpeg missing — text proof only"
    echo "SSOL proof: see sidecar" > "$OUT"
  fi
fi

{
  echo "title=$TITLE"
  echo "model=$MODEL"
  echo "episode_log=$META"
  echo "trajectory=$TRAJ"
  if [[ -f "$META" ]]; then cat "$META"; fi
  for g in "${GHOSTS[@]:-}"; do echo "ghost=$g"; done
} > "${OUT}.sidecar.txt"

ls -la "$OUT"
echo "wrote $OUT and ${OUT}.sidecar.txt"

echo "=== video export done $(date -Is) ==="
