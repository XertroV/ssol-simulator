#!/usr/bin/env bash
# Publish best-run videos into data/videos/<run_id>/ for easy monitoring.
#
# Run id format: YYYYMMDD_HHMMSS_<tag>_n{orbs}_s{seed}
#
# Usage:
#   bash scripts/publish_run_videos.sh --tag n7_wr --model data/sac_corrective/n7_mix_80k \
#     --num-orbs 7 --route wr --seed 0 --kinds path,3d
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TAG="run"
MODEL=""
NUM_ORBS=7
ROUTE=wr
SEED=0
KINDS="path,3d"   # path | 3d | both
SECS=90
SPEED_3D="${SPEED_3D:-2}"
CAPTURE_FPS="${CAPTURE_FPS:-60}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag) TAG="$2"; shift 2 ;;
    --model) MODEL="$2"; shift 2 ;;
    --num-orbs) NUM_ORBS="$2"; shift 2 ;;
    --route) ROUTE="$2"; shift 2 ;;
    --seed) SEED="$2"; shift 2 ;;
    --kinds) KINDS="$2"; shift 2 ;;
    --secs) SECS="$2"; shift 2 ;;
    --speed-3d) SPEED_3D="$2"; shift 2 ;;
    --fps) CAPTURE_FPS="$2"; shift 2 ;;
    *) echo "unknown $1"; exit 2 ;;
  esac
done

if [[ -z "$MODEL" ]]; then
  echo "Need --model DIR"
  exit 1
fi

TS="$(date +%Y%m%d_%H%M%S)"
RUN_ID="${TS}_${TAG}_n${NUM_ORBS}_${ROUTE}_s${SEED}"
OUT_DIR="data/videos/${RUN_ID}"
mkdir -p "$OUT_DIR"

{
  echo "run_id=$RUN_ID"
  echo "created=$(date -Is)"
  echo "model=$MODEL"
  echo "num_orbs=$NUM_ORBS route=$ROUTE seed=$SEED"
  echo "kinds=$KINDS"
  echo "host=$(hostname)"
} >"$OUT_DIR/RUN.txt"

echo "=== publish run $RUN_ID → $OUT_DIR ==="

if [[ "$KINDS" == *path* ]]; then
  echo "--- path video ---"
  bash scripts/export_run_video.sh \
    --model "$MODEL" \
    --num-orbs "$NUM_ORBS" \
    --route "$ROUTE" \
    --seed "$SEED" \
    --secs "$SECS" \
    --out "$OUT_DIR/path.mp4" || echo "path export failed (non-fatal)"
  # Also keep a stable latest symlink-style copy
  if [[ -f "$OUT_DIR/path.mp4" ]]; then
    cp -f "$OUT_DIR/path.mp4" "data/videos/latest_path.mp4"
    cp -f "$OUT_DIR/path.mp4.sidecar.txt" "data/videos/latest_path.mp4.sidecar.txt" 2>/dev/null || true
  fi
fi

if [[ "$KINDS" == *3d* ]]; then
  echo "--- 3d gameplay video (fps=$CAPTURE_FPS) ---"
  CAPTURE_FPS="$CAPTURE_FPS" bash scripts/record_gameplay_video.sh \
    --model "$MODEL" \
    --num-orbs "$NUM_ORBS" \
    --route "$ROUTE" \
    --seed "$SEED" \
    --secs "$SECS" \
    --speed "$SPEED_3D" \
    --title "Open SSOL · ${TAG} · ${NUM_ORBS} orbs · ${ROUTE} · seed ${SEED}" \
    --out "$OUT_DIR/gameplay_3d.mp4" || echo "3d export failed (non-fatal)"
  if [[ -f "$OUT_DIR/gameplay_3d.mp4" ]]; then
    cp -f "$OUT_DIR/gameplay_3d.mp4" "data/videos/latest_3d.mp4"
    cp -f "$OUT_DIR/gameplay_3d.mp4.sidecar.txt" "data/videos/latest_3d.mp4.sidecar.txt" 2>/dev/null || true
  fi
fi

# Index
{
  echo "# Run $RUN_ID"
  echo
  echo "| file | size |"
  echo "| --- | ---: |"
  for f in "$OUT_DIR"/*; do
    [[ -f "$f" ]] || continue
    bn=$(basename "$f")
    sz=$(stat -c%s "$f")
    echo "| \`$bn\` | $sz |"
  done
} >"$OUT_DIR/INDEX.md"

# Update global latest index
{
  echo "# data/videos — latest runs"
  echo
  echo "Watch: \`watch -n 5 'ls -lht data/videos/*/'\`"
  echo
  ls -1dt data/videos/*/ 2>/dev/null | head -20 | while read -r d; do
    echo "- \`$(basename "$d")\`"
  done
} > data/videos/INDEX.md

ls -la "$OUT_DIR"
echo "=== published $OUT_DIR ==="
echo "Also: data/videos/latest_path.mp4  data/videos/latest_3d.mp4"
