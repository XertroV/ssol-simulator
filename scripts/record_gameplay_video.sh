#!/usr/bin/env bash
# Record full 3D Open SSOL gameplay (window under Xvfb+lavapipe) driven by a policy,
# prepend a 2s title card, write to data/videos/.
#
# Title card style (approx):
#   full-frame black backdrop; centered text on 50%-opacity black padded box
#
# Usage:
#   bash scripts/record_gameplay_video.sh \
#     --model data/sac_corrective/n7_mix_80k \
#     --num-orbs 7 --route wr --seed 0 \
#     --out data/videos/proof_wr7_3d.mp4
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODEL=""
NUM_ORBS=7
ROUTE=wr
SEED=0
OUT="data/videos/proof_gameplay.mp4"
SECS=90
SPEED="${SPEED:-1.5}"
TITLE=""
BC="${BC:-data/bc_policy.pt}"
BIN="${BIN:-target/release/ssol_simulator}"
PY="${PY:-python/.venv/bin/python}"
[[ -x "$PY" ]] || PY="python/venv/bin/python"
POLICY="${POLICY:-sac}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model) MODEL="$2"; shift 2 ;;
    --num-orbs) NUM_ORBS="$2"; shift 2 ;;
    --route) ROUTE="$2"; shift 2 ;;
    --seed) SEED="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --secs) SECS="$2"; shift 2 ;;
    --speed) SPEED="$2"; shift 2 ;;
    --title) TITLE="$2"; shift 2 ;;
    --policy) POLICY="$2"; shift 2 ;;
    --bc) BC="$2"; shift 2 ;;
    *) echo "unknown arg: $1"; exit 2 ;;
  esac
done

if [[ -z "$MODEL" || ! -f "$MODEL/sac_model.zip" ]]; then
  echo "Need --model DIR containing sac_model.zip"
  exit 1
fi
if [[ ! -x "$BIN" ]]; then
  echo "Missing binary $BIN"
  exit 1
fi

mkdir -p "$(dirname "$OUT")" data/videos
WORK="$(dirname "$OUT")/gameplay_work_$$"
mkdir -p "$WORK"
# Ensure assets visible next to release binary (Bevy resolves some paths from exe dir)
if [[ ! -e target/release/assets ]]; then
  ln -sfn "$ROOT/assets" target/release/assets
fi

LVP="$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n1 || true)"
if [[ -z "$LVP" ]]; then
  echo "error: lavapipe ICD not found (install mesa-vulkan-drivers)"
  exit 1
fi

# Pick a free display number
DISP_NUM=$(( 90 + RANDOM % 50 ))
export DISPLAY=":${DISP_NUM}"
export VK_ICD_FILENAMES="$LVP"
export BEVY_ASSET_ROOT="$ROOT"
export PYTHONUNBUFFERED=1

# Clean any leftover Xvfb on this number
for p in $(pgrep -x Xvfb || true); do
  if tr '\0' ' ' < /proc/$p/cmdline 2>/dev/null | grep -q ":${DISP_NUM}"; then
    kill "$p" 2>/dev/null || true
  fi
done
sleep 0.3

Xvfb ":${DISP_NUM}" -screen 0 1280x720x24 +extension GLX \
  >"$WORK/xvfb.log" 2>&1 &
XPID=$!
echo "$XPID" >"$WORK/xvfb.pid"
sleep 1
if ! kill -0 "$XPID" 2>/dev/null; then
  echo "Xvfb failed to start"; cat "$WORK/xvfb.log"; exit 1
fi

cleanup() {
  if [[ -n "${FPID:-}" ]]; then kill "$FPID" 2>/dev/null || true; fi
  if [[ -n "${XPID:-}" ]]; then kill "$XPID" 2>/dev/null || true; fi
}
trap cleanup EXIT

# Start x11grab slightly before the game so we don't miss the first frames
RAW="$WORK/gameplay_raw.mp4"
ffmpeg -y -f x11grab -video_size 1280x720 -framerate 30 -i "${DISPLAY}.0" \
  -c:v libx264 -pix_fmt yuv420p -preset ultrafast \
  "$RAW" </dev/null >"$WORK/ffmpeg.log" 2>&1 &
FPID=$!
sleep 1

if [[ -z "$TITLE" ]]; then
  TITLE="Open SSOL  ·  ${NUM_ORBS} orbs  ·  route=${ROUTE}  ·  seed=${SEED}"
fi

echo "=== 3D gameplay record $(date -Is) display=$DISPLAY model=$MODEL ==="
set +e
PYTHONPATH=python/src "$PY" -u -m ssol_training.phase1_eval \
  --sim-bin "$BIN" \
  --policy "$POLICY" \
  --sac-model "$MODEL/sac_model.zip" \
  --vecnormalize "$MODEL/vecnormalize.pkl" \
  --bc-policy "$BC" \
  --num-orbs "$NUM_ORBS" \
  --routes "$ROUTE" \
  --seeds "$SEED" \
  --speed "$SPEED" \
  --max-episode-secs "$SECS" \
  --early-fail-after 0 \
  --windowed \
  --out "$WORK/eval" \
  >"$WORK/eval.log" 2>&1
EVAL_RC=$?
set -e

# Stop capture
sleep 0.5
kill -INT "$FPID" 2>/dev/null || true
wait "$FPID" 2>/dev/null || true
FPID=""

# Require successful episode
python3 - "$WORK/eval/summary.json" <<'PY' || { echo "Episode not successful — not writing proof video"; exit 1; }
import json,sys
from pathlib import Path
p=Path(sys.argv[1])
if not p.is_file():
    sys.exit(1)
s=json.load(open(p))
ok=s.get("successes",0)>0 or s.get("overall_success_rate",0)>0
for r in s.get("by_route",{}).values():
    if r.get("successes",0)>0: ok=True
sys.exit(0 if ok else 1)
PY

# Build 2s title card: black frame + 50% opacity black box with padded white text
TITLE_CARD="$WORK/title.mp4"
# Escape drawtext special chars
TITLE_ESC=$(printf '%s' "$TITLE" | sed 's/:/\\:/g; s/'"'"'/\\'"'"'/g')
ffmpeg -y -f lavfi -i "color=c=black:s=1280x720:d=2" \
  -vf "drawtext=text='${TITLE_ESC}':fontcolor=white:fontsize=32:x=(w-text_w)/2:y=(h-text_h)/2:box=1:boxcolor=black@0.5:boxborderw=48" \
  -c:v libx264 -pix_fmt yuv420p -t 2 "$TITLE_CARD" >"$WORK/title_ff.log" 2>&1 \
  || ffmpeg -y -f lavfi -i "color=c=black:s=1280x720:d=2" -c:v libx264 -pix_fmt yuv420p "$TITLE_CARD"

# Concat title + gameplay
LIST="$WORK/concat.txt"
{
  echo "file '$TITLE_CARD'"
  echo "file '$RAW'"
} >"$LIST"
ffmpeg -y -f concat -safe 0 -i "$LIST" -c copy "$OUT" >"$WORK/concat_ff.log" 2>&1 \
  || ffmpeg -y -i "$TITLE_CARD" -i "$RAW" -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0" -c:v libx264 -pix_fmt yuv420p "$OUT"

# Sidecar
{
  echo "title=$TITLE"
  echo "model=$MODEL"
  echo "num_orbs=$NUM_ORBS route=$ROUTE seed=$SEED speed=$SPEED"
  echo "display=$DISPLAY lavapipe=$LVP"
  echo "eval_log=$WORK/eval.log"
  cat "$WORK/eval/episodes.jsonl" 2>/dev/null || true
} >"${OUT}.sidecar.txt"

ls -la "$OUT"
echo "=== wrote 3D gameplay proof $(date -Is) → $OUT ==="
