//! Phase 0 training harness: privileged obs, variable act rate, multi-route
//! high-level, scripted go-to baseline. Does **not** require `--features ai`
//! (no ZMQ/navmesh).
//!
//! Private residual latent `z` ([`PolicyState`]) lives on the episode only.
//! **Bridges must not require `z` for reset/step observations** — it is not
//! part of [`PrivilegedObs`] / env export. Phase 0 uses [`IdentityLatent`]
//! (`f = 0`); learned residual updates come later.

mod latent;
mod obs;
mod rays;
mod reward;
mod route;
mod route_family;
mod scripted;

// Public train API for RL bridges / later tasks (may be unused in Phase 0 binary).
#[allow(unused_imports)]
pub use latent::{
    residual_apply, IdentityLatent, LatentUpdate, PolicyState, LATENT_DIM,
};
#[allow(unused_imports)]
pub use obs::{PrivilegedObs, OBS_DIM, OBS_SCHEMA_VERSION};
#[allow(unused_imports)]
pub use rays::{WALL_RAY_COUNT, MAX_RAY_DISTANCE};
pub use reward::{act_reward, finish_bonus_edge, RewardConfig};
pub use route::WrRoute;
pub use route_family::{sample_route, ActiveRoute, RouteMode};
pub use scripted::{scripted_go_to, TrainAction};

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use bevy::app::AppExit;
use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;
use bevy_rapier3d::prelude::{PhysicsSet, ReadRapierContext, Velocity};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::Serialize;

use crate::ai_support::{AiActionInput, AiConfig};
use crate::game_state::{FinishReached, GameState, OrbParent, OrbPickedUp};
use crate::orb_curriculum::OrbId;
use crate::player::{Player, PlayerCamera, PlayerRespawnRequest};
use crate::scene_loader::WhiteFinishArch;

use obs::{wrap_angle, yaw_toward};
use rays::cast_wall_rays;

/// Default path for the confirmed level-zero WR route (repo `assets/`).
pub const DEFAULT_WR_ROUTE_PATH: &str = "assets/wr_route_level_zero.json";

/// Physics fixed rate (must match `main.rs`).
pub const PHYSICS_HZ: f32 = 100.0;

#[derive(Resource, Clone, Debug)]
pub struct TrainConfig {
    pub enabled: bool,
    /// Use the scripted go-to-target controller.
    pub scripted: bool,
    /// Policy decision rate (Hz). Physics stays at 100 Hz; action is held between decisions.
    pub act_hz: f32,
    /// Max episode length in sim seconds (player_time / wall fixed time).
    pub max_episode_secs: f32,
    pub wr_route_path: PathBuf,
    /// High-level route family mode (default `Mix` for training generalization).
    pub route_mode: RouteMode,
    /// RNG seed for route sampling (and future stochastic env hooks).
    pub seed: u64,
    /// Exit the process when the episode ends (for headless smoke runs).
    pub exit_on_done: bool,
    /// Log metrics every N physics ticks (0 = only at end).
    pub log_every_ticks: u32,
    /// Emit a single JSON object line prefixed with `TRAIN_METRICS_JSON ` at episode end.
    pub metrics_json: bool,
    /// Number of episodes per process (multi-ep soft reset via respawn).
    pub num_episodes: u32,
    /// If set, append transition JSONL to this path (act-boundary MDP tuples).
    pub dump_transitions: Option<PathBuf>,
    /// Live step protocol: print TRAIN_STEP_JSON, read action JSON from stdin each act.
    pub train_stdio: bool,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scripted: true,
            act_hz: 10.0,
            max_episode_secs: 600.0,
            wr_route_path: PathBuf::from(DEFAULT_WR_ROUTE_PATH),
            // Prefer mix for train; use `--route-mode wr` for WR-only eval.
            route_mode: RouteMode::Mix,
            seed: 0,
            exit_on_done: true,
            log_every_ticks: 500,
            metrics_json: true,
            num_episodes: 1,
            dump_transitions: None,
            train_stdio: false,
        }
    }
}

/// One episode result as a machine-readable JSON object (JSONL-friendly).
///
/// Printed as: `TRAIN_METRICS_JSON {…}` so shell harnesses can `grep` + strip the prefix.
#[derive(Debug, Clone, Serialize)]
pub struct EpisodeMetricsLine {
    pub seed: u64,
    pub route_mode: String,
    pub num_orbs: u32,
    pub orbs: u32,
    pub success: bool,
    pub player_time: f32,
    pub wall_secs: f32,
    pub ticks: u32,
    /// Extra fields (ignored by minimal consumers; useful for throughput notes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed_out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps_per_sec: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_route_mode: Option<String>,
}

impl TrainConfig {
    pub fn control_period_ticks(&self) -> u32 {
        let hz = self.act_hz.max(0.1);
        ((PHYSICS_HZ / hz).round() as u32).max(1)
    }

    pub fn control_dt(&self) -> f32 {
        self.control_period_ticks() as f32 / PHYSICS_HZ
    }
}

#[derive(Resource, Debug, Default)]
pub struct TrainEpisode {
    pub tick: u32,
    pub act_step: u32,
    pub collected: HashSet<u8>,
    pub target_orb_id: Option<u8>,
    pub target_pos: Option<Vec3>,
    pub held_action: TrainAction,
    /// Private residual latent (not in PrivilegedObs / as_vec export).
    pub policy_state: PolicyState,
    pub done: bool,
    pub success: bool,
    pub timed_out: bool,
    pub start_instant: Option<Instant>,
    pub orbs_at_start: u32,
    /// Concrete route sampled once when active orbs are first seen.
    pub active_route: Option<ActiveRoute>,
    pub route_logged: bool,
    /// Target distance at previous act decision (for potential-based reward).
    pub prev_target_dist: f32,
    /// Score at previous act decision (orbs_gained = score - this).
    pub score_at_last_act: u32,
    /// Last computed act reward (for metrics / logging).
    pub last_act_reward: f32,
    /// Finish bonus already applied (game_win is sticky after last orb).
    pub finish_bonus_paid: bool,
    /// Episode index within this process (0-based).
    pub episode_index: u32,
    /// Target id at last act (for goal-switch shaping mask).
    pub prev_target_for_shaping: Option<Option<u8>>,
    /// True after metrics emitted for current episode (awaiting multi-ep reset).
    pub metrics_emitted: bool,
    /// Open transition: obs/action at last decision, closed on next act or episode end.
    pub pending_transition: Option<PendingTransition>,
}

/// Pending MDP step opened at an act decision, closed with reward at next act/end.
#[derive(Clone, Debug)]
pub struct PendingTransition {
    pub obs: Vec<f32>,
    pub action: [f32; 3],
    pub dist_at_open: f32,
    pub score_at_open: u32,
    pub target_at_open: Option<u8>,
}

#[derive(Resource, Debug, Default)]
pub struct TrainMetrics {
    pub orbs_collected: u32,
    pub orbs_total: u32,
    pub player_time: f32,
    pub physics_ticks: u32,
    pub act_steps: u32,
    pub success: bool,
    pub timed_out: bool,
    pub wall_secs: f32,
    pub final_target: Option<u8>,
    pub route_mode: Option<RouteMode>,
}

pub struct TrainPlugin;

impl Plugin for TrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrivilegedObs>()
            .init_resource::<TrainEpisode>()
            .init_resource::<TrainMetrics>()
            .init_resource::<RewardConfig>()
            .add_systems(Startup, train_startup)
            // After Rapier Writeback (so Transform edits stick) and before player
            // movement so AiActionInput + yaw are visible to acceleration.
            .add_systems(
                FixedUpdate,
                (
                    update_target_and_obs,
                    decide_action,
                    apply_action,
                    tick_episode,
                )
                    .chain()
                    .after(PhysicsSet::Writeback)
                    .before(crate::player::player_update_start)
                    .run_if(train_enabled),
            )
            .add_observer(on_orb_picked_up)
            .add_observer(on_finish_reached);
    }
}

fn train_enabled(cfg: Option<Res<TrainConfig>>) -> bool {
    cfg.map(|c| c.enabled).unwrap_or(false)
}

fn train_startup(
    mut commands: Commands,
    cfg: Res<TrainConfig>,
    mut ai_cfg: ResMut<AiConfig>,
    mut episode: ResMut<TrainEpisode>,
) {
    if !cfg.enabled {
        return;
    }

    match WrRoute::load_from_path(&cfg.wr_route_path) {
        Ok(route) => {
            info!(
                "Train: loaded WR route ({} stops, last={}) from {}",
                route.stops.len(),
                route.last_orb_id,
                cfg.wr_route_path.display()
            );
            commands.insert_resource(route);
        }
        Err(e) => {
            error!("Train: {e}");
            // Still enable AI control so the run is controllable; targets fall back to nearest.
            commands.insert_resource(WrRoute {
                stops: Vec::new(),
                last_orb_id: 0,
            });
        }
    }

    ai_cfg.enabled = true;
    ai_cfg.waiting_for_action = false;
    commands.insert_resource(AiActionInput::default());

    episode.start_instant = Some(Instant::now());
    info!(
        "Train: scripted={} act_hz={} (period={} ticks, dt={:.3}s) max_episode={}s route_mode={} seed={}",
        cfg.scripted,
        cfg.act_hz,
        cfg.control_period_ticks(),
        cfg.control_dt(),
        cfg.max_episode_secs,
        cfg.route_mode,
        cfg.seed
    );
}

fn on_orb_picked_up(
    trigger: On<OrbPickedUp>,
    mut episode: ResMut<TrainEpisode>,
    q_ids: Query<&OrbId, With<OrbParent>>,
    cfg: Option<Res<TrainConfig>>,
) {
    if !cfg.map(|c| c.enabled).unwrap_or(false) {
        return;
    }
    let ent = trigger.event().0;
    if let Ok(id) = q_ids.get(ent) {
        episode.collected.insert(id.0);
        info!("Train: collected orb_id={} ({} total)", id.0, episode.collected.len());
    }
}

fn on_finish_reached(
    _trigger: On<FinishReached>,
    mut episode: ResMut<TrainEpisode>,
    mut metrics: ResMut<TrainMetrics>,
    cfg: Res<TrainConfig>,
) {
    if !cfg.enabled || episode.done {
        return;
    }
    episode.done = true;
    episode.success = true;
    metrics.success = true;
    info!("Train: FinishReached — success");
}

fn update_target_and_obs(
    cfg: Res<TrainConfig>,
    wr: Option<Res<WrRoute>>,
    game: Res<GameState>,
    mut episode: ResMut<TrainEpisode>,
    mut obs: ResMut<PrivilegedObs>,
    q_player: Query<(Entity, &Transform, &Velocity), With<Player>>,
    q_camera: Query<&Transform, (With<PlayerCamera>, Without<Player>)>,
    q_orbs: Query<
        (&OrbId, &GlobalTransform, &Visibility),
        (With<OrbParent>, Without<Disabled>),
    >,
    q_arch: Query<&GlobalTransform, With<WhiteFinishArch>>,
    rapier: ReadRapierContext,
) {
    let Ok((player_ent, player_tf, vel)) = q_player.single() else {
        return;
    };
    let pitch = q_camera
        .single()
        .ok()
        .map(|t| t.rotation.to_euler(EulerRot::YXZ).1)
        .unwrap_or(0.0);
    let yaw = player_tf.rotation.to_euler(EulerRot::YXZ).0;
    let pos = player_tf.translation;

    let wall_rays = if let Ok(ctx) = rapier.single() {
        cast_wall_rays(player_tf, &ctx, player_ent)
    } else {
        [1.0; WALL_RAY_COUNT]
    };

    // Active curriculum orbs (not Disabled). Hidden ⇒ collected this episode.
    let mut active: HashSet<u8> = HashSet::new();
    let mut remaining_positions: Vec<(u8, Vec3)> = Vec::new();
    let mut nearest: Option<(f32, u8, Vec3)> = None;
    for (id, gt, vis) in q_orbs.iter() {
        if *vis == Visibility::Hidden {
            episode.collected.insert(id.0);
            continue;
        }
        active.insert(id.0);
        let p = gt.translation();
        remaining_positions.push((id.0, p));
        let d = pos.distance_squared(p);
        if nearest.map(|(bd, _, _)| d < bd).unwrap_or(true) {
            nearest = Some((d, id.0, p));
        }
    }

    if episode.orbs_at_start == 0 && game.nb_orbs > 0 {
        episode.orbs_at_start = game.nb_orbs;
    }

    // Sample ActiveRoute once when we first see active orbs (level loaded).
    if episode.active_route.is_none() && !remaining_positions.is_empty() {
        let wr_ref = wr.as_deref();
        let empty_wr = WrRoute {
            stops: Vec::new(),
            last_orb_id: 0,
        };
        let wr_for_sample = wr_ref.unwrap_or(&empty_wr);
        let mut rng = StdRng::seed_from_u64(cfg.seed.wrapping_add(episode.episode_index as u64));
        let sampled = sample_route(
            cfg.route_mode,
            wr_for_sample,
            &remaining_positions,
            &mut rng,
        );
        if !episode.route_logged {
            info!(
                "Train: route_mode={} (requested={}) stops={} dynamic_greedy={}",
                sampled.mode,
                cfg.route_mode,
                sampled.stops.len(),
                sampled.dynamic_greedy
            );
            episode.route_logged = true;
        }
        episode.active_route = Some(sampled);
    }

    let collected = episode.collected.clone();
    let (target_id, target_pos) = if game.game_win || collected.len() as u32 >= game.nb_orbs {
        // Head for finish arch.
        let arch = q_arch
            .single()
            .map(|t| t.translation())
            .unwrap_or(Vec3::new(344.5, -4.5, -23.4));
        (None, Some(arch))
    } else if let Some(ref active_route) = episode.active_route {
        if let Some((id, p)) =
            active_route.next_target(pos, &collected, &active, &remaining_positions)
        {
            (Some(id), Some(p))
        } else if let Some((_, id, p)) = nearest {
            (Some(id), Some(p))
        } else {
            (None, None)
        }
    } else if let Some((_, id, p)) = nearest {
        (Some(id), Some(p))
    } else {
        (None, None)
    };

    episode.target_orb_id = target_id;
    episode.target_pos = target_pos;

    let (target_rel, target_dist, yaw_err) = if let Some(tp) = target_pos {
        let rel = tp - pos;
        let dist = rel.length();
        let desired = yaw_toward(pos, tp);
        let err = wrap_angle(desired - yaw);
        (rel, dist, err)
    } else {
        (Vec3::ZERO, 0.0, 0.0)
    };

    *obs = PrivilegedObs {
        player_pos: pos,
        player_vel: vel.linear,
        yaw,
        pitch,
        speed: game.player_speed,
        speed_of_light: game.speed_of_light,
        speed_multiplier: game.speed_multiplier,
        lorentz: game.lorentz_factor,
        score: game.score,
        nb_orbs: game.nb_orbs,
        player_time: game.player_time,
        control_dt: cfg.control_dt(),
        target_rel,
        target_dist,
        target_yaw_err: yaw_err,
        target_orb_id: target_id,
        episode_tick: episode.tick,
        wall_rays,
    };
}

fn decide_action(
    cfg: Res<TrainConfig>,
    rew_cfg: Res<RewardConfig>,
    game: Res<GameState>,
    obs: Res<PrivilegedObs>,
    mut episode: ResMut<TrainEpisode>,
) {
    if episode.done {
        return;
    }
    let period = cfg.control_period_ticks();
    if episode.tick % period != 0 {
        return;
    }

    let obs_vec = obs.as_vec();
    let route_leaf = episode
        .active_route
        .as_ref()
        .map(|r| r.mode.as_str().to_string())
        .unwrap_or_else(|| cfg.route_mode.as_str().to_string());

    // Close previous pending transition with reward over the hold interval.
    if let Some(pending) = episode.pending_transition.take() {
        let rew = close_transition_reward(
            &rew_cfg,
            &obs,
            &game,
            &mut episode,
            &pending,
            /*terminal_success=*/ false,
        );
        episode.last_act_reward = rew;
        if let Some(ref path) = cfg.dump_transitions {
            append_transition(
                path,
                &TransitionLine {
                    schema: OBS_SCHEMA_VERSION,
                    episode: episode.episode_index,
                    seed: cfg.seed.wrapping_add(episode.episode_index as u64),
                    route_mode: route_leaf.clone(),
                    obs: pending.obs,
                    action: pending.action.to_vec(),
                    reward: rew,
                    next_obs: obs_vec.clone(),
                    done: false,
                    truncated: false,
                },
            );
        }
    }

    episode.score_at_last_act = game.score;
    episode.prev_target_dist = obs.target_dist;
    episode.prev_target_for_shaping = Some(episode.target_orb_id);

    if cfg.train_stdio {
        // Live RL: publish obs (+ reward for previous act) and wait for action.
        let step = StdioStep {
            obs: obs_vec.clone(),
            reward: episode.last_act_reward,
            done: false,
            truncated: false,
            episode: episode.episode_index,
            score: game.score,
            nb_orbs: game.nb_orbs,
        };
        if let Ok(json) = serde_json::to_string(&step) {
            println!("TRAIN_STEP_JSON {json}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        match read_stdio_action() {
            Some(a) => {
                episode.held_action = a;
            }
            None => {
                // Fallback to scripted if stdin closed / invalid.
                episode.held_action = scripted_go_to(&obs, &episode.policy_state);
            }
        }
        episode.policy_state =
            IdentityLatent.update(&episode.policy_state, &obs, &episode.held_action);
    } else if cfg.scripted {
        episode.held_action = scripted_go_to(&obs, &episode.policy_state);
        episode.policy_state =
            IdentityLatent.update(&episode.policy_state, &obs, &episode.held_action);
    }

    // Open new transition for this decision.
    episode.pending_transition = Some(PendingTransition {
        obs: obs_vec,
        action: episode.held_action.as_array(),
        dist_at_open: obs.target_dist,
        score_at_open: game.score,
        target_at_open: episode.target_orb_id,
    });

    if cfg.log_every_ticks > 0 && episode.act_step % 20 == 0 {
        info!(
            "Train: act={} tick={} last_rew={:.4} dist={:.1} front={:.2} target={:?}",
            episode.act_step + 1,
            episode.tick,
            episode.last_act_reward,
            obs.target_dist,
            rays::frontal_clearance(&obs.wall_rays),
            episode.target_orb_id
        );
    }
    episode.act_step += 1;
}

/// Reward for a pending action from open → current obs (or terminal).
fn close_transition_reward(
    rew_cfg: &RewardConfig,
    obs: &PrivilegedObs,
    game: &GameState,
    episode: &mut TrainEpisode,
    pending: &PendingTransition,
    terminal_success: bool,
) -> f32 {
    let target_changed = pending.target_at_open != episode.target_orb_id;
    let prev_dist = if target_changed {
        obs.target_dist // zero potential spike on goal switch
    } else {
        pending.dist_at_open
    };
    let orbs_gained = game.score.saturating_sub(pending.score_at_open);
    let (finished_now, paid_after) =
        finish_bonus_edge(game.game_win || terminal_success, episode.finish_bonus_paid);
    episode.finish_bonus_paid = paid_after;
    // Prefer arch success as terminal finish signal when available.
    let finished = finished_now || terminal_success;
    act_reward(rew_cfg, prev_dist, obs, orbs_gained, finished)
}

#[derive(Serialize)]
struct TransitionLine {
    schema: u32,
    episode: u32,
    seed: u64,
    route_mode: String,
    obs: Vec<f32>,
    action: Vec<f32>,
    reward: f32,
    next_obs: Vec<f32>,
    done: bool,
    truncated: bool,
}

#[derive(Serialize)]
struct StdioStep {
    obs: Vec<f32>,
    reward: f32,
    done: bool,
    truncated: bool,
    episode: u32,
    score: u32,
    nb_orbs: u32,
}

fn read_stdio_action() -> Option<TrainAction> {
    use std::io::{self, BufRead};
    let mut line = String::new();
    let stdin = io::stdin();
    if stdin.lock().read_line(&mut line).ok()? == 0 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let arr = v.get("action")?.as_array()?;
    if arr.len() < 3 {
        return None;
    }
    let mx = arr[0].as_f64()? as f32;
    let my = arr[1].as_f64()? as f32;
    let yaw = arr[2].as_f64()? as f32;
    Some(TrainAction {
        move_dir: Vec2::new(mx.clamp(-1.0, 1.0), my.clamp(-1.0, 1.0)),
        yaw_rate: yaw.clamp(-2.5, 2.5),
    })
}

fn append_transition(path: &std::path::Path, line: &TransitionLine) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            error!("Train: dump mkdir {}: {e}", parent.display());
            return;
        }
    }
    match serde_json::to_string(line) {
        Ok(s) => match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(mut f) => {
                if let Err(e) = writeln!(f, "{s}") {
                    error!("Train: dump write {}: {e}", path.display());
                }
            }
            Err(e) => error!("Train: dump open {}: {e}", path.display()),
        },
        Err(e) => error!("Train: failed to serialize transition: {e}"),
    }
}

fn apply_action(
    cfg: Res<TrainConfig>,
    episode: Res<TrainEpisode>,
    mut ai_input: ResMut<AiActionInput>,
    mut q_player: Query<(&mut Transform, &mut GlobalTransform), With<Player>>,
    time: Res<Time<Fixed>>,
) {
    if episode.done || !cfg.enabled {
        return;
    }
    let action = &episode.held_action;
    // Movement consumed by player FixedUpdate via AiActionInput.
    ai_input.move_dir = action.move_dir;
    // Avoid double-application in Update look path.
    ai_input.look = Vec2::ZERO;

    // Apply yaw rate in FixedUpdate so headless / high-speed runs stay consistent.
    // Convention: positive yaw_rate increases world yaw (same sign as target_yaw_err /
    // desired−current). Note player look path uses `yaw -= look.y`, so if bridging via
    // AiActionInput.look, pass look.y = -yaw_rate * dt.
    //
    // Must also write GlobalTransform: transform propagation only runs in PostUpdate,
    // but at high --speed many FixedUpdates run per frame. Rapier SyncBackend reads
    // GlobalTransform; without this, Writeback restores the pre-turn rotation every tick
    // (same pattern as ghost_verify_apply_rotation).
    if let Ok((mut tf, mut gtf)) = q_player.single_mut() {
        let (yaw, pitch, roll) = tf.rotation.to_euler(EulerRot::YXZ);
        let new_yaw = yaw + action.yaw_rate * time.delta_secs();
        tf.rotation = Quat::from_euler(EulerRot::YXZ, new_yaw, pitch, roll);
        *gtf = GlobalTransform::from(*tf);
    }
}

fn tick_episode(
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    cfg: Res<TrainConfig>,
    rew_cfg: Res<RewardConfig>,
    game: Res<GameState>,
    mut episode: ResMut<TrainEpisode>,
    mut metrics: ResMut<TrainMetrics>,
    obs: Res<PrivilegedObs>,
) {
    if !cfg.enabled {
        return;
    }
    episode.tick = episode.tick.saturating_add(1);

    // Timeout on sim time.
    if !episode.done && game.player_time >= cfg.max_episode_secs {
        episode.done = true;
        episode.timed_out = true;
        info!(
            "Train: timeout at player_time={:.1}s score={}/{}",
            game.player_time, game.score, game.nb_orbs
        );
    }

    if cfg.log_every_ticks > 0 && episode.tick % cfg.log_every_ticks == 0 && !episode.done {
        info!(
            "Train: tick={} act={} score={}/{} t={:.1}s target={:?} dist={:.1} yaw_err={:.2} front={:.2}",
            episode.tick,
            episode.act_step,
            game.score,
            game.nb_orbs,
            game.player_time,
            episode.target_orb_id,
            obs.target_dist,
            obs.target_yaw_err,
            rays::frontal_clearance(&obs.wall_rays)
        );
    }

    if episode.done && !episode.metrics_emitted {
        episode.metrics_emitted = true;

        let route_leaf = episode
            .active_route
            .as_ref()
            .map(|r| r.mode.as_str().to_string())
            .unwrap_or_else(|| cfg.route_mode.as_str().to_string());

        // Close last pending action with terminal flags (correct interval reward).
        if let Some(pending) = episode.pending_transition.take() {
            let terminal_success = episode.success;
            let rew = close_transition_reward(
                &rew_cfg,
                &obs,
                &game,
                &mut episode,
                &pending,
                terminal_success,
            );
            episode.last_act_reward = rew;
            if let Some(ref path) = cfg.dump_transitions {
                append_transition(
                    path,
                    &TransitionLine {
                        schema: OBS_SCHEMA_VERSION,
                        episode: episode.episode_index,
                        seed: cfg.seed.wrapping_add(episode.episode_index as u64),
                        route_mode: route_leaf,
                        obs: pending.obs,
                        action: pending.action.to_vec(),
                        reward: rew,
                        next_obs: obs.as_vec(),
                        done: episode.success,
                        truncated: episode.timed_out && !episode.success,
                    },
                );
            }
        }

        let wall = episode
            .start_instant
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        metrics.orbs_collected = game.score;
        metrics.orbs_total = game.nb_orbs;
        metrics.player_time = game.player_time;
        metrics.physics_ticks = episode.tick;
        metrics.act_steps = episode.act_step;
        metrics.success = episode.success;
        metrics.timed_out = episode.timed_out;
        metrics.wall_secs = wall;
        metrics.final_target = episode.target_orb_id;
        metrics.route_mode = episode.active_route.as_ref().map(|r| r.mode);

        let steps_per_sec = if wall > 0.0 {
            metrics.physics_ticks as f32 / wall
        } else {
            0.0
        };
        let route_mode_str = metrics
            .route_mode
            .map(|m| m.as_str())
            .unwrap_or("none");

        info!(
            "Train episode {} done: success={} timeout={} orbs={}/{} player_t={:.2}s ticks={} acts={} wall={:.2}s steps/s={:.0} route_mode={} seed={}",
            episode.episode_index,
            metrics.success,
            metrics.timed_out,
            metrics.orbs_collected,
            metrics.orbs_total,
            metrics.player_time,
            metrics.physics_ticks,
            metrics.act_steps,
            metrics.wall_secs,
            steps_per_sec,
            route_mode_str,
            cfg.seed
        );

        if cfg.metrics_json {
            let line = EpisodeMetricsLine {
                seed: cfg.seed.wrapping_add(episode.episode_index as u64),
                route_mode: route_mode_str.to_string(),
                num_orbs: metrics.orbs_total,
                orbs: metrics.orbs_collected,
                success: metrics.success,
                player_time: metrics.player_time,
                wall_secs: metrics.wall_secs,
                ticks: metrics.physics_ticks,
                timed_out: Some(metrics.timed_out),
                act_steps: Some(metrics.act_steps),
                steps_per_sec: Some(steps_per_sec),
                requested_route_mode: Some(cfg.route_mode.as_str().to_string()),
            };
            match serde_json::to_string(&line) {
                Ok(json) => {
                    println!("TRAIN_METRICS_JSON {json}");
                    info!("TRAIN_METRICS_JSON {json}");
                }
                Err(e) => error!("Train: failed to serialize metrics JSON: {e}"),
            }
        }

        let next_ep = episode.episode_index.saturating_add(1);
        if next_ep < cfg.num_episodes.max(1) {
            info!(
                "Train: starting episode {}/{}",
                next_ep + 1,
                cfg.num_episodes
            );
            // Soft multi-episode: respawn resets orbs/player/GameState.
            commands.trigger(PlayerRespawnRequest);
            episode.episode_index = next_ep;
            episode.tick = 0;
            episode.act_step = 0;
            episode.collected.clear();
            episode.target_orb_id = None;
            episode.target_pos = None;
            episode.held_action = TrainAction::default();
            episode.policy_state = PolicyState::zeros();
            episode.done = false;
            episode.success = false;
            episode.timed_out = false;
            episode.start_instant = Some(Instant::now());
            episode.orbs_at_start = 0;
            episode.active_route = None;
            episode.route_logged = false;
            episode.prev_target_dist = 0.0;
            episode.score_at_last_act = 0;
            episode.last_act_reward = 0.0;
            episode.finish_bonus_paid = false;
            episode.prev_target_for_shaping = None;
            episode.metrics_emitted = false;
            episode.pending_transition = None;
            *metrics = TrainMetrics::default();
        } else if cfg.exit_on_done || cfg.train_stdio {
            if cfg.train_stdio {
                let step = StdioStep {
                    obs: obs.as_vec(),
                    reward: episode.last_act_reward,
                    done: episode.success,
                    truncated: episode.timed_out && !episode.success,
                    episode: episode.episode_index,
                    score: game.score,
                    nb_orbs: game.nb_orbs,
                };
                if let Ok(json) = serde_json::to_string(&step) {
                    println!("TRAIN_STEP_JSON {json}");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
            }
            let code = if metrics.success { 0 } else { 1 };
            exit.write(AppExit::from_code(code));
        }
    }
}
