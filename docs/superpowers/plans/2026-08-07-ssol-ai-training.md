# SSOL AI Training Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Train an agent that reliably finishes Open SSOL level-zero under relativistic kinematics, with **route generalization** (not WR-only overfitting) and a **hierarchical policy with private residual latent “thinking” channels** between layers.

**Architecture:** Privileged-state, goal-conditioned continuous control at a configurable act rate; high-level chooses the next orb (or waypoint) from a **sampled route family** (WR, greedy, noisy variants, random TSP-ish); low-level is `π_lo(a | s, g, z)` where `z` is a private residual latent carried across steps/layers and **not** part of the environment observation API. Physics stays at 100 Hz; decisions at `act_hz` (default 10). Train headless without `--features ai` (no ZMQ/navmesh dependency).

**Tech Stack:** Rust / Bevy 0.19 / bevy_rapier3d 0.35; fixed 100 Hz sim; JSON WR route asset; later Python (Gymnasium + PPO/SAC) optional bridge. Existing hooks: `AiConfig` / `AiActionInput` (no `ai` feature required), `CurriculumConfig` (`--num-orbs`), ghost recordings, headless `--speed`.

## Global Constraints

- **Do not** require Cargo feature `ai` (ZMQ + vendored navmesh) for Phase 0–2 training loops.
- Physics fixed timestep **100 Hz** (`Time::<Fixed>::from_hz(100.0)` in `src/main.rs`).
- Player AI actions use `AiActionInput { look: Vec2, move_dir: Vec2 }` as consumed in `src/player.rs` (forward = positive `move_dir.y` after existing sign convention).
- `orb_id` is **spawn-distance index** (`OrbId`), same as `assets/wr_route_level_zero.json`.
- Confirmed WR order lives at `assets/wr_route_level_zero.json` (`status: user_confirmed_wr` in source notes).
- Act rate must be **runtime-variable**: include `control_dt` in privileged obs so one policy can run at multiple rates.
- Residual latent `z` is **policy-internal only** — never written into privileged env obs exported to the bridge as “ground truth state.”
- Prefer small diffs; do not drive-by reformat `vendor/` or rewrite the old `src/ai/*` stack unless a task explicitly migrates it.
- Headless automation: `--headless --no-audio`; default training smoke uses `--scripted-baseline`.
- Commits: multi-line subject + why; no force-push to master; no secrets.

---

## Spec Summary (problem model)

### Game as MDP

| Item | Detail |
| --- | --- |
| Task | Collect all active orbs, then pass white finish arch |
| Level | Single map, 100 orbs, dense fences, mild villager traffic |
| Horizon | ~10⁴–10⁵ physics ticks full clear |
| Structure | Hierarchical: (A) multi-goal routing + (B) continuous go-to under non-stationary *c* |
| Win | `FinishReached` after all orbs |

### Hard requirements from research + user notes

1. **Hierarchy** — high-level target selection; low-level motor control.
2. **Multi-route training** — sample routes so the low-level (and any learned high-level) **generalizes**, not only WR.
3. **Curriculum** — orb count / radius (`--num-orbs`, `CurriculumConfig`).
4. **Privileged obs first** — not pixels.
5. **Variable act rate** — hold actions `period = round(100 / act_hz)` ticks; obs includes `control_dt`.
6. **Residual latent thinking** — private vector `z` between layers / timesteps (see Architecture).
7. **WR route** — expert high-level prior + evaluation baseline, not the only train distribution.
8. **Demos** — no ghosts yet; scripted teacher first; optional human ghosts later; WR video for strategy only.

### Residual latent (required design)

```
High-level  π_hi:  (s_hi, z) → (g, z')     # optional learned; Phase 0 uses classical router
                           │
                           ▼
Low-level   π_lo:  (s_lo, g, z') → (a, z'')  # motor + latent update
```

- **`z ∈ R^{D}`** (recommend D=32 or 64 default; configurable).
- **Not exposed** in `PrivilegedObs::as_vec()` env export; lives in `PolicyState` / trainer only.
- **Residual form (recommended):**  
  `z' = normalize(z + f_θ(s, g, z))` or GRU-style gated update so identity is easy early in training.
- **Init:** `z_0 = 0` each episode (or learned `z_0` embedding of route_id for multi-route conditioning).
- **Use cases:** carry “I am mid-strafe around a fence,” “commit to current goal despite temporary occlusion,” cross-layer messages when high-level switches goals.
- **Training:** backprop through time on `z` with truncated BPTT (e.g. 16–64 act steps); or stop-grad on `z` into high-level if training layers separately.

### Multi-route family (required design)

Episode samples a **route mode** (configurable mixture):

| Mode | Weight (start) | Construction |
| --- | --- | --- |
| `wr` | 0.25 | Confirmed WR order filtered to active orbs |
| `greedy` | 0.25 | Nearest remaining active orb each time (recomputed) |
| `wr_noisy` | 0.20 | WR with random adjacent swaps / skip-insert noise |
| `random_nn` | 0.15 | Random start orb, then nearest-neighbor tour |
| `reverse_wr` | 0.10 | WR reversed (still finish arch last) |
| `eval_wr` | eval only | Fixed WR for scoreboard |

- Low-level always sees **current goal** `g` (relative vector), not the full tour — so it cannot memorize WR as a single open-loop path.
- Optional: condition low-level on one-hot / embedding of `route_mode` only during ablation; default **no route_id in obs** so skills transfer.
- Evaluation splits: success@N on `eval_wr`, `greedy`, and held-out noisy seeds.

---

## File Map

| Path | Responsibility |
| --- | --- |
| `assets/wr_route_level_zero.json` | Confirmed WR tour (orb_id + world xyz) |
| `src/train/mod.rs` | Plugin, episode loop, act-rate hold, metrics, exit |
| `src/train/route.rs` | Route load + `next_target`; **route family sampler** |
| `src/train/obs.rs` | `PrivilegedObs` (+ `control_dt`); **no latent z** |
| `src/train/scripted.rs` | Scripted go-to teacher |
| `src/train/latent.rs` | `PolicyState { z }`, residual update trait, dims |
| `src/train/action.rs` | `TrainAction`, mapping to `AiActionInput` |
| `src/main.rs` | CLI: `--scripted-baseline`, `--act-hz`, `--max-episode-secs`, `--wr-route`, later `--route-mode` |
| `src/ai_support.rs` | `AiConfig` / `AiActionInput` stubs (already exist without feature `ai`) |
| `docs/superpowers/plans/2026-08-07-ssol-ai-training.md` | This plan |
| `docs/superpowers/specs/2026-08-07-ssol-ai-training.md` | Optional short spec pointer |
| `python/` (later) | Gymnasium env + PPO/SAC; latent inside torch module |
| `justfile` | `baseline-smoke`, `test-train` recipes |

**Out of scope for early tasks:** deleting `src/ai/*`, pixel RL, world models, full navmesh requirement.

---

### Task 0: Document and freeze contracts (this plan)

**Files:**
- Create: `docs/superpowers/plans/2026-08-07-ssol-ai-training.md` (this file)
- Create: `docs/superpowers/specs/2026-08-07-ssol-ai-training.md` (1-page spec summary)

**Interfaces:**
- Produces: frozen contracts for `PrivilegedObs`, `TrainAction`, `PolicyState`, route modes

- [x] **Step 1: Write this plan under `docs/superpowers/plans/`**
- [ ] **Step 2: Write 1-page spec** summarizing goal, multi-route, latent residual, phases
- [ ] **Step 3: Commit docs**

```bash
git add docs/superpowers/plans/2026-08-07-ssol-ai-training.md docs/superpowers/specs/2026-08-07-ssol-ai-training.md
git commit -m "$(cat <<'EOF'
docs: add SSOL AI training plan (multi-route + residual latent)

Capture research conclusions and implementation tasks so Phase 0+
work stays aligned with generalization and hierarchical latents.
EOF
)"
```

---

### Task 1: Finish Phase 0 harness — privileged loop + scripted baseline

**Files:**
- Modify: `src/train/mod.rs`, `src/train/obs.rs`, `src/train/scripted.rs`, `src/train/route.rs`
- Modify: `src/main.rs` (CLI already partially wired)
- Create: `assets/wr_route_level_zero.json` (if missing on branch)
- Test: unit tests in `src/train/*`

**Interfaces:**
- Consumes: `AiConfig`, `AiActionInput`, `GameState`, `OrbId`, `FinishReached`
- Produces:
  - `TrainConfig { enabled, scripted, act_hz, max_episode_secs, wr_route_path, exit_on_done, log_every_ticks }`
  - `PrivilegedObs` (see fields in `src/train/obs.rs`)
  - `TrainAction { move_dir: Vec2, yaw_rate: f32 }`
  - CLI: `--scripted-baseline --act-hz 10 --max-episode-secs 120 --num-orbs N --headless --no-audio --speed 50`

- [ ] **Step 1: Confirm unit tests pass**

```bash
cargo test --bin ssol_simulator train::
```

Expected: `train::route` and `train::scripted` tests PASS.

- [ ] **Step 2: Fix compile warnings that hide real issues**

Allow or use `RouteStop::{seq, phase}`, `as_vec`, `yaw_error` (prefix `_` or `#[allow(dead_code)]` only if intentional public API).

- [ ] **Step 3: Headless smoke — 3 orbs, short timeout**

```bash
cargo run --release -- \
  --headless --no-audio --speed 100 \
  --scripted-baseline --num-orbs 3 \
  --act-hz 10 --max-episode-secs 60
```

Expected: process exits; log line `Train episode done:` with `orbs=k/3` for some k≥1 ideally; no panic.

- [ ] **Step 4: Headless smoke — 7 orbs**

Same as Step 3 with `--num-orbs 7 --max-episode-secs 180`. Record orbs collected and wall steps/s in commit message or `docs/` note.

- [ ] **Step 5: Add just recipes**

```just
# justfile
test-train:
    cargo test --bin ssol_simulator train::

baseline-smoke n="3" secs="60" speed="100":
    cargo run --release -- --headless --no-audio --speed {{speed}} \
      --scripted-baseline --num-orbs {{n}} --act-hz 10 --max-episode-secs {{secs}}
```

- [ ] **Step 6: Commit**

```bash
git add src/train src/main.rs assets/wr_route_level_zero.json justfile scripts/release_assets.txt
git commit -m "$(cat <<'EOF'
feat: Phase 0 train harness with scripted WR baseline

Headless-capable privileged loop at configurable act_hz, WR
high-level targets, and go-to teacher without enabling feature ai.
EOF
)"
```

---

### Task 2: Multi-route sampler (generalization)

**Files:**
- Modify: `src/train/route.rs`
- Create: `src/train/route_family.rs` (if `route.rs` grows past ~200 lines)
- Modify: `src/train/mod.rs` (sample route at episode start)
- Modify: `src/main.rs` — `--route-mode wr|greedy|mix|…`

**Interfaces:**
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteMode { Wr, Greedy, WrNoisy, RandomNn, ReverseWr, Mix }

pub struct ActiveRoute {
    pub mode: RouteMode,
    pub stops: Vec<RouteStop>, // static order modes; greedy may recompute
    pub dynamic_greedy: bool,
}

impl ActiveRoute {
    pub fn next_target(
        &self,
        player: Vec3,
        collected: &HashSet<u8>,
        active: &HashSet<u8>,
        remaining_positions: &[(u8, Vec3)],
    ) -> Option<(u8, Vec3)>;
}

pub fn sample_route(
    mode: RouteMode,
    wr: &WrRoute,
    active_orbs: &[(u8, Vec3)],
    rng: &mut impl Rng,
) -> ActiveRoute;
```

- [ ] **Step 1: Failing tests for greedy and reverse**

```rust
#[test]
fn greedy_picks_nearest_uncollected() {
    let active = vec![(1, Vec3::new(10.0,0.0,0.0)), (2, Vec3::new(3.0,0.0,0.0))];
    let collected = HashSet::new();
    let player = Vec3::ZERO;
    let (id, _) = greedy_next(player, &collected, &active).unwrap();
    assert_eq!(id, 2);
}

#[test]
fn reverse_wr_order_is_reversed_ids() {
    let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
    let r = build_reverse_wr(&wr, None);
    let ids: Vec<_> = r.stops.iter().map(|s| s.orb_id).collect();
    assert_eq!(ids, vec![2, 1, 4]);
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cargo test --bin ssol_simulator train::route -- --nocapture
```

- [ ] **Step 3: Implement `sample_route` + `next_target`**

- Greedy: each call argmin distance among uncollected∩active.
- WrNoisy: clone WR, apply `k` random adjacent swaps (k~Uniform(1,5)).
- RandomNn: shuffle start among active, then classic NN tour.
- ReverseWr: reverse WR list (finish still after last orb via game_win branch).
- Mix: weighted sample of modes using weights in Global Constraints table.

- [ ] **Step 4: Episode start samples `ActiveRoute`; log `route_mode=`**

- [ ] **Step 5: Smoke with `--route-mode greedy --num-orbs 5`**

- [ ] **Step 6: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(train): multi-route sampler for generalization

Train/eval against WR, greedy, noisy WR, random NN, and reverse
tours so low-level control cannot overfit a single open-loop path.
EOF
)"
```

---

### Task 3: Residual latent `PolicyState` (private thinking)

**Files:**
- Create: `src/train/latent.rs`
- Modify: `src/train/mod.rs` (hold `PolicyState` on episode; do not put in `PrivilegedObs`)
- Modify: `src/train/scripted.rs` (scripted may ignore z, but API accepts it)
- Later Python: mirror dims

**Interfaces:**

```rust
/// Private residual latent — NOT part of PrivilegedObs / env export.
#[derive(Clone, Debug)]
pub struct PolicyState {
    pub z: Vec<f32>, // length LATENT_DIM
}

pub const LATENT_DIM: usize = 32;

impl PolicyState {
    pub fn zeros() -> Self { Self { z: vec![0.0; LATENT_DIM] } }
}

/// Residual update: z <- normalize(z + f(s,g,z)).
/// Phase 0 scripted: f = 0 (identity). Learned f comes in RL phase.
pub trait LatentUpdate {
    fn update(&self, z: &PolicyState, obs: &PrivilegedObs, action: &TrainAction) -> PolicyState;
}

pub struct IdentityLatent;
impl LatentUpdate for IdentityLatent {
    fn update(&self, z: &PolicyState, _: &PrivilegedObs, _: &TrainAction) -> PolicyState {
        z.clone()
    }
}
```

- [ ] **Step 1: Test latent is excluded from env export**

```rust
#[test]
fn privileged_obs_vec_has_no_latent_slots() {
    let obs = PrivilegedObs::default();
    let v = obs.as_vec();
    // Document exact length; must not grow when LATENT_DIM changes
    assert_eq!(v.len(), 23);
}
```

- [ ] **Step 2: Test residual identity + additive stub**

```rust
#[test]
fn residual_add_changes_z() {
    let z = PolicyState::zeros();
    let f = vec![0.1; LATENT_DIM];
    let z2 = residual_apply(&z, &f);
    assert!(z2.z.iter().any(|x| *x != 0.0));
}
```

- [ ] **Step 3: Implement `latent.rs`; wire `PolicyState` into `TrainEpisode`**

- [ ] **Step 4: Document in module rustdoc: “z is private; bridges must not require it for reset/step obs”**

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(train): private residual PolicyState latent between layers

Add z-space for hierarchical/recurrent thinking that is not part of
exported privileged observations, enabling later learned residual updates.
EOF
)"
```

---

### Task 4: Goal-conditioned reward helper (sim-side)

**Files:**
- Create: `src/train/reward.rs`
- Modify: `src/train/mod.rs` to log reward each act step

**Interfaces:**

```rust
pub struct RewardConfig {
    pub orb: f32,           // default 1.0
    pub finish: f32,        // default 10.0
    pub step_cost: f32,     // default -0.001 per act
    pub dist_coef: f32,     // potential-based: -dist_coef * (d' - d)
    pub collision_coef: f32 // optional 0.0 until contact signal exposed
}

pub fn act_reward(
    cfg: &RewardConfig,
    prev_dist: f32,
    obs: &PrivilegedObs,
    orbs_gained: u32,
    finished: bool,
) -> f32;
```

- [ ] **Step 1: Unit tests for potential-based shaping sign**

```rust
#[test]
fn closer_to_goal_is_non_negative_shaping() {
    let cfg = RewardConfig::default();
    let r = act_reward(&cfg, 10.0, &obs_with_dist(8.0), 0, false);
    assert!(r > cfg.step_cost);
}
```

- [ ] **Step 2: Implement + log `rew=` in train tick every act**

- [ ] **Step 3: Commit**

---

### Task 5: Throughput + multi-seed eval harness

**Files:**
- Create: `src/train/eval.rs` or `scripts/train_baseline_matrix.sh`
- Modify: `justfile`

**Interfaces:**
- Produces JSON lines metrics: `{seed, route_mode, num_orbs, orbs, success, player_time, wall_secs, ticks}`

- [ ] **Step 1: Script matrix**

```bash
for mode in wr greedy; do
  for n in 1 3 7; do
    cargo run --release -- --headless --no-audio --speed 200 \
      --scripted-baseline --route-mode $mode --num-orbs $n \
      --max-episode-secs 120 || true
  done
done
```

- [ ] **Step 2: Document median orbs and success in `docs/superpowers/plans/` appendix or `docs/train_baseline.md`**

- [ ] **Step 3: Commit**

---

### Task 6: Python / RL entry (optional bridge, after harness stable)

**Files:**
- Create: `python/src/ssol_training/phase0_env.py` (new, do not revive old ZMQ stack blindly)
- Prefer: subprocess per env **or** thin IPC later; start with **offline dataset** of scripted `(obs, action, reward, done)` if live bridge is slow

**Interfaces:**

```python
class SSOLPrivilegedEnv(gym.Env):
    observation_space: Box  # PrivilegedObs.as_vec()
    action_space: Box(4,)   # move_x, move_y, yaw_rate, (optional pitch unused)
    # PolicyState z is INSIDE the torch Actor, not env.state
```

Actor sketch:

```python
class Actor(nn.Module):
    def __init__(self, obs_dim, act_dim, z_dim=32):
        ...
        self.f = nn.GRUCell(obs_dim + goal_dim + z_dim, z_dim)  # or residual MLP
        self.pi = nn.Linear(obs_dim + goal_dim + z_dim, act_dim)

    def forward(self, obs, z):
        z2 = z + self.f_res(torch.cat([obs, z], -1))  # residual
        a = self.pi(torch.cat([obs, z2], -1))
        return a, z2
```

- [ ] **Step 1: Dump scripted rollouts to `data/scripted_*.npz`**
- [ ] **Step 2: BC warm-start on go-to with mixed routes**
- [ ] **Step 3: PPO/SAC fine-tune goal-conditioned**
- [ ] **Step 4: Commit Python package changes separately from sim**

---

### Task 7: Scale curriculum + WR eval

**Files:** sim CLI only + training configs

- [ ] **Step 1: Ladder** `num_orbs ∈ {1,3,7,18,50,100}` with route mix
- [ ] **Step 2: Unlock full `return_growth` / *c* schedule after 7-orb success ≥ 90%**
- [ ] **Step 3: Report `eval_wr` full-clear rate and time vs scripted teacher**
- [ ] **Step 4: Commit configs + results notes**

---

### Task 8: Merge worktree to master + cleanup

**Files:** none (git ops)

- [ ] **Step 1: Ensure CI-equivalent checks**

```bash
cargo test --bin ssol_simulator train::
cargo build --release
```

- [ ] **Step 2: Merge branch into master**

```bash
cd /home/xertrov/src/ssol-simulator
git checkout master
git merge --no-ff train/phase0-env -m "$(cat <<'EOF'
Merge branch 'train/phase0-env'

Phase 0 training harness, WR asset, multi-route/latent plan docs.
EOF
)"
```

- [ ] **Step 3: Remove worktree**

```bash
git worktree remove /home/xertrov/src/ssol-simulator-wt-phase0
git branch -d train/phase0-env  # if fully merged
```

---

## Phase Map (execution order)

| Phase | Tasks | Exit criteria |
| --- | --- | --- |
| **0 — Harness** | 0–1 | Scripted headless episodes run; metrics logged; WR targets work |
| **0b — Generalization plumbing** | 2–3 | Route mix + private `z` API exist; tests green |
| **1 — Motor skill** | 4–6 | Goal-conditioned BC/RL reaches ≥90% success on 7 orbs across `wr`+`greedy` |
| **2 — Full clear** | 7 | Non-zero full WR clears; multi-route success tracked |
| **3 — Polish** | optional | Ghost IL, residual RL speedrun, vision distill |

---

## Self-Review (writing-plans checklist)

**1. Spec coverage**

| Requirement | Task |
| --- | --- |
| Hierarchy high/low | Architecture + Tasks 1–2, 6 |
| Multi-route / no WR-only overfit | Task 2, 5, 7 |
| Residual latent between layers | Task 3, 6 |
| Variable act rate + `control_dt` | Task 1 (obs already has field) |
| Privileged obs not pixels | Task 1 |
| Curriculum num_orbs | Task 1, 5, 7 |
| Scripted teacher first | Task 1 |
| WR asset | Task 1 |
| Headless smoke | Task 1, 5 |
| Worktree merge cleanup | Task 8 |
| Ignore old ZMQ `ai` feature | Global Constraints |

**2. Placeholder scan:** No TBD/TODO steps; code sketches included for interfaces.

**3. Type consistency:** `PrivilegedObs`, `TrainAction`, `PolicyState`, `ActiveRoute`, `RouteMode`, `LATENT_DIM=32` used consistently above.

**Known risks (do not hide):**
- Scripted baseline may fail dense fence pockets without local obstacle rays (add N-ray distances in a follow-up if smoke shows wall-stuck).
- Bevy headless throughput may limit RL sample rate; measure before large PPO runs.
- Partial Phase 0 code already exists on branch `train/phase0-env` — Task 1 is finish/verify, not greenfield.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-07-ssol-ai-training.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — continue in this session with executing-plans / PIRFL checkpoints  

**Which approach?**
