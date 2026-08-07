#!/usr/bin/env bash
# Multi-seed scripted baseline matrix: route_mode × num_orbs × seeds.
#
# Emits one JSON object per episode (from TRAIN_METRICS_JSON lines) into an
# output JSONL file, then prints a short summary (median orbs, success rate).
#
# Usage:
#   scripts/train_baseline_matrix.sh
#   scripts/train_baseline_matrix.sh --out docs/baseline_matrix.jsonl
#   SECS=60 SPEED=200 SEEDS="0 1 2" MODES="wr greedy" ORBS="1 3 7" \
#     scripts/train_baseline_matrix.sh
#
# Requires a release build (built automatically via cargo run --release).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="${OUT:-docs/baseline_matrix.jsonl}"
SECS="${SECS:-60}"
SPEED="${SPEED:-200}"
ACT_HZ="${ACT_HZ:-10}"
# shellcheck disable=SC2206
MODES=(${MODES:-wr greedy})
# shellcheck disable=SC2206
ORBS=(${ORBS:-1 3 7})
# shellcheck disable=SC2206
SEEDS=(${SEEDS:-0 1 2})
# Prefer prebuilt release binary when present (skip cargo re-link per cell).
if [[ -z "${BIN:-}" && -x "$ROOT/target/release/ssol_simulator" ]]; then
  BIN="$ROOT/target/release/ssol_simulator"
else
  BIN="${BIN:-}"
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --secs) SECS="$2"; shift 2 ;;
    --speed) SPEED="$2"; shift 2 ;;
    --modes) MODES=($2); shift 2 ;;
    --orbs) ORBS=($2); shift 2 ;;
    --seeds) SEEDS=($2); shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,20p' "$0"
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$(dirname "$OUT")"
: >"$OUT"

run_one() {
  local mode="$1" n="$2" seed="$3"
  local log
  log="$(mktemp)"
  set +e
  if [[ -n "$BIN" ]]; then
    "$BIN" \
      --headless --no-audio --speed "$SPEED" \
      --scripted-baseline --route-mode "$mode" --num-orbs "$n" \
      --act-hz "$ACT_HZ" --max-episode-secs "$SECS" --seed "$seed" \
      >"$log" 2>&1
  else
    cargo run --release --quiet -- \
      --headless --no-audio --speed "$SPEED" \
      --scripted-baseline --route-mode "$mode" --num-orbs "$n" \
      --act-hz "$ACT_HZ" --max-episode-secs "$SECS" --seed "$seed" \
      >"$log" 2>&1
  fi
  local rc=$?
  set -e

  # Prefer stdout/log JSON line; fall back to reconstructing from "Train episode done".
  local metrics_line
  metrics_line="$(grep -o 'TRAIN_METRICS_JSON {.*}' "$log" | tail -n1 || true)"
  if [[ -n "$metrics_line" ]]; then
    echo "${metrics_line#TRAIN_METRICS_JSON }" >>"$OUT"
  else
    echo "warn: no TRAIN_METRICS_JSON for mode=$mode n=$n seed=$seed (exit=$rc); parsing log" >&2
    # Best-effort parse of human log line.
    local line
    line="$(grep -o 'Train episode done:.*' "$log" | tail -n1 || true)"
    if [[ -z "$line" ]]; then
      echo "{\"seed\":$seed,\"route_mode\":\"$mode\",\"num_orbs\":$n,\"orbs\":0,\"success\":false,\"player_time\":0,\"wall_secs\":0,\"ticks\":0,\"error\":\"no_metrics\",\"exit_code\":$rc}" >>"$OUT"
    else
      # Example: success=false timeout=true orbs=1/3 player_t=60.00s ticks=6001 ...
      local success orbs num_orbs player_t ticks wall
      success="$(echo "$line" | sed -n 's/.*success=\([^ ]*\).*/\1/p')"
      orbs="$(echo "$line" | sed -n 's/.*orbs=\([0-9]*\)\/\([0-9]*\).*/\1/p')"
      num_orbs="$(echo "$line" | sed -n 's/.*orbs=\([0-9]*\)\/\([0-9]*\).*/\2/p')"
      player_t="$(echo "$line" | sed -n 's/.*player_t=\([0-9.]*\)s.*/\1/p')"
      ticks="$(echo "$line" | sed -n 's/.*ticks=\([0-9]*\).*/\1/p')"
      wall="$(echo "$line" | sed -n 's/.*wall=\([0-9.]*\)s.*/\1/p')"
      echo "{\"seed\":$seed,\"route_mode\":\"$mode\",\"num_orbs\":${num_orbs:-$n},\"orbs\":${orbs:-0},\"success\":${success:-false},\"player_time\":${player_t:-0},\"wall_secs\":${wall:-0},\"ticks\":${ticks:-0},\"parsed_from_log\":true}" >>"$OUT"
    fi
  fi
  rm -f "$log"
}

total=0
for mode in "${MODES[@]}"; do
  for n in "${ORBS[@]}"; do
    for seed in "${SEEDS[@]}"; do
      total=$((total + 1))
      echo "==> [$total] route_mode=$mode num_orbs=$n seed=$seed (secs=$SECS speed=$SPEED)"
      run_one "$mode" "$n" "$seed" || true
    done
  done
done

echo ""
echo "Wrote JSONL metrics: $OUT ($total runs)"
echo "--- summary (python) ---"
python3 - "$OUT" <<'PY'
import json, sys
from collections import defaultdict
from statistics import median

path = sys.argv[1]
rows = []
with open(path) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as e:
            print(f"skip bad line: {e}", file=sys.stderr)

if not rows:
    print("no rows")
    sys.exit(0)

by = defaultdict(list)
for r in rows:
    key = (r.get("route_mode"), r.get("num_orbs"))
    by[key].append(r)

print(f"{'mode':<10} {'n':>3}  {'runs':>4}  {'succ':>5}  {'med_orbs':>8}  {'med_wall':>8}  {'med_steps/s':>11}")
for (mode, n), group in sorted(by.items(), key=lambda x: (str(x[0][0]), x[0][1] or 0)):
    succ = sum(1 for r in group if r.get("success"))
    orbs = [r.get("orbs") or 0 for r in group]
    walls = [float(r.get("wall_secs") or 0) for r in group]
    sps = []
    for r in group:
        if r.get("steps_per_sec") is not None:
            sps.append(float(r["steps_per_sec"]))
        else:
            w = float(r.get("wall_secs") or 0)
            t = float(r.get("ticks") or 0)
            sps.append(t / w if w > 0 else 0.0)
    print(f"{str(mode):<10} {n:>3}  {len(group):>4}  {succ:>2}/{len(group):<2}  {median(orbs):>8.1f}  {median(walls):>8.2f}  {median(sps):>11.0f}")

walls_all = [float(r.get("wall_secs") or 0) for r in rows]
sps_all = []
for r in rows:
    if r.get("steps_per_sec") is not None:
        sps_all.append(float(r["steps_per_sec"]))
    else:
        w = float(r.get("wall_secs") or 0)
        t = float(r.get("ticks") or 0)
        sps_all.append(t / w if w > 0 else 0.0)
print(f"\noverall: {len(rows)} episodes, median wall_secs={median(walls_all):.2f}, median steps/s={median(sps_all):.0f}")
PY
