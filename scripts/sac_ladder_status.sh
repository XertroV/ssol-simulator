#!/usr/bin/env bash
# Summarize SAC ladder progress on this host (x-alpha) for check-ins / ETA.
# Usage: bash scripts/sac_ladder_status.sh [OUT_ROOT]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT_ROOT="${1:-data/sac_ladder}"
LOG="$OUT_ROOT/ladder.log"

echo "host=$(hostname) time=$(date -Is)"
echo "out_root=$OUT_ROOT"

# Running?
if pgrep -af 'ssol_training.phase1_sac|run_sac_ladder' >/tmp/sac_ladder_ps.$$ 2>/dev/null; then
  echo "process:"
  # Drop the pgrep line itself
  grep -v 'sac_ladder_status\|pgrep' /tmp/sac_ladder_ps.$$ | head -8 || true
else
  echo "process: none"
fi
rm -f /tmp/sac_ladder_ps.$$

for tag in n1_wr_30k n3_mix_100k n7_mix_300k; do
  if [[ -f "$OUT_ROOT/$tag/sac_model.zip" ]]; then
    sz=$(stat -c%s "$OUT_ROOT/$tag/sac_model.zip" 2>/dev/null || echo 0)
    mtime=$(date -Is -r "$OUT_ROOT/$tag/sac_model.zip" 2>/dev/null || true)
    echo "stage $tag: DONE model_bytes=$sz mtime=$mtime"
  else
    echo "stage $tag: pending"
  fi
done

if [[ -f "$LOG" ]]; then
  echo "log_bytes=$(stat -c%s "$LOG") log_mtime=$(date -Is -r "$LOG")"
  # Staleness (seconds) — long gaps can mean episode-gated dumps, not a dead run
  now_s=$(date +%s)
  log_s=$(date +%s -r "$LOG")
  echo "log_age_s=$((now_s - log_s))"
  # Last SB3 metrics dump (fps / timesteps / reward)
  python3 - "$LOG" <<'PY'
import re, sys
from pathlib import Path
text = Path(sys.argv[1]).read_text(errors="replace")
# Prefer timestamped headers from phase1_sac
headers = list(re.finditer(r"=== (\S+) timesteps=(\d+) ===", text)
)
stages = list(re.finditer(r"=== Stage (\S+):", text))
dones = list(re.finditer(r"=== Stage (\S+) done", text))
complete = "SAC ladder complete" in text
print(f"log_stages_started={[m.group(1) for m in stages[-5:]]}")
print(f"log_stages_done={[m.group(1) for m in dones]}")
print(f"ladder_complete={complete}")
# last metric block
blocks = re.split(r"-{20,}", text)
last = None
for b in reversed(blocks):
    if "total_timesteps" in b and "fps" in b:
        last = b
        break
if last:
    def grab(key):
        m = re.search(rf"\|\s*{key}\s*\|\s*([^\s|]+)", last)
        return m.group(1) if m else "?"
    print(
        "last_metrics:",
        f"ts={grab('total_timesteps')}",
        f"fps={grab('fps')}",
        f"ep_rew={grab('ep_rew_mean')}",
        f"ep_len={grab('ep_len_mean')}",
        f"elapsed_s={grab('time_elapsed')}",
        f"episodes={grab('episodes')}",
    )
if headers:
    h = headers[-1]
    print(f"last_timestamped_dump: {h.group(1)} timesteps={h.group(2)}")
# Heartbeat lines from phase1_sac (not episode-gated)
heartbeats = list(
    re.finditer(
        r"(\S+) heartbeat timesteps=(\d+) fps≈([0-9.]+)",
        text,
    )
)
if heartbeats:
    hb = heartbeats[-1]
    print(
        f"last_heartbeat: {hb.group(1)} timesteps={hb.group(2)} fps≈{hb.group(3)}"
    )
    # Prefer heartbeat for current progress if newer than metric dump
    try:
        cur_ts = max(cur_ts if "cur_ts" in dir() else 0, int(hb.group(2)))
        fps = float(hb.group(3))
    except Exception:
        pass
# ETA for remaining curriculum (rough)
# budgets: n1=30k n3=100k n7=300k
budgets = [("n1_wr_30k", 30000), ("n3_mix_100k", 100000), ("n7_mix_300k", 300000)]
done_tags = {m.group(1) for m in dones}
# also count sac_model presence via env note — status script already prints DONE
import os
root = Path(sys.argv[1]).parent
done_tags |= {t for t, _ in budgets if (root / t / "sac_model.zip").is_file()}
fps = 30.0
if last:
    try:
        fps = float(re.search(r"\|\s*fps\s*\|\s*([0-9.]+)", last).group(1))
    except Exception:
        pass
# current stage progress
cur_ts = 0
if last:
    try:
        cur_ts = int(float(re.search(r"\|\s*total_timesteps\s*\|\s*([0-9.]+)", last).group(1)))
    except Exception:
        pass
# which stage is active?
active = None
for t, b in budgets:
    if t not in done_tags:
        active = (t, b)
        break
if active is None:
    print("eta: ladder complete (all stages done)")
else:
    tag, budget = active
    # if last log is for this stage, use cur_ts; else 0
    # heuristic: if stage just started, cur_ts may still be previous stage — clamp
    progress = min(cur_ts, budget) if tag not in done_tags else budget
    # if we see "Stage tag:" after last done, use cur_ts only if log mentions tag recently
    rem = 0.0
    first = True
    for t, b in budgets:
        if t in done_tags:
            continue
        if first:
            left = max(0, b - (cur_ts if t == tag else 0))
            # slower later stages
            scale = {"n1_wr_30k": 1.0, "n3_mix_100k": 0.85, "n7_mix_300k": 0.65}.get(t, 0.7)
            rem += left / max(fps * scale, 1e-6)
            first = False
        else:
            scale = {"n1_wr_30k": 1.0, "n3_mix_100k": 0.85, "n7_mix_300k": 0.65}.get(t, 0.7)
            rem += b / max(fps * scale, 1e-6)
    hrs = rem / 3600.0
    print(f"active_stage={tag} budget={budget} last_ts={cur_ts} fps≈{fps:.1f}")
    print(f"eta_remaining≈{rem/60:.0f} min ({hrs:.1f} h)")
PY
  echo "--- last 12 log lines ---"
  tail -n 12 "$LOG"
else
  echo "log: missing"
fi
