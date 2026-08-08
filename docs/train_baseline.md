# Scripted baseline matrix + Phase 1 BC/SAC

Throughput + multi-seed eval of the **scripted go-to teacher** under fixed route modes.
Does **not** enable Cargo feature `ai`.

## Phase 1 training quickstart

```bash
cargo build --release
# 1) demos (schema v2 JSONL, ~7.5k transitions for wr|greedy × 1,3,7 × seeds 0-2)
just collect-demos

# 2) BC (needs python/.venv — see python/README note or create with torch+sb3)
just bc-train

# 3) residual SAC smoke (live --train-stdio)
just sac-train n=1 steps=5000 route=wr
```

NN + method: [`docs/superpowers/specs/2026-08-07-phase1-nn-and-training.md`](superpowers/specs/2026-08-07-phase1-nn-and-training.md)

## How to run

```bash
# Full matrix (default: wr|greedy × 1,3,7 orbs × seeds 0,1,2; 60s sim; speed 200)
just baseline-matrix

# Or directly:
scripts/train_baseline_matrix.sh --out docs/baseline_matrix.jsonl

# Single smoke:
just baseline-smoke n=3 secs=60 speed=200 route=wr seed=0
```

Optional env / flags: `SECS`, `SPEED`, `MODES`, `ORBS`, `SEEDS`, `BIN` (prebuilt binary),
`--out PATH`.

Each episode prints a machine-readable line:

```text
TRAIN_METRICS_JSON {"seed":0,"route_mode":"wr","num_orbs":1,"orbs":1,"success":true,...}
```

The matrix script strips the prefix into a JSONL file (one object per line).

CLI flags used: `--scripted-baseline --route-mode … --num-orbs … --seed … --max-episode-secs … --headless --no-audio --speed …`.

## Results (2026-08-07)

Host: release binary, `--speed 200`, `--max-episode-secs 60`, `--act-hz 10`, seeds `{0,1,2}`.
Raw: [`docs/baseline_matrix.jsonl`](baseline_matrix.jsonl).

| route_mode | num_orbs | runs | success | median orbs | median wall_s | median steps/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| wr | 1 | 3 | 3/3 | 1.0 | 1.14 | ~560 |
| wr | 3 | 3 | 0/3 | 2.0 | 10.54 | ~570 |
| wr | 7 | 3 | 0/3 | 6.0 | 10.51 | ~570 |
| greedy | 1 | 3 | 3/3 | 1.0 | 1.18 | ~540 |
| greedy | 3 | 3 | 0/3 | 2.0 | 10.62 | ~565 |
| greedy | 7 | 3 | 0/3 | 3.0 | 10.60 | ~565 |

**Overall:** 18 episodes; median wall ≈ 10.4 s for timeout runs; median physics throughput ≈ **560 ticks/s** (~5.6× realtime at 100 Hz).

### Notes

1. **`wr` / `greedy` are deterministic** for a fixed map + curriculum: seeds only affect modes that use RNG (`mix`, `wr_noisy`, `random_nn`). Identical orbs across seeds for leaf modes is expected.
2. **1-orb** finishes in ~1.0 s sim / ~1.2 s wall (success via arch after last orb).
3. **3-orb** scripted teacher times out at 2/3 for both modes (often stuck on geometry; no raycast/obstacle avoidance yet).
4. **7-orb WR** collects 6/7 then times out — stronger than greedy (3/7). Route prior matters even for a pure go-to teacher.
5. **Throughput** is CPU-bound: raising `--speed` past ~200 does not raise steps/s once the fixed loop saturates. Expect ~0.5–0.6k physics steps/s on this host for headless scripted runs — factor this into PPO sample budgets (Task 6+).
6. Exit code is 0 on success, 1 on timeout/failure; the matrix continues on non-zero (`|| true` / per-run isolation).

## Metrics schema

```json
{
  "seed": 0,
  "route_mode": "wr",
  "num_orbs": 3,
  "orbs": 2,
  "success": false,
  "player_time": 60.0,
  "wall_secs": 10.5,
  "ticks": 6002,
  "timed_out": true,
  "act_steps": 601,
  "steps_per_sec": 570.0,
  "requested_route_mode": "wr"
}
```
