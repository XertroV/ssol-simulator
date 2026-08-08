#!/usr/bin/env bash
# Collect multi-route scripted demos for Phase 1 BC.
# Usage: scripts/collect_demos.sh [out_dir]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT_DIR="${1:-data/demos}"
mkdir -p "$OUT_DIR"

BIN="${BIN:-target/release/ssol_simulator}"
if [[ ! -x "$BIN" ]]; then
  echo "Building release binary..."
  cargo build --release
  BIN="target/release/ssol_simulator"
fi

SECS="${SECS:-75}"
SPEED="${SPEED:-200}"
# modes × orbs × seeds — keep wall time reasonable
MODES="${MODES:-wr greedy mix}"
ORBS="${ORBS:-1 3 7}"
SEEDS="${SEEDS:-0 1 2 3 4}"

echo "Collecting demos → $OUT_DIR (secs=$SECS speed=$SPEED)"
for mode in $MODES; do
  for n in $ORBS; do
    for seed in $SEEDS; do
      out="$OUT_DIR/${mode}_n${n}_s${seed}.jsonl"
      echo "=== $mode n=$n seed=$seed → $out"
      "$BIN" --headless --no-audio --speed "$SPEED" \
        --scripted-baseline --num-orbs "$n" --route-mode "$mode" --seed "$seed" \
        --act-hz 10 --max-episode-secs "$SECS" --num-episodes 1 \
        --dump-transitions "$out" \
        2> >(grep -E 'TRAIN_METRICS|episode.*done|error' >&2 || true) \
        | grep TRAIN_METRICS || true
    done
  done
done

# Merge
MERGED="$OUT_DIR/all_merged.jsonl"
: > "$MERGED"
for f in "$OUT_DIR"/*.jsonl; do
  [[ "$(basename "$f")" == "all_merged.jsonl" ]] && continue
  cat "$f" >> "$MERGED"
done
lines=$(wc -l < "$MERGED" | tr -d ' ')
echo "Merged $lines transitions → $MERGED"
