//! Phase 0 training harness: privileged obs, variable act rate, WR high-level,
//! scripted go-to baseline. Does **not** require `--features ai` (no ZMQ/navmesh).

mod obs;
mod route;
mod scripted;

pub use obs::PrivilegedObs;
pub use route::WrRoute;
pub use scripted::{scripted_go_to, TrainAction};

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use bevy::app::AppExit;
use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;
use bevy_rapier3d::prelude::{PhysicsSet, Velocity};

use crate::ai_support::{AiActionInput, AiConfig};
use crate::game_state::{FinishReached, GameState, OrbParent, OrbPickedUp};
use crate::orb_curriculum::OrbId;
use crate::player::{Player, PlayerCamera};
use crate::scene_loader::WhiteFinishArch;

use obs::{wrap_angle, yaw_toward};

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
    /// Exit the process when the episode ends (for headless smoke runs).
    pub exit_on_done: bool,
    /// Log metrics every N physics ticks (0 = only at end).
    pub log_every_ticks: u32,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scripted: true,
            act_hz: 10.0,
            max_episode_secs: 600.0,
            wr_route_path: PathBuf::from(DEFAULT_WR_ROUTE_PATH),
            exit_on_done: true,
            log_every_ticks: 500,
        }
    }
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
    pub done: bool,
    pub success: bool,
    pub timed_out: bool,
    pub start_instant: Option<Instant>,
    pub orbs_at_start: u32,
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
}

pub struct TrainPlugin;

impl Plugin for TrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrivilegedObs>()
            .init_resource::<TrainEpisode>()
            .init_resource::<TrainMetrics>()
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
        "Train: scripted={} act_hz={} (period={} ticks, dt={:.3}s) max_episode={}s",
        cfg.scripted,
        cfg.act_hz,
        cfg.control_period_ticks(),
        cfg.control_dt(),
        cfg.max_episode_secs
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
    route: Option<Res<WrRoute>>,
    game: Res<GameState>,
    mut episode: ResMut<TrainEpisode>,
    mut obs: ResMut<PrivilegedObs>,
    q_player: Query<(&Transform, &Velocity), With<Player>>,
    q_camera: Query<&Transform, (With<PlayerCamera>, Without<Player>)>,
    q_orbs: Query<
        (&OrbId, &GlobalTransform, &Visibility),
        (With<OrbParent>, Without<Disabled>),
    >,
    q_arch: Query<&GlobalTransform, With<WhiteFinishArch>>,
) {
    let Ok((player_tf, vel)) = q_player.single() else {
        return;
    };
    let pitch = q_camera
        .single()
        .ok()
        .map(|t| t.rotation.to_euler(EulerRot::YXZ).1)
        .unwrap_or(0.0);
    let yaw = player_tf.rotation.to_euler(EulerRot::YXZ).0;
    let pos = player_tf.translation;

    // Active curriculum orbs (not Disabled). Hidden ⇒ collected this episode.
    let mut active: HashSet<u8> = HashSet::new();
    let mut live_pos: std::collections::HashMap<u8, Vec3> = std::collections::HashMap::new();
    let mut nearest: Option<(f32, u8, Vec3)> = None;
    for (id, gt, vis) in q_orbs.iter() {
        if *vis == Visibility::Hidden {
            episode.collected.insert(id.0);
            continue;
        }
        active.insert(id.0);
        let p = gt.translation();
        live_pos.insert(id.0, p);
        let d = pos.distance_squared(p);
        if nearest.map(|(bd, _, _)| d < bd).unwrap_or(true) {
            nearest = Some((d, id.0, p));
        }
    }

    if episode.orbs_at_start == 0 && game.nb_orbs > 0 {
        episode.orbs_at_start = game.nb_orbs;
    }

    let collected = episode.collected.clone();
    let (target_id, target_pos) = if game.game_win || collected.len() as u32 >= game.nb_orbs {
        // Head for finish arch.
        let arch = q_arch
            .single()
            .map(|t| t.translation())
            .unwrap_or(Vec3::new(344.5, -4.5, -23.4));
        (None, Some(arch))
    } else if let Some(route) = route.as_ref() {
        if let Some(stop) = route.next_target(&collected, Some(&active)) {
            // Prefer live spawn position over WR JSON (curriculum/spawn may differ slightly).
            let p = live_pos
                .get(&stop.orb_id)
                .copied()
                .unwrap_or_else(|| stop.position());
            (Some(stop.orb_id), Some(p))
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
    };
}

fn decide_action(
    cfg: Res<TrainConfig>,
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
    if cfg.scripted {
        episode.held_action = scripted_go_to(&obs);
    }
    episode.act_step += 1;
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
    mut exit: MessageWriter<AppExit>,
    cfg: Res<TrainConfig>,
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
            "Train: tick={} act={} score={}/{} t={:.1}s target={:?} dist={:.1} yaw_err={:.2}",
            episode.tick,
            episode.act_step,
            game.score,
            game.nb_orbs,
            game.player_time,
            episode.target_orb_id,
            obs.target_dist,
            obs.target_yaw_err
        );
    }

    if episode.done && metrics.physics_ticks == 0 {
        // Fill metrics once.
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

        info!(
            "Train episode done: success={} timeout={} orbs={}/{} player_t={:.2}s ticks={} acts={} wall={:.2}s steps/s={:.0}",
            metrics.success,
            metrics.timed_out,
            metrics.orbs_collected,
            metrics.orbs_total,
            metrics.player_time,
            metrics.physics_ticks,
            metrics.act_steps,
            metrics.wall_secs,
            if wall > 0.0 {
                metrics.physics_ticks as f32 / wall
            } else {
                0.0
            }
        );

        if cfg.exit_on_done {
            let code = if metrics.success { 0 } else { 1 };
            exit.write(AppExit::from_code(code));
        }
    }
}
