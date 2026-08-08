# Phase 1: Training method + NN shape

**Date:** 2026-08-07  
**Status:** Active (sim Phase 1a landed: rays + teacher + dumps)  
**Research agents:** training-method + train-stack gap review (2026-08-07)

## Recommended stack (decisive)

| Piece | Choice |
| --- | --- |
| Algorithm | **SAC** (off-policy continuous control) |
| Bootstrap | **BC warm-start** from ray-aware scripted teacher → **residual SAC** |
| Not first | Pure PPO from scratch; RecurrentPPO; TD3 as default |
| Latent `z` | **Identity / unused in Phase 1 motor skill** (slot kept for Phase 2+) |
| Sensors | **16 horizontal wall rays** @ act rate (in `PrivilegedObs`, schema v2) |
| Act rate | **10 Hz** (physics 100 Hz); `control_dt` in obs |
| Library | **Stable-Baselines3 SAC** (+ torch BC); CleanRL optional |

### Why SAC + BC residual

- Sim throughput ~**560 phys ticks/s** ≈ **56 act/s** — sample efficiency dominates.
- Dense rewards (distance potential + orb) fit off-policy continuous control.
- Entropy helps escape fence traps where deterministic policies stick.
- Teacher already gets most of the way (e.g. 6/7 WR); residual learns only the failure modes.

### Why not train `z` yet

Go-to with privileged state + goal + rays is nearly Markov. GRU residual is for longer commitment / hierarchical messaging after 7-orb motor skill works.

---

## Neural network shape

### Observation (schema v2) — `OBS_DIM = 39`

```
[0..22]  base (23): pos3, vel3, yaw, pitch, speed, c, speed_mult, lorentz,
         score, nb_orbs, player_time, control_dt, target_rel3, target_dist,
         target_yaw_err, target_orb_id (-1=arch), episode_tick
[23..38] wall_rays[16]: 0=touch … 1=clear (toi / 40m), body-relative 360°
```

**Not in obs:** residual latent `z` (policy-private, dim 32).

**Normalize:** running mean/std (SB3 `VecNormalize`) on unbounded fields; rays already in [0,1].

### Action — dim 3

| Index | Meaning | Bounds |
| --- | --- | --- |
| 0 | `move_dir.x` (strafe via AI convention) | [−1, 1] |
| 1 | `move_dir.y` (forward) | [−1, 1] |
| 2 | `yaw_rate` (rad/s) | [−2.5, 2.5] |

SAC: tanh-squashed Gaussian.

**Residual form (preferred):**  
`a = clip(a_teacher(s) + π_θ(s), bounds)` with smaller residual ranges (e.g. ±0.5 move, ±1 yaw).

### Network (default — Phase 1 motor)

```
obs (39) → Linear(256) → SiLU → Linear(256) → SiLU
  ├─ Actor: μ, log_σ → tanh-Gaussian (3)
  └─ Twin Q: same width 256–256
```

- Soft target τ = 0.005, γ = 0.99, batch 256, buffer 1e6, lr 3e-4, `ent_coef=auto`
- **No** CNN / transformers on the low-level residual for Phase 1
- Parameter count is tiny (~150k including twin Q); **sim + multi-env is the wall-time bottleneck**, not matmuls

### Architecture ladder (when to grow beyond 256×256)

| When | Architecture | Why |
| --- | --- | --- |
| Phase 1 (now) | **MLP 256–256** | Fixed 39-d nearly-Markov obs; residual SAC + BC; sample efficiency from algorithm, not capacity |
| Easy win if stuck | **Factored trunk**: separate small MLPs for `base(23)` and `rays(16)` → concat → fusion → actor/Q | Inductive bias for “kinematics vs walls”; still cheap |
| Slightly better MLP | **LayerNorm + residual MLP** (SimBa-style / LN-MLP) | Often more sample-efficient than raw MLP on vector RL; still O(d²) |
| Partial observability / commitment | **GRU/LSTM residual** over short history (or private latent `z`) | Memory of last wall scrape / turn commitment — *not* a full transformer |
| Multi-orb **set** obs (no single `g`) | **Set attention / tiny Transformer encoder** over orb tokens | Permutation-invariant “which orb next”; high-level or mid-level only |
| Pixels / top-down map | CNN or ViT trunk | Not Phase 1 (privileged vectors win) |
| Offline long demos | Decision Transformer / RT-1 style | Different data regime; optional later if we have WR-scale trajectories |

**Transformers are not “more efficient” here by default.** For a fixed 39-float vector they usually:

- Need more data and wall-time (attention is O(T²) or O(N² tokens); optimizer cost dominates tiny MLPs)
- Help only when there is **structure they can exploit**: variable-length sets, long history, multi-modal tokens
- Hurt CPU-only multi-env ladders where every gradient step should be microseconds

**Recommended next capacity upgrades (order):** (1) factored base+ray encoders, (2) LN/residual MLP, (3) short GRU for residual, (4) set-attention high-level over orb list once hierarchy exists. Full sequence transformers only if we switch to history-as-tokens or multi-orb set policies.

### Ideal vs planned

| Ideal (long-term) | Phase 1 plan |
| --- | --- |
| Body-frame relative kinematics | World + yaw_err OK for now; body-frame is stretch |
| HER multi-goal | Not needed — external high-level supplies one `g` |
| Learned high-level | Classical route family (mix) until motor clears 7 orbs |
| Recurrent residual z | After Phase 1 gate |
| Multi-env 8× | Multi-process headless when BC+SAC needs scale |
| Set-attention / tiny TF over orbs | After motor skill; high-level route head, not residual SAC actor |

---

## Training recipe

1. Collect demos: scripted + rays, routes mix/wr/greedy, `num_orbs` 1→7, `--dump-transitions`
2. BC (MSE) until open-space go-to is solid
3. Residual SAC, curriculum 1→3→7
4. Eval frozen: `--route-mode wr` and `greedy`, ≥20 seeds, **≥90% @ 7 orbs both**

### Wall-time estimate (1 env @ 56 act/s)

| Act budget | Wall |
| --- | --- |
| 1M | ~5 h |
| 2M | ~10 h |

With 4 processes ≈ 1.5–2× less wall (CPU-limited).

---

## What not to do

- Pure PPO/RecurrentPPO cold start on 7 orbs  
- Pixel RL for motor skill  
- Depend on `--features ai` / ZMQ / navmesh  
- Train huge nets on 39-dim obs  
- Put `z` into env obs  

---

## Sim CLI (Phase 1a)

```bash
# Rays + improved teacher smoke
just baseline-smoke n=7 secs=90 speed=200 route=wr

# Dump transitions for BC
cargo run --release -- --headless --no-audio --speed 200 \
  --scripted-baseline --num-orbs 7 --route-mode mix --seed 0 \
  --max-episode-secs 90 --num-episodes 3 \
  --dump-transitions data/scripted_mix_n7.jsonl
```
