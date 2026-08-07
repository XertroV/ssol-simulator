# SSOL AI Training — Spec (short)

**Date:** 2026-08-07  
**Status:** Active  
**Full plan:** `docs/superpowers/plans/2026-08-07-ssol-ai-training.md`

## Goal

An agent that **finishes Open SSOL level-zero** (collect active orbs → white arch) under relativistic player physics, with skills that **transfer across orb orders**, not a single memorized WR path.

## Non-goals (early)

- Pixel / first-person vision RL  
- Requiring Cargo feature `ai` (ZMQ + navmesh)  
- Villager multi-agent modeling  
- Perfect video→ghost action recovery from WR streams  

## Core design decisions

1. **Hierarchy:** high-level selects next goal `g` (orb / arch); low-level outputs continuous move + yaw rate.
2. **Multi-route training:** each episode samples a route family member — WR, greedy NN, noisy WR, random NN, reverse WR — so low-level is goal-conditioned and cannot overfit one tour. WR remains the **eval** gold standard and one train mode.
3. **Privileged observations:** ego kinematics, `c` / speed mult / Lorentz, score, relative goal, `control_dt`. No latent in the env export.
4. **Variable act rate:** physics 100 Hz; policy at `act_hz` (default 10); action hold; `control_dt` in obs.
5. **Private residual latent `z`:** dimension 32 (configurable), carried inside the policy across act steps and between hierarchical modules. Update form residual `z ← z + f(·)`. **Not** part of `PrivilegedObs`. Enables “thinking” / commitment / mid-maneuver memory without polluting the env API.
6. **Teachers:** scripted go-to first; optional ghosts later; WR JSON for high-level prior.
7. **Curriculum:** `--num-orbs` / spawn radius before full 100 + full *c* schedule.

## Success metrics

| Gate | Metric |
| --- | --- |
| Phase 0 | Headless scripted episodes run; metrics logged |
| Phase 1 | ≥90% success clearing 7 orbs on **both** WR-filtered and greedy routes |
| Phase 2 | Non-zero full clears on `eval_wr`; multi-route stats reported |
| Generalization | Gap between WR-only train vs mixed-route train on held-out greedy/noisy seeds |

## Artifacts

- WR route: `assets/wr_route_level_zero.json`  
- Train module: `src/train/`  
- Graph (human annotation): `screenshots/level_zero_route_graph.png` (gitignored; regen from tools if needed)

## Open product choice (resolved default)

Optimize for **reliably finishing + route-general motor skill**. Human-like camera polish is Phase 3 optional.
