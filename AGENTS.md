# Open SSOL — Agent Instructions

`ssol_simulator` is a Bevy desktop game/simulator (Open SSOL): collect orbs that lower the speed of light, then finish through the white arch. Player-facing docs live in `README.md`. Release packaging is in `RELEASING.md`.

Prefer **actionable** instructions here. Link out for long docs rather than duplicating them.

## Stack

| Piece | Notes |
| --- | --- |
| Language | Rust edition **2024** |
| Engine | **Bevy 0.19** (`bevy_shader` 0.19) |
| Physics | **bevy_rapier3d 0.35** (`Velocity.linear` / `.angular`, not `linvel`/`angvel`) |
| Optional AI | Cargo feature `ai` → ZMQ bridge + training hooks; not in default release builds |
| Python training | `python/` (uv project); talks to the sim when AI/ZMQ is enabled |
| Perf HUD | Vendored `vendor/iyes_perf_ui` (Bevy 0.19 text API patches) |
| Navmesh (AI) | Optional `vendor/vleue_navigator` + polyanya; see `vendor/README.md` |

Keep the game runnable from the repo root with `assets/` beside the binary. Do not assume assets are embedded.

## Commands

Use the root `justfile` when a recipe exists.

| Goal | Command |
| --- | --- |
| List recipes | `just` |
| Unit / integration tests | `just test` or `cargo test` |
| UI screenshot unit tests (no GPU) | `just test-ui-screenshots` |
| Capture UI screenshots | `just ui-screenshots` → `screenshots/ui/` |
| Screenshots + AI UI | `just ui-screenshots-ai` |
| Run game (dev) | `cargo run -- --no-audio` (add flags as needed) |
| Release binary | `cargo build --release --locked` |

Useful CLI flags (see `src/main.rs`): `--headless`, `--no-audio`, `--num-orbs N`, `--ui-screenshots DIR`, ghost verify/test flags, and (with `--features ai`) `--ai-mode` / `--ai-test` / `--zmq-port`.

**Headless UI screenshots:** when `DISPLAY` is unset, `just ui-screenshots` uses Xvfb + Mesa lavapipe (`mesa-vulkan-drivers`). Hardware Vulkan often fails under Xvfb without DRI3.

**Worktrees:** if using multiple worktrees on this machine, prefer `export RUSTC_WRAPPER=sccache` so dependency compiles are shared (see `_CLAUDE.todo.md`).

## Layout

```
src/main.rs          # binary entry, CLI, plugin wiring
src/ui/              # Bevy UI: HUD, pause, finish, toasts, screenshot harness
src/player/          # movement, camera grab, orbs
src/game_state/      # score, timers, pause, win flow
src/relativity/      # relativistic materials / shaders
src/ai/              # only meaningful with --features ai
src/ghost.rs         # recording / verification of runs
assets/              # models, textures, shaders, audio, scene JSON (required at runtime)
vendor/              # path deps for Bevy 0.19; do not casually rewrite
python/              # RL / training bridge
scripts/             # release packaging
```

There is **no** `lib.rs`; modules are declared from `main.rs`. Shared code for bins/tests is still under `src/`.

## Coding norms

- Match existing style: Bevy plugins, systems, resources, observers (`On` / `commands.trigger`), Rapier in fixed schedule where already used.
- Prefer small, task-scoped diffs. Do not drive-by reformat or rename unrelated code.
- After Bevy upgrades, search migration guides for APIs this repo already hit:
  - UI: `FontSize::Px`, `font: handle.into()` (`FontSource`), `Node.border_radius` (not a separate component)
  - Scenes: `WorldAssetRoot`, `WorldInstanceReady` (`bevy::world_serialization`)
  - Cameras: `RenderTarget` as its own component (not `Camera.target`)
  - Lights: `shadow_maps_enabled` (not `shadows_enabled`)
- New runtime assets: add paths to `scripts/release_assets.txt` when they must ship in releases.
- Do not commit secrets, local config dumps, or generated `screenshots/` (gitignored).
- Do not expand `vendor/` edits unless the task is specifically Bevy-ecosystem compatibility; prefer upstream crates when they support 0.19.

## UI work

- In-game UI lives under `src/ui/` (pause menu, HUD, finish screens, toasts). Minimap is `src/minimap.rs` (render-to-texture), not the UI module tree.
- Visual checks: implement or extend scenarios in `src/ui/screenshot_harness.rs`, then `just ui-screenshots`.
- Harness builds a **minimal** app (UI plugins + stub scene), not the full level load — good for UI chrome, not for in-world gameplay shots.

## AI / training

- Default feature set has **no** `ai`. Releases do not enable it.
- AI code: `src/ai/` (observations, rewards, ZMQ bridge, navmesh, gizmos).
- Navmesh pathfinding is for training guidance; `populate_orb_targets_observation` may still use Euclidean distance for some observation fields — check `src/ai/navmesh.rs` before assuming navmesh paths are what the agent sees.
- `--features ai` may fail to compile until `vleue_navigator` / polyanya glam versions align with Bevy 0.19 (`vendor/README.md`).

## Testing & verification

- Prefer `cargo test` / `just test` for pure logic.
- Ghost determinism: `--ghost-test`, `--verify-ghost PATH` (see `src/ghost.rs`).
- After UI changes that affect layout or visibility, regenerate screenshots and spot-check pause/finish/toast states.
- Do not claim GPU/visual correctness from unit tests alone.

## Git / commits

- **Commit when a task is complete** (working tree should not leave finished work uncommitted by default).
- Use multi-line commit messages: short subject + body of *why*.
- Do not commit unless the user asked **or** the task is done and the change set is coherent (this project’s default: commit completed tasks).
- Never update git config; never force-push to `main`/`master`; never skip hooks (`--no-verify`).
- Do not push unless the user explicitly asks.

## Out of scope / caution

- Full Bevy version bumps: plan migrations, update `RELEASING.md` version notes if release docs still mention old engine versions, re-run screenshot baseline.
- Editing only `vendor/` “to make AI compile” without documenting remaining glam/polyanya risk is incomplete — state residual breakage clearly.
- Audio can hang on some Linux setups; default to `--no-audio` in automation and CI-like runs.
