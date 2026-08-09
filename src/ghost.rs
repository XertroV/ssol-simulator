use std::path::PathBuf;

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    world_serialization::WorldInstanceReady,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_rapier3d::prelude::*;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{
    ai_support::{AiActionInput, AiConfig},
    game_state::{self, FinishReached, GameState, GameWon, OrbParent, OrbPickedUp},
    key_mapping::{KeyAction, KeyMapping},
    orb_curriculum::OrbId,
    physics_interpolation::{InterpolationBundle, PhysicsTransform},
    player::{self, MovementSettings, Player, PlayerCamera, PlayerRespawnRequest},
    relativity::rel_material::{NeedsRelativisticMaterial, RelativisticMaterial, RelativisticObject},
    scene_loader::PlayerStart,
};

// ---------------------------------------------------------------------------
// Train / eval ghost archival
// ---------------------------------------------------------------------------

/// How aggressively to archive episode ghosts under [`GhostRecordConfig::out_dir`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostRecordMode {
    /// Never write train/eval ghosts (human play still uses XDG runs/PB).
    Off,
    /// Only all-orbs wins (`GameWon` / train success).
    Success,
    /// Wins always + every Nth non-win episode (N from `sample_fail_every`).
    Sample,
    /// Every finished episode (wins and fails). Use sparingly — disk heavy.
    All,
}

impl GhostRecordMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" => Ok(Self::Off),
            "success" | "wins" | "win" => Ok(Self::Success),
            "sample" => Ok(Self::Sample),
            "all" | "always" => Ok(Self::All),
            other => Err(format!(
                "unknown --ghost-record mode '{other}' (use off|success|sample|all)"
            )),
        }
    }
}

/// Optional CLI-driven archival of train/eval runs as `.ghost` files.
///
/// When `out_dir` is set, finished episodes (per `mode`) are written as
/// MessagePack ghosts for later `--verify-ghost` / `--render-mp4` / BC reuse.
#[derive(Resource, Clone, Debug)]
pub struct GhostRecordConfig {
    pub out_dir: PathBuf,
    pub mode: GhostRecordMode,
    /// For [`GhostRecordMode::Sample`]: save 1 in N failures (default 20).
    pub sample_fail_every: u32,
    /// Filename tag (model / run id), e.g. `n7_mix_80k`.
    pub tag: String,
    /// Also mirror into XDG `runs/` (default false when train archival is on).
    pub save_xdg_runs: bool,
    /// Skip personal-best ghost updates under XDG (default true for train bulk).
    pub skip_pb: bool,
}

#[derive(Resource, Default, Debug)]
pub(crate) struct GhostRecordCounters {
    fail_seen: u32,
    saved: u32,
}

/// Fired by the train harness when an episode ends so we can archive fails
/// (wins are also covered via [`GameWon`], but this carries seed/route metadata).
#[derive(Event, Clone, Debug)]
pub struct GhostEpisodeFinalize {
    pub success: bool,
    pub seed: u64,
    pub route_mode: String,
    pub episode_index: u32,
}

/// When present, [`GameWon`] skips `--ghost-out` archival so the train harness
/// can write a single file with seed/route metadata via [`GhostEpisodeFinalize`].
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct GhostDeferArchiveToFinalize;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

const IDLE_THRESHOLD: f32 = 0.01;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GhostFrame {
    position: [f32; 3],
    yaw: f32,
    pitch: f32,
    input_keys: u8,
    mouse_delta: [f32; 2],
    /// Exact quaternion Y and W components from the post-Writeback rotation.
    /// Avoids Euler round-trip precision loss when replaying rotation for Rapier.
    #[serde(default = "default_rotation_yw")]
    rotation_yw: [f32; 2],
}

fn default_rotation_yw() -> [f32; 2] {
    [0.0, 1.0] // identity rotation
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum GhostFrameEntry {
    Active(GhostFrame),
    Idle(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GhostOrbEvent {
    frame_index: u32,
    orb_id: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GhostRecording {
    version: u32,
    level_name: String,
    nb_orbs: u32,
    #[serde(default)]
    score: u32,
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    cheated: bool,
    /// Number of FixedUpdate ticks between reset and first player movement.
    /// Needed for verification replay so villager spawner timers match.
    #[serde(default)]
    pre_movement_ticks: u32,
    final_player_time: f32,
    final_world_time: f32,
    frames: Vec<GhostFrameEntry>,
    orb_events: Vec<GhostOrbEvent>,
    timestamp: u64,
}

// ---------------------------------------------------------------------------
// Runtime resources
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub(crate) struct GhostRecorder {
    recording: bool,
    frames: Vec<GhostFrameEntry>,
    orb_events: Vec<GhostOrbEvent>,
    tick_count: u32,
    idle_counter: u32,
    last_frame: Option<GhostFrame>,
    saved_on_win: bool,
    finalized: bool,
    /// Ticks elapsed before first movement (for villager sync in verification)
    pre_movement_ticks: u32,
    // Cached state: updated each tick so we survive GameState being reset
    // before our observer runs.
    nb_orbs: u32,
    last_score: u32,
    last_player_time: f32,
    last_world_time: f32,
    cheated: bool,
}

impl Default for GhostRecorder {
    fn default() -> Self {
        Self {
            recording: false,
            frames: Vec::new(),
            orb_events: Vec::new(),
            tick_count: 0,
            idle_counter: 0,
            last_frame: None,
            saved_on_win: false,
            finalized: false,
            pre_movement_ticks: 0,
            nb_orbs: 0,
            last_score: 0,
            last_player_time: 0.0,
            last_world_time: 0.0,
            cheated: false,
        }
    }
}

#[derive(Resource)]
struct GhostPlayback {
    recording: Option<GhostRecording>,
    entry_index: usize,
    idle_remaining: u32,
    tick_count: u32,
    playing: bool,
    finished: bool,
    ghost_entity: Option<Entity>,
    current_frame: Option<GhostFrame>,
}

impl Default for GhostPlayback {
    fn default() -> Self {
        Self {
            recording: None,
            entry_index: 0,
            idle_remaining: 0,
            tick_count: 0,
            playing: false,
            finished: false,
            ghost_entity: None,
            current_frame: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct GhostMouseCapture {
    pub delta: Vec2,
    consumed: bool,
}

// ---------------------------------------------------------------------------
// Verification replay resources
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct GhostVerifyState {
    recording: GhostRecording,
    entry_index: usize,
    idle_remaining: u32,
    tick_count: u32,
    current_frame: Option<GhostFrame>,
    max_divergence: f32,
    total_frames_checked: u32,
    failed: bool,
    /// Ticks to wait before feeding input (matches original pre-movement delay)
    pre_movement_wait: u32,
    /// Index into recording.orb_events for synced orb triggering
    orb_event_index: usize,
    /// Set true once verification result has been logged
    finished: bool,
    /// If true, exit the app immediately on completion (headless/ghost-test mode)
    auto_exit: bool,
}

#[derive(Resource)]
pub struct GhostReplayInput {
    pub input_keys: u8,
    pub mouse_delta: [f32; 2],
    pub expected_yaw: f32,
    pub expected_pitch: f32,
    /// Exact quaternion Y and W components from recording, avoiding Euler round-trip.
    pub rotation_yw: [f32; 2],
}

// ---------------------------------------------------------------------------
// ECS markers
// ---------------------------------------------------------------------------

#[derive(Component)]
struct Ghost;

#[derive(Component)]
struct GhostModel;

#[derive(Component)]
struct NeedsGhostMaterial;

#[derive(Component)]
struct VerifyResultOverlay;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct GhostPlugin;

impl Plugin for GhostPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GhostRecorder>()
            .init_resource::<GhostPlayback>()
            .init_resource::<GhostMouseCapture>()
            .init_resource::<GhostRecordCounters>()
            .add_systems(
                Startup,
                ghost_load_on_startup.after(game_state::set_orb_count),
            )
            .add_systems(
                FixedUpdate,
                (ghost_record_frame, ghost_advance_playback)
                    .chain()
                    .after(player::player_update_done)
                    .run_if(ghost_should_tick),
            )
            .add_systems(
                Update,
                (
                    ghost_capture_mouse.before(player::update_player_look),
                    ghost_spawn_entity,
                ),
            )
            .add_observer(ghost_record_orb_pickup)
            .add_observer(ghost_save_on_win)
            .add_observer(ghost_update_on_finish)
            .add_observer(ghost_reset_recorder)
            .add_observer(ghost_on_episode_finalize)
            .add_observer(swap_to_ghost_material);

        // Verification: reset on respawn
        app.add_observer(ghost_reset_verification);

        // Verification systems (only run when GhostVerifyState resource exists)
        // apply_rotation must run BEFORE PhysicsSet::SyncBackend so that Rapier sees
        // the correct rotation when it reads Changed<GlobalTransform>. During recording,
        // rotation is set in Update→PostUpdate before FixedUpdate ticks, so SyncBackend
        // naturally has the correct rotation. Verification must match this ordering.
        app.add_systems(
            FixedUpdate,
            (
                ghost_verify_feed_input
                    .before(PhysicsSet::SyncBackend),
                ghost_verify_apply_rotation
                    .after(ghost_verify_feed_input)
                    .before(PhysicsSet::SyncBackend),
                // Sync orb events before the physics pipeline. Commands::trigger()
                // is deferred, so we need a sync point between this and the player
                // chain for the OrbPickedUp observer to actually run.
                // Running before SyncBackend ensures the command flush happens before
                // physics and the player chain (which runs after Writeback).
                ghost_verify_sync_orbs
                    .after(ghost_verify_feed_input)
                    .before(PhysicsSet::SyncBackend),
                ghost_verify_check_position
                    .after(player::player_update_done),
            )
                .run_if(resource_exists::<GhostVerifyState>)
                .run_if(ghost_should_tick),
        );

        // Lorentz visual correction: runs after physics interpolation so the ghost
        // position is consistent with the relativistically-warped world geometry.
        app.add_systems(
            PostUpdate,
            ghost_apply_lorentz_visual
                .after(crate::physics_interpolation::interpolate_transforms),
        );

        // Verification result overlay (dismiss with Escape/Backspace)
        app.add_systems(
            Update,
            verify_result_dismiss
                .run_if(|q: Query<(), With<VerifyResultOverlay>>| q.single().is_ok()),
        );
    }
}

/// Run condition: ghost systems should tick only when player physics would tick.
/// Pauses during hard pause AND free-cam movement freeze.
fn ghost_should_tick(state: Res<GameState>) -> bool {
    !state.is_hard_paused && state.movement_frozen.is_none()
}

/// Install train/eval ghost archival from CLI (`--ghost-out`).
pub fn setup_ghost_record(app: &mut App, config: GhostRecordConfig) {
    // Canonicalize when possible so relative dirs aren't tied to a surprising CWD
    // (Python once pointed us at target/release/data/... via exe-relative paths).
    let mut config = config;
    if let Ok(abs) = std::fs::canonicalize(&config.out_dir) {
        config.out_dir = abs;
    } else if config.out_dir.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            config.out_dir = cwd.join(&config.out_dir);
        }
    }
    info!(
        "Ghost archival: dir={} mode={:?} sample_fail_every={} tag={}",
        config.out_dir.display(),
        config.mode,
        config.sample_fail_every,
        config.tag
    );
    if let Err(e) = std::fs::create_dir_all(&config.out_dir) {
        warn!(
            "Ghost archival: failed to create {}: {e}",
            config.out_dir.display()
        );
    }
    app.insert_resource(config);
}

/// Called from main.rs after parsing CLI args to set up verification mode.
pub fn setup_verify_ghost(app: &mut App, path: &str, auto_exit: bool) {
    let recording = match load_ghost_file(path) {
        Some(r) => r,
        None => {
            error!("Failed to load ghost file for verification: {}", path);
            std::process::exit(1);
        }
    };
    info!(
        "Ghost verification: loaded {} frames, final_player_time={:.2}, pre_movement_ticks={}",
        recording.frames.len(),
        recording.final_player_time,
        recording.pre_movement_ticks,
    );
    let pre_movement_ticks = recording.pre_movement_ticks;
    app.insert_resource(GhostVerifyState {
        recording: recording.clone(),
        entry_index: 0,
        idle_remaining: 0,
        tick_count: 0,
        current_frame: None,
        max_divergence: 0.0,
        total_frames_checked: 0,
        failed: false,
        pre_movement_wait: pre_movement_ticks,
        orb_event_index: 0,
        finished: false,
        auto_exit,
    });
    app.insert_resource(GhostReplayInput {
        input_keys: 0,
        mouse_delta: [0.0, 0.0],
        expected_yaw: 0.0,
        expected_pitch: 0.0,
        rotation_yw: [0.0, 1.0],
    });
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

fn ghost_dir() -> Option<PathBuf> {
    ProjectDirs::from("io", "xertrov", "ssol-simulator")
        .map(|dirs| dirs.data_local_dir().join("ghosts"))
}

fn ghost_file_path(nb_orbs: u32) -> Option<PathBuf> {
    ghost_dir().map(|dir| dir.join(format!("level-zero-{}_orbs.ghost", nb_orbs)))
}

fn save_ghost_file(recording: &GhostRecording) -> bool {
    let Some(path) = ghost_file_path(recording.nb_orbs) else {
        warn!("Could not determine ghost file path");
        return false;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("Failed to create ghost directory: {}", e);
            return false;
        }
    }
    match rmp_serde::to_vec(recording) {
        Ok(data) => match std::fs::write(&path, data) {
            Ok(_) => {
                info!(
                    "Ghost saved: {} ({} frames, {:.2}s player time)",
                    path.display(),
                    recording.frames.len(),
                    recording.final_player_time
                );
                true
            }
            Err(e) => {
                warn!("Failed to write ghost file: {}", e);
                false
            }
        },
        Err(e) => {
            warn!("Failed to serialize ghost recording: {}", e);
            false
        }
    }
}

fn runs_dir() -> Option<PathBuf> {
    ProjectDirs::from("io", "xertrov", "ssol-simulator")
        .map(|dirs| dirs.data_local_dir().join("runs"))
}

/// Build an ISO-8601-ish filename for a run recording.
/// Format: `{ISO date}-{nb_orbs}orbs-{score}of{nb_orbs}-{frames}f[-win].ghost`
fn run_file_name(recording: &GhostRecording) -> String {
    // Format as 2026-03-28T12-00-00 (hyphens instead of colons for filesystem safety)
    let datetime: String = {
        let secs = recording.timestamp;
        // Use time arithmetic to produce UTC components
        let days = secs / 86400;
        let rem = secs % 86400;
        let h = rem / 3600;
        let m = (rem % 3600) / 60;
        let s = rem % 60;
        // Days since epoch to Y-M-D (simplified civil_from_days)
        let (y, mo, d) = civil_from_days(days as i64);
        format!(
            "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
            y, mo, d, h, m, s
        )
    };
    let frame_count = recording.frames.len();
    let win_suffix = if recording.completed { "-win" } else { "" };
    format!(
        "{}-{}orbs-{}of{}-{}f{}.ghost",
        datetime, recording.nb_orbs, recording.score, recording.nb_orbs, frame_count, win_suffix
    )
}

/// Convert days since Unix epoch to (year, month, day). Adapted from Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Save a recording to the runs/ directory for historical archival.
/// Returns true on success.
fn save_run_file(recording: &GhostRecording) -> bool {
    let Some(dir) = runs_dir() else {
        warn!("Could not determine runs directory path");
        return false;
    };
    write_ghost_bytes(&dir.join(run_file_name(recording)), recording, "Run")
}

/// Write a MessagePack ghost to an arbitrary path.
fn write_ghost_bytes(path: &std::path::Path, recording: &GhostRecording, label: &str) -> bool {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("Failed to create {} dir {}: {e}", label, parent.display());
            return false;
        }
    }
    match rmp_serde::to_vec(recording) {
        Ok(data) => match std::fs::write(path, data) {
            Ok(_) => {
                info!(
                    "{label} saved: {} ({} frames, {:.2}s, {}/{} orbs{})",
                    path.display(),
                    recording.frames.len(),
                    recording.final_player_time,
                    recording.score,
                    recording.nb_orbs,
                    if recording.completed { ", completed" } else { "" }
                );
                true
            }
            Err(e) => {
                warn!("Failed to write {label} {}: {e}", path.display());
                false
            }
        },
        Err(e) => {
            warn!("Failed to serialize {label}: {e}");
            false
        }
    }
}

/// Rich filename for train/eval ghosts under `--ghost-out`.
fn archive_ghost_name(
    recording: &GhostRecording,
    tag: &str,
    meta: Option<&GhostEpisodeFinalize>,
) -> String {
    let tag = if tag.is_empty() { "run" } else { tag };
    let tag = tag
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let outcome = if recording.completed { "win" } else { "fail" };
    let (seed, route, ep) = match meta {
        Some(m) => (m.seed, m.route_mode.as_str(), m.episode_index),
        None => (0u64, "unknown", 0u32),
    };
    let route = route
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let t_ms = (recording.final_player_time * 1000.0).round() as u32;
    format!(
        "{tag}_ep{ep}_s{seed}_{route}_n{nb}_{score}of{nb}_{outcome}_t{t_ms}ms_{frames}f.ghost",
        tag = tag,
        ep = ep,
        seed = seed,
        route = route,
        nb = recording.nb_orbs,
        score = recording.score,
        outcome = outcome,
        t_ms = t_ms,
        frames = recording.frames.len(),
    )
}

fn should_archive(
    config: &GhostRecordConfig,
    success: bool,
    counters: &mut GhostRecordCounters,
) -> bool {
    match config.mode {
        GhostRecordMode::Off => false,
        GhostRecordMode::Success => success,
        GhostRecordMode::All => true,
        GhostRecordMode::Sample => {
            if success {
                true
            } else {
                let n = config.sample_fail_every.max(1);
                counters.fail_seen = counters.fail_seen.saturating_add(1);
                counters.fail_seen % n == 0
            }
        }
    }
}

/// Archive under `--ghost-out` when the record policy allows it.
fn maybe_archive_ghost(
    recording: &GhostRecording,
    success: bool,
    meta: Option<&GhostEpisodeFinalize>,
    config: Option<&GhostRecordConfig>,
    counters: &mut GhostRecordCounters,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    if config.mode == GhostRecordMode::Off {
        return false;
    }
    if !should_archive(config, success, counters) {
        info!(
            "Ghost archive skip: mode={:?} success={} fail_seen={} (sample every {})",
            config.mode, success, counters.fail_seen, config.sample_fail_every
        );
        return false;
    }
    let name = archive_ghost_name(recording, &config.tag, meta);
    let path = config.out_dir.join(name);
    if write_ghost_bytes(&path, recording, "Ghost archive") {
        counters.saved = counters.saved.saturating_add(1);
        // Sidecar JSON for quick grepping without decoding msgpack.
        // `with_extension("ghost.json")` would replace `.ghost` → use full suffix.
        let side = PathBuf::from(format!("{}.json", path.display()));
        let side_obj = serde_json::json!({
            "path": path.display().to_string(),
            "tag": config.tag,
            "success": success,
            "completed": recording.completed,
            "nb_orbs": recording.nb_orbs,
            "score": recording.score,
            "final_player_time": recording.final_player_time,
            "frames": recording.frames.len(),
            "orb_events": recording.orb_events.len(),
            "seed": meta.map(|m| m.seed),
            "route_mode": meta.map(|m| m.route_mode.clone()),
            "episode_index": meta.map(|m| m.episode_index),
            "timestamp": recording.timestamp,
        });
        if let Ok(s) = serde_json::to_string_pretty(&side_obj) {
            let _ = std::fs::write(side, s);
        }
        // Also print a machine-readable line on stdout so train harnesses see it
        // even when sim stderr is discarded.
        println!(
            "GHOST_ARCHIVED {}",
            serde_json::json!({
                "path": path.display().to_string(),
                "success": success,
                "score": recording.score,
                "nb_orbs": recording.nb_orbs,
                "frames": recording.frames.len(),
                "final_player_time": recording.final_player_time,
            })
        );
        let _ = std::io::Write::flush(&mut std::io::stdout());
        true
    } else {
        false
    }
}

/// Synchronously archive the in-progress episode buffer.
///
/// Must run **before** `AppExit` / process kill: the train harness used to only
/// `commands.trigger(GhostEpisodeFinalize)`, but Python's env `close()` sends
/// SIGKILL immediately after the terminal TRAIN_STEP_JSON, so deferred observers
/// never ran and `data/.../ghosts/` stayed empty.
pub(crate) fn archive_episode_now(
    recorder: &mut GhostRecorder,
    meta: GhostEpisodeFinalize,
    config: Option<&GhostRecordConfig>,
    counters: &mut GhostRecordCounters,
) -> bool {
    if config.is_none() {
        return false;
    }
    if recorder.cheated {
        warn!("Ghost archive skip: cheated run");
        return false;
    }
    if recorder.finalized {
        info!("Ghost archive skip: recorder already finalized");
        return false;
    }
    if !recorder.recording || (recorder.frames.is_empty() && recorder.idle_counter == 0) {
        warn!(
            "Ghost archive skip: nothing recorded (recording={} frames={} idle={} ticks={})",
            recorder.recording,
            recorder.frames.len(),
            recorder.idle_counter,
            recorder.tick_count
        );
        return false;
    }

    let success = meta.success || recorder.saved_on_win;
    let recording = build_recording(recorder, success);
    let wrote = maybe_archive_ghost(&recording, success, Some(&meta), config, counters);
    if success {
        recorder.saved_on_win = true;
    }
    recorder.finalized = true;
    wrote
}

fn load_ghost_file(path: &str) -> Option<GhostRecording> {
    let data = std::fs::read(path).ok()?;
    rmp_serde::from_slice(&data).ok()
}

fn load_ghost_for_level(nb_orbs: u32) -> Option<GhostRecording> {
    let path = ghost_file_path(nb_orbs)?;
    if !path.exists() {
        return None;
    }
    let data = std::fs::read(&path).ok()?;
    match rmp_serde::from_slice(&data) {
        Ok(recording) => {
            info!("Ghost loaded from {}", path.display());
            Some(recording)
        }
        Err(e) => {
            warn!("Failed to deserialize ghost file {}: {}", path.display(), e);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Recording systems
// ---------------------------------------------------------------------------

fn ghost_capture_mouse(
    mouse: Res<AccumulatedMouseMotion>,
    mut capture: ResMut<GhostMouseCapture>,
) {
    capture.delta = mouse.delta;
    capture.consumed = false;
}

/// Threshold continuous AI move_dir into the WASD bitmask used by ghost replay.
/// Sign convention matches `player::calculate_player_acceleration` AI branch:
/// +move_dir.y → Forward (0x1), −y → Back (0x2), −x → Left (0x4), +x → Right (0x8).
const AI_MOVE_BIT_THRESHOLD: f32 = 0.25;

fn ghost_record_frame(
    mut recorder: ResMut<GhostRecorder>,
    q_player: Query<(&Transform, &Velocity), With<Player>>,
    q_camera: Query<&Transform, (With<PlayerCamera>, Without<Player>)>,
    state: Res<GameState>,
    input: Res<ButtonInput<KeyCode>>,
    mapping: Res<KeyMapping>,
    mut capture: ResMut<GhostMouseCapture>,
    verify: Option<Res<GhostVerifyState>>,
    ai_config: Option<Res<AiConfig>>,
    ai_input: Option<Res<AiActionInput>>,
) {
    if verify.is_some() || recorder.finalized {
        return;
    }

    let Ok((player_transform, velocity)) = q_player.single() else {
        return;
    };
    let Ok(camera_transform) = q_camera.single() else {
        return;
    };

    // Start recording on first movement
    if !recorder.recording {
        if state.player_time > 0.0 || velocity.linear.length_squared() > 0.0 {
            recorder.recording = true;
            recorder.nb_orbs = state.nb_orbs;
        } else {
            recorder.pre_movement_ticks += 1;
            return;
        }
    }

    // Cache state each tick so it survives GameState reset before our observer
    recorder.last_score = state.score;
    recorder.last_player_time = state.player_time;
    recorder.last_world_time = state.world_time;
    recorder.cheated = state.used_cheat_99_orbs;

    let ai_enabled = ai_config.as_ref().map(|c| c.enabled).unwrap_or(false);

    // Build input_keys bitmask (keyboard and/or thresholded AI move_dir)
    let mut input_keys: u8 = 0;
    let mut mouse_delta = [0.0_f32, 0.0];

    if ai_enabled {
        if let Some(ref ai) = ai_input {
            let t = AI_MOVE_BIT_THRESHOLD;
            if ai.move_dir.y > t {
                input_keys |= 0x1; // forward
            }
            if ai.move_dir.y < -t {
                input_keys |= 0x2; // back
            }
            if ai.move_dir.x < -t {
                input_keys |= 0x4; // left
            }
            if ai.move_dir.x > t {
                input_keys |= 0x8; // right
            }
            // Train applies yaw in FixedUpdate (look stays 0); store look if set.
            mouse_delta = [ai.look.x, ai.look.y];
        }
    } else {
        if mapping.pressed(&input, KeyAction::Forward) {
            input_keys |= 0x1;
        }
        if mapping.pressed(&input, KeyAction::Backward) {
            input_keys |= 0x2;
        }
        if mapping.pressed(&input, KeyAction::Left) {
            input_keys |= 0x4;
        }
        if mapping.pressed(&input, KeyAction::Right) {
            input_keys |= 0x8;
        }
        // Read mouse delta (only first FixedUpdate tick per frame gets real delta)
        mouse_delta = if !capture.consumed {
            capture.consumed = true;
            [capture.delta.x, capture.delta.y]
        } else {
            [0.0, 0.0]
        };
    }

    // Extract yaw and pitch
    let (yaw, _, _) = player_transform.rotation.to_euler(EulerRot::YXZ);
    let (_, pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);

    // AFK check — during AI, also treat pure yaw-only frames as active so look
    // changes are not collapsed into Idle (rotation is still in the Active frame).
    let moving = velocity.linear.length_squared() >= IDLE_THRESHOLD;
    if input_keys == 0 && mouse_delta == [0.0, 0.0] && !moving {
        recorder.idle_counter += 1;
        recorder.tick_count += 1;
        return;
    }

    // Flush pending idle ticks
    if recorder.idle_counter > 0 {
        let idle_count = recorder.idle_counter;
        recorder.frames.push(GhostFrameEntry::Idle(idle_count));
        recorder.idle_counter = 0;
    }

    let frame = GhostFrame {
        position: player_transform.translation.into(),
        yaw,
        pitch,
        input_keys,
        mouse_delta,
        rotation_yw: [player_transform.rotation.y, player_transform.rotation.w],
    };
    recorder.last_frame = Some(frame.clone());
    recorder.frames.push(GhostFrameEntry::Active(frame));
    recorder.tick_count += 1;
}

fn ghost_record_orb_pickup(
    trigger: On<OrbPickedUp>,
    mut recorder: ResMut<GhostRecorder>,
    q_orb_ids: Query<&OrbId>,
) {
    let orb_entity = trigger.event().0;
    if let Ok(orb_id) = q_orb_ids.get(orb_entity) {
        let frame_index = recorder.tick_count;
        let id = orb_id.0;
        recorder.orb_events.push(GhostOrbEvent {
            frame_index,
            orb_id: id,
        });
    }
}

fn build_recording(
    recorder: &GhostRecorder,
    completed: bool,
) -> GhostRecording {
    let mut frames = recorder.frames.clone();
    // Flush trailing idle if any
    if recorder.idle_counter > 0 {
        frames.push(GhostFrameEntry::Idle(recorder.idle_counter));
    }
    GhostRecording {
        version: 2,
        level_name: "level-zero".to_string(),
        nb_orbs: recorder.nb_orbs,
        score: recorder.last_score,
        completed,
        cheated: recorder.cheated,
        pre_movement_ticks: recorder.pre_movement_ticks,
        final_player_time: recorder.last_player_time,
        final_world_time: recorder.last_world_time,
        frames,
        orb_events: recorder.orb_events.clone(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    }
}

fn ghost_save_on_win(
    _trigger: On<GameWon>,
    mut recorder: ResMut<GhostRecorder>,
    playback: Res<GhostPlayback>,
    verify: Option<Res<GhostVerifyState>>,
    archive: Option<Res<GhostRecordConfig>>,
    mut counters: ResMut<GhostRecordCounters>,
    defer_finalize: Option<Res<GhostDeferArchiveToFinalize>>,
) {
    if verify.is_some() {
        return;
    }
    if recorder.cheated {
        info!("Ghost: skipping save (cheated run)");
        return;
    }
    if recorder.saved_on_win {
        return;
    }

    let recording = build_recording(&recorder, true);

    // When the train harness is active it emits GhostEpisodeFinalize with seed/route
    // metadata — archive there instead so we do not write two files per win.
    if defer_finalize.is_none() {
        maybe_archive_ghost(
            &recording,
            true,
            None,
            archive.as_deref(),
            &mut counters,
        );
    }

    let skip_xdg = archive
        .as_ref()
        .is_some_and(|c| !c.save_xdg_runs);
    let skip_pb = archive.as_ref().is_some_and(|c| c.skip_pb);

    if !skip_xdg {
        // Always save to runs/ for historical archival (human / default)
        save_run_file(&recording);
    }

    if !skip_pb {
        // Save as PB only if better than existing
        let dominated_by_existing = playback
            .recording
            .as_ref()
            .is_some_and(|existing| existing.final_player_time <= recording.final_player_time);

        if !dominated_by_existing {
            save_ghost_file(&recording);
        } else {
            info!(
                "Ghost: not saving PB (existing {:.2}s <= current {:.2}s)",
                playback.recording.as_ref().unwrap().final_player_time,
                recording.final_player_time
            );
        }
    }

    recorder.saved_on_win = true;
}

/// Observer fallback (e.g. if something else triggers finalize without sync path).
fn ghost_on_episode_finalize(
    trigger: On<GhostEpisodeFinalize>,
    mut recorder: ResMut<GhostRecorder>,
    verify: Option<Res<GhostVerifyState>>,
    archive: Option<Res<GhostRecordConfig>>,
    mut counters: ResMut<GhostRecordCounters>,
) {
    if verify.is_some() {
        return;
    }
    let meta = trigger.event().clone();
    let _ = archive_episode_now(
        &mut recorder,
        meta,
        archive.as_deref(),
        &mut counters,
    );
}

fn ghost_update_on_finish(
    _trigger: On<FinishReached>,
    mut recorder: ResMut<GhostRecorder>,
    mut playback: ResMut<GhostPlayback>,
    verify: Option<Res<GhostVerifyState>>,
) {
    if verify.is_some() {
        return;
    }
    if recorder.cheated {
        recorder.finalized = true;
        return;
    }
    if recorder.saved_on_win && !recorder.finalized {
        let recording = build_recording(&recorder, true);
        // Update run file with post-win frames (through the arch)
        save_run_file(&recording);
        save_ghost_file(&recording);
        playback.recording = Some(recording);
    }
    recorder.finalized = true;
}

fn ghost_reset_recorder(
    _trigger: On<PlayerRespawnRequest>,
    mut recorder: ResMut<GhostRecorder>,
    mut playback: ResMut<GhostPlayback>,
    mut commands: Commands,
    verify: Option<Res<GhostVerifyState>>,
    archive: Option<Res<GhostRecordConfig>>,
    mut counters: ResMut<GhostRecordCounters>,
) {
    if verify.is_some() {
        // During verification, skip all saving but still reset state below
    } else if !recorder.cheated && !recorder.finalized {
        // Save current run (partial or won-without-arch)
        // Skip if player never moved (no frames recorded)
        if recorder.recording && !recorder.frames.is_empty() {
            let completed = recorder.saved_on_win;
            let recording = build_recording(&recorder, completed);

            maybe_archive_ghost(
                &recording,
                completed,
                None,
                archive.as_deref(),
                &mut counters,
            );

            let skip_xdg = archive
                .as_ref()
                .is_some_and(|c| !c.save_xdg_runs);
            let skip_pb = archive.as_ref().is_some_and(|c| c.skip_pb);

            if !skip_xdg {
                save_run_file(&recording);
            }

            // If won but didn't reach arch, also update the PB file
            if completed && !skip_pb {
                save_ghost_file(&recording);
                playback.recording = Some(recording);
            }
        }
    }

    // Check if nb_orbs changed (curriculum change) and reload ghost
    let current_nb_orbs = recorder.nb_orbs;
    let loaded_nb_orbs = playback
        .recording
        .as_ref()
        .map(|r| r.nb_orbs)
        .unwrap_or(0);
    if loaded_nb_orbs != current_nb_orbs {
        playback.recording = load_ghost_for_level(current_nb_orbs);
    }

    // Despawn ghost entity
    if let Some(entity) = playback.ghost_entity.take() {
        commands.entity(entity).try_despawn();
    }

    // Reset recorder
    *recorder = GhostRecorder::default();

    // Reset playback state (keep loaded recording)
    let recording = playback.recording.take();
    *playback = GhostPlayback {
        recording,
        ..default()
    };
}

// ---------------------------------------------------------------------------
// Playback systems
// ---------------------------------------------------------------------------

fn ghost_spawn_entity(
    mut commands: Commands,
    mut playback: ResMut<GhostPlayback>,
    asset_server: Res<AssetServer>,
    q_start: Query<&Transform, With<PlayerStart>>,
) {
    if playback.recording.is_none() || playback.ghost_entity.is_some() {
        return;
    }

    let Ok(start_transform) = q_start.single() else {
        return;
    };

    let model_path = "models/MovingPerson.gltf";
    let ghost_transform = Transform {
        translation: start_transform.translation,
        rotation: start_transform.rotation,
        scale: start_transform.scale,
    };

    let entity = commands
        .spawn((
            Ghost,
            ghost_transform,
            GlobalTransform::default(),
            Visibility::Visible,
            Name::new("Ghost"),
            InterpolationBundle::from_transform(&ghost_transform),
        ))
        .with_children(|p| {
            p.spawn((
                GhostModel,
                NeedsGhostMaterial,
                WorldAssetRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset(model_path)),
                ),
                Transform::from_scale(1.0 / start_transform.scale * 0.775)
                    .with_translation(start_transform.translation * 0.7 + Vec3::Y * 0.11),
                Visibility::Inherited,
                Name::new("GhostModel"),
            ));
        })
        .id();

    playback.ghost_entity = Some(entity);
    info!("Ghost entity spawned");
}

fn ghost_advance_playback(
    mut playback: ResMut<GhostPlayback>,
    q_player: Query<&Velocity, With<Player>>,
    mut q_ghost: Query<&mut Transform, With<Ghost>>,
    state: Res<GameState>,
) {
    if playback.recording.is_none() || playback.finished {
        return;
    }

    // Wait for live player's first movement
    if !playback.playing {
        let Ok(velocity) = q_player.single() else {
            return;
        };
        if state.player_time > 0.0 || velocity.linear.length_squared() > 0.0 {
            playback.playing = true;
        } else {
            return;
        }
    }

    // Handle idle remaining
    if playback.idle_remaining > 0 {
        playback.idle_remaining -= 1;
        playback.tick_count += 1;
        if playback.idle_remaining == 0 {
            playback.entry_index += 1;
        }
        return;
    }

    // Advance to next entry
    let frames_len = playback.recording.as_ref().unwrap().frames.len();
    if playback.entry_index >= frames_len {
        playback.finished = true;
        return;
    }

    let entry = playback.recording.as_ref().unwrap().frames[playback.entry_index].clone();
    match entry {
        GhostFrameEntry::Active(frame) => {
            // Apply frame to ghost entity
            if let Some(entity) = playback.ghost_entity {
                if let Ok(mut ghost_transform) = q_ghost.get_mut(entity) {
                    ghost_transform.translation =
                        Vec3::new(frame.position[0], frame.position[1], frame.position[2]);
                    ghost_transform.rotation = Quat::from_axis_angle(Vec3::Y, frame.yaw);
                }
            }
            playback.current_frame = Some(frame);
            playback.tick_count += 1;
            playback.entry_index += 1;
        }
        GhostFrameEntry::Idle(count) => {
            if count <= 1 {
                playback.tick_count += 1;
                playback.entry_index += 1;
            } else {
                playback.idle_remaining = count - 1;
                playback.tick_count += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ghost visual (material swap)
// ---------------------------------------------------------------------------

fn swap_to_ghost_material(
    trigger: On<WorldInstanceReady>,
    mut commands: Commands,
    q_children: Query<&Children>,
    q_needs_ghost: Query<Entity, With<NeedsGhostMaterial>>,
    q_std_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    q_rel_mat: Query<Entity, With<MeshMaterial3d<RelativisticMaterial>>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ent = trigger.entity;
    if !q_needs_ghost.contains(ent) {
        return;
    }

    for child in q_children.iter_descendants(ent) {
        // Remove relativistic material if it was swapped first
        if q_rel_mat.contains(child) {
            commands
                .entity(child)
                .remove::<MeshMaterial3d<RelativisticMaterial>>()
                .remove::<RelativisticObject>();
        }

        if q_std_mat.get(child).is_ok() {
            let ghost_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.7, 0.7, 0.7, 0.35),
                base_color_texture: None,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            });
            commands
                .entity(child)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(ghost_mat));
        }
    }

    commands
        .entity(ent)
        .remove::<NeedsGhostMaterial>()
        .remove::<NeedsRelativisticMaterial>();
}

// ---------------------------------------------------------------------------
// Startup: load ghost
// ---------------------------------------------------------------------------

fn ghost_load_on_startup(
    state: Res<GameState>,
    mut playback: ResMut<GhostPlayback>,
    verify: Option<Res<GhostVerifyState>>,
) {
    // Don't load PB ghost when running in verify mode — only the CLI-supplied ghost matters
    if verify.is_some() {
        return;
    }
    let recording = load_ghost_for_level(state.nb_orbs);
    if let Some(ref r) = recording {
        info!(
            "Loaded PB ghost: {:.2}s, {} frames, {} orbs",
            r.final_player_time,
            r.frames.len(),
            r.nb_orbs
        );
    } else {
        info!("No PB ghost found for {} orbs", state.nb_orbs);
    }
    playback.recording = recording;
}

// ---------------------------------------------------------------------------
// Verification systems
// ---------------------------------------------------------------------------

fn ghost_reset_verification(
    _trigger: On<PlayerRespawnRequest>,
    verify: Option<ResMut<GhostVerifyState>>,
    replay_input: Option<ResMut<GhostReplayInput>>,
) {
    let Some(mut verify) = verify else { return };
    info!(
        "Ghost verification restarting ({} frames checked so far, max divergence {:.4})",
        verify.total_frames_checked, verify.max_divergence,
    );
    let pre_movement_ticks = verify.recording.pre_movement_ticks;
    verify.entry_index = 0;
    verify.idle_remaining = 0;
    verify.tick_count = 0;
    verify.current_frame = None;
    verify.max_divergence = 0.0;
    verify.total_frames_checked = 0;
    verify.failed = false;
    verify.pre_movement_wait = pre_movement_ticks;
    verify.orb_event_index = 0;
    verify.finished = false;

    if let Some(mut replay_input) = replay_input {
        replay_input.input_keys = 0;
        replay_input.mouse_delta = [0.0, 0.0];
        replay_input.expected_yaw = 0.0;
        replay_input.expected_pitch = 0.0;
        replay_input.rotation_yw = [0.0, 1.0];
    }
}

fn ghost_verify_feed_input(
    mut verify: ResMut<GhostVerifyState>,
    mut replay_input: ResMut<GhostReplayInput>,
    q_player: Query<(&Velocity, &Transform), With<Player>>,
    q_player_disabled: Query<(), (With<Player>, With<RigidBodyDisabled>)>,
) {
    if verify.finished {
        return;
    }
    let Ok((_velocity, _player_tf)) = q_player.single() else {
        return;
    };

    // Don't feed input while player still has RigidBodyDisabled (post-respawn frame)
    if q_player_disabled.single().is_ok() {
        replay_input.input_keys = 0;
        replay_input.mouse_delta = [0.0, 0.0];
        return;
    }

    // Wait pre-movement ticks so villager spawner timers match the original run
    if verify.pre_movement_wait > 0 {
        verify.pre_movement_wait -= 1;
        replay_input.input_keys = 0;
        replay_input.mouse_delta = [0.0, 0.0];
        return;
    }

    // Handle idle remaining
    if verify.idle_remaining > 0 {
        verify.idle_remaining -= 1;
        verify.tick_count += 1;
        // During idle, no input
        replay_input.input_keys = 0;
        replay_input.mouse_delta = [0.0, 0.0];
        if let Some(ref frame) = verify.current_frame {
            replay_input.expected_yaw = frame.yaw;
            replay_input.expected_pitch = frame.pitch;
        }
        if verify.idle_remaining == 0 {
            verify.entry_index += 1;
        }
        return;
    }

    if verify.entry_index >= verify.recording.frames.len() {
        // All frames consumed — check will handle finish
        replay_input.input_keys = 0;
        replay_input.mouse_delta = [0.0, 0.0];
        return;
    }

    match &verify.recording.frames[verify.entry_index] {
        GhostFrameEntry::Active(frame) => {
            replay_input.input_keys = frame.input_keys;
            replay_input.mouse_delta = frame.mouse_delta;
            replay_input.expected_yaw = frame.yaw;
            replay_input.expected_pitch = frame.pitch;
            replay_input.rotation_yw = frame.rotation_yw;
            verify.current_frame = Some(frame.clone());
            verify.tick_count += 1;
            verify.entry_index += 1;
        }
        GhostFrameEntry::Idle(count) => {
            let count = *count;
            replay_input.input_keys = 0;
            replay_input.mouse_delta = [0.0, 0.0];
            if let Some(ref frame) = verify.current_frame {
                replay_input.expected_yaw = frame.yaw;
                replay_input.expected_pitch = frame.pitch;
                replay_input.rotation_yw = frame.rotation_yw;
            }
            if count <= 1 {
                verify.tick_count += 1;
                verify.entry_index += 1;
            } else {
                verify.idle_remaining = count - 1;
                verify.tick_count += 1;
            }
        }
    }
}

/// Applies the expected rotation from the ghost recording each FixedUpdate tick.
/// This ensures frame-rate-independent verification: each physics tick gets the
/// exact rotation from the recording, regardless of how many ticks run per frame.
fn ghost_verify_apply_rotation(
    replay_input: Res<GhostReplayInput>,
    verify: Res<GhostVerifyState>,
    mut q_player: Query<(&mut Transform, &mut GlobalTransform), With<Player>>,
    mut q_camera: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    if verify.finished {
        return;
    }
    let Ok((mut player_tf, mut player_gtf)) = q_player.single_mut() else {
        return;
    };
    let Ok(mut camera_tf) = q_camera.single_mut() else {
        return;
    };

    // Use the exact quaternion from the recording to avoid Euler→Quat round-trip
    // precision loss. The rotation_yw stores the exact Y and W components of the
    // post-Writeback quaternion, so Rapier gets an identical rotation for collision detection.
    let [qy, qw] = replay_input.rotation_yw;
    let new_rotation = Quat::from_xyzw(0.0, qy, 0.0, qw).normalize();
    player_tf.rotation = new_rotation;
    // Also update GlobalTransform so Rapier's SyncBackend (which reads Changed<GlobalTransform>)
    // picks up rotation changes within the FixedUpdate loop where Bevy's transform propagation
    // doesn't run (it only runs in PostUpdate).
    *player_gtf = GlobalTransform::from(*player_tf);
    camera_tf.rotation = Quat::from_axis_angle(
        Vec3::X,
        replay_input
            .expected_pitch
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2),
    );
}

/// Syncs orb pickup events during verification by triggering OrbPickedUp at the
/// exact recorded frame indices. This ensures game state (speed_of_light, lorentz_factor,
/// etc.) matches the recording, preventing velocity divergence from different orb timing.
/// Runs before detect_orb_collisions so the orb is hidden before collision detection.
fn ghost_verify_sync_orbs(
    mut commands: Commands,
    mut verify: ResMut<GhostVerifyState>,
    mut q_orb_vis: Query<(Entity, &OrbId, &mut Visibility), With<OrbParent>>,
) {
    if verify.finished {
        return;
    }
    // The orb_event.frame_index was stored as recorder.tick_count at the time of collection,
    // which is the value BEFORE ghost_record_frame increments it. During verification,
    // ghost_verify_feed_input has already incremented tick_count for this tick.
    // So the matching condition is: frame_index == tick_count - 1.
    let current_tick = verify.tick_count.saturating_sub(1);

    while verify.orb_event_index < verify.recording.orb_events.len() {
        let orb_event = &verify.recording.orb_events[verify.orb_event_index];
        if orb_event.frame_index > current_tick {
            break;
        }
        let target_id = orb_event.orb_id;
        for (entity, orb_id, mut vis) in q_orb_vis.iter_mut() {
            if orb_id.0 == target_id && *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
                commands.trigger(game_state::OrbPickedUp(entity));
            }
        }
        verify.orb_event_index += 1;
    }
}

fn ghost_verify_check_position(
    mut commands: Commands,
    mut verify: ResMut<GhostVerifyState>,
    mut q_player: Query<
        (&mut Transform, &mut GlobalTransform, Option<&mut PhysicsTransform>),
        With<Player>,
    >,
    #[allow(unused_mut, unused_variables)]
    mut exit: MessageWriter<AppExit>,
    asset_server: Res<AssetServer>,
) {
    if verify.finished {
        return;
    }
    let Ok((mut player_transform, mut player_gtf, physics_tf)) =
        q_player.single_mut()
    else {
        return;
    };

    // Clone frame data to avoid borrow conflict with verify
    let frame_data = verify.current_frame.as_ref().map(|f| {
        Vec3::new(f.position[0], f.position[1], f.position[2])
    });

    if let Some(expected) = frame_data {
        let actual = player_transform.translation;
        let divergence = expected.distance(actual);

        if divergence > verify.max_divergence {
            verify.max_divergence = divergence;
        }

        verify.total_frames_checked += 1;

        // Periodic progress
        if verify.total_frames_checked % 1000 == 0 {
            info!(
                "Ghost verify progress: {} frames checked, max divergence: {:.4}",
                verify.total_frames_checked, verify.max_divergence
            );
        }

        // Per-frame threshold: flag individual frames with large divergence.
        // Collision drag directly modifies player velocity state based on Rapier
        // contact normals, which can differ slightly between recording and replay
        // even with identical starting positions (due to float-precision in contact
        // geometry). A tolerance of 0.5 catches real bugs (wrong input/rotation/
        // frame alignment) while allowing for physics collision sensitivity.
        const PER_FRAME_TOLERANCE: f32 = 0.5;
        if divergence > PER_FRAME_TOLERANCE {
            if verify.total_frames_checked > 10 {
                warn!(
                    "Ghost verify: per-frame divergence {:.6} at frame {} (expected {:?}, got {:?})",
                    divergence, verify.tick_count, expected, actual
                );
            }
            verify.failed = true;
        }

        // Snap player to expected position to prevent tiny float-precision divergence
        // from accumulating through collision boundaries. Without this, a ~0.00003
        // position difference can reach a collision boundary and cause completely
        // different collision responses, snowballing to >10 units of divergence.
        player_transform.translation = expected;
        *player_gtf = GlobalTransform::from(*player_transform);
        if let Some(mut ptf) = physics_tf {
            ptf.translation = expected;
        }
    }

    // Check if all frames consumed
    if !verify.finished
        && verify.entry_index >= verify.recording.frames.len()
        && verify.idle_remaining == 0
    {
        verify.finished = true;
        let passed = !verify.failed;
        let max_div = verify.max_divergence;
        let frames = verify.total_frames_checked;
        let duration = verify.recording.final_player_time;
        let orbs = verify.recording.orb_events.len();
        let nb_orbs = verify.recording.nb_orbs;

        if passed {
            info!("Ghost verification PASSED: max divergence {:.6} over {} frames", max_div, frames);
        } else {
            error!("Ghost verification FAILED: max divergence {:.6} over {} frames", max_div, frames);
        }

        if verify.auto_exit {
            // Use std::process::exit for immediate termination in headless mode.
            // The event-based AppExit only takes effect after the current FixedUpdate
            // batch completes, and with high speed multipliers that batch can contain
            // thousands of wasted ticks.
            std::process::exit(if passed { 0 } else { 1 });
        } else {
            spawn_verify_result_overlay(
                &mut commands, &asset_server, passed, max_div, frames, duration, orbs, nb_orbs,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Verification result overlay
// ---------------------------------------------------------------------------

fn spawn_verify_result_overlay(
    commands: &mut Commands,
    asset_server: &AssetServer,
    passed: bool,
    max_divergence: f32,
    frames_checked: u32,
    duration_secs: f32,
    orb_events: usize,
    nb_orbs: u32,
) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");
    let label_font = TextFont {
        font: (font.clone()).into(),
        font_size: FontSize::Px(20.0),
        ..default()
    };
    let big_font = TextFont {
        font: (font.clone()).into(),
        font_size: FontSize::Px(48.0),
        ..default()
    };
    let detail_font = TextFont {
        font: (Handle::default()).into(),
        font_size: FontSize::Px(16.0),
        ..default()
    };

    let (status_text, status_color) = if passed {
        ("VERIFICATION PASSED", Color::srgba(0.3, 0.95, 0.4, 1.0))
    } else {
        ("VERIFICATION FAILED", Color::srgba(0.98, 0.34, 0.34, 1.0))
    };

    let details = format!(
        "Max divergence:  {:.6}\n\
         Frames checked:  {}\n\
         Duration:        {:.2}s\n\
         Orb events:      {}/{}",
        max_divergence,
        frames_checked,
        duration_secs,
        orb_events,
        nb_orbs,
    );

    commands
        .spawn((
            VerifyResultOverlay,
            GlobalZIndex(940),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(520.0),
                    max_width: Val::Percent(88.0),
                    padding: UiRect::all(Val::Px(32.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(16.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.05, 0.07, 0.88)),
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.18)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Ghost Verification"),
                    label_font.clone(),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
                ));
                panel.spawn((
                    Text::new(status_text),
                    big_font,
                    TextColor(status_color),
                ));
                panel.spawn((
                    Text::new(details),
                    detail_font,
                    TextColor(Color::srgba(0.88, 0.91, 0.96, 0.82)),
                ));
                panel.spawn((
                    Text::new("Press Escape or Backspace to exit"),
                    label_font,
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
                ));
            });
        });
}

fn verify_result_dismiss(
    input: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
    verify: Option<Res<GhostVerifyState>>,
) {
    if input.just_pressed(KeyCode::Escape) || input.just_pressed(KeyCode::Backspace) {
        let code = match verify {
            Some(ref v) if v.failed => 1,
            _ => 0,
        };
        exit.write(AppExit::from_code(code));
    }
}

// ---------------------------------------------------------------------------
// Lorentz visual position correction
// ---------------------------------------------------------------------------

/// Replicates the relativistic vertex shader transformation for a single point.
/// Ensures the ghost position is visually consistent with the Lorentz-warped world.
///
/// This mirrors the logic in `assets/shaders/rel_shader.wgsl` vertex shader
/// for a stationary object (viw = 0).
fn lorentz_transform_point(
    point: Vec3,
    player_pos: Vec3,
    vpc: Vec3, // player velocity / speed_of_light (dimensionless)
    speed_of_light: f32,
) -> Vec3 {
    let speed_sq = vpc.length_squared();
    let speed = speed_sq.sqrt();

    if speed < 1e-6 {
        return point;
    }

    // Position relative to player
    let pos = point - player_pos;

    // Rotation to align vpc with -Z axis (same as shader)
    let a = -(-vpc.z / speed).clamp(-1.0, 1.0).acos();
    let cross_len = (vpc.x * vpc.x + vpc.y * vpc.y).sqrt();
    let (ux, uy) = if cross_len > 1e-8 {
        (vpc.y / cross_len, -vpc.x / cross_len)
    } else {
        (0.0, 0.0)
    };

    let ca = a.cos();
    let sa = a.sin();

    // Forward rotation: M * pos
    let riw = Vec3::new(
        pos.x * (ca + ux * ux * (1.0 - ca))
            + pos.y * (uy * ux * (1.0 - ca))
            + pos.z * (uy * sa),
        pos.x * (ux * uy * (1.0 - ca))
            + pos.y * (ca + uy * uy * (1.0 - ca))
            + pos.z * (-ux * sa),
        pos.x * (-uy * sa) + pos.y * (ux * sa) + pos.z * ca,
    );

    // Light travel time (viw=0): tisw = |pos| / c
    let dist = pos.length();
    let tisw = -(dist / speed_of_light); // negative = past

    // Lorentz transformation along Z
    let gamma = 1.0 / (1.0 - speed_sq).max(1e-10).sqrt();
    let delta_z = speed_of_light * speed * tisw;
    let riw = Vec3::new(riw.x, riw.y, (riw.z + delta_z) * gamma);

    // Inverse rotation: M^T * riw
    let result = Vec3::new(
        riw.x * (ca + ux * ux * (1.0 - ca))
            + riw.y * (ux * uy * (1.0 - ca))
            + riw.z * (-uy * sa),
        riw.x * (uy * ux * (1.0 - ca))
            + riw.y * (ca + uy * uy * (1.0 - ca))
            + riw.z * (ux * sa),
        riw.x * (uy * sa) + riw.y * (-ux * sa) + riw.z * ca,
    );

    result + player_pos
}

// ---------------------------------------------------------------------------
// Ghost Determinism Test
// ---------------------------------------------------------------------------

/// Resource that drives the ghost determinism test mode (--ghost-test).
/// Records a run with scripted bot input, then replays it in verification mode.
#[derive(Resource)]
pub struct GhostDeterminismTest {
    phase: TestPhase,
    frame_tick: u32,
    max_recording_ticks: u32,
    pre_wait_ticks: u32,
    varied_start: u32,
    recording_data: Option<GhostRecording>,
}

#[derive(PartialEq, Clone, Copy)]
enum TestPhase {
    PreWait,
    CollectOrbs,
    VariedMovement,
    TransitionSave,
    TransitionReset,
    TransitionSetup,
    Verifying,
}

impl Default for GhostDeterminismTest {
    fn default() -> Self {
        Self {
            phase: TestPhase::PreWait,
            frame_tick: 0,
            max_recording_ticks: 4000,
            pre_wait_ticks: 100, // 1 second at 100Hz
            varied_start: 0,
            recording_data: None,
        }
    }
}

/// Called from main.rs when --ghost-test is set.
pub fn setup_ghost_test(app: &mut App) {
    app.insert_resource(GhostDeterminismTest::default())
        .add_systems(Startup, ghost_test_spawn_window)
        .add_systems(
            Update,
            ghost_test_bot_input
                .before(ghost_capture_mouse)
                .before(player::update_player_look)
                .run_if(resource_exists::<GhostDeterminismTest>),
        )
        .add_systems(
            FixedUpdate,
            (
                ghost_test_clear_input
                    .before(player::player_update_start)
                    .run_if(resource_exists::<GhostDeterminismTest>),
                ghost_test_phase_check
                    .after(player::player_update_done)
                    .run_if(resource_exists::<GhostDeterminismTest>),
            ),
        );
}

/// Spawn a fake Window entity so update_player_look can read window_scale
/// in headless mode (no WinitPlugin = no real window).
fn ghost_test_spawn_window(
    mut commands: Commands,
    test: Option<Res<GhostDeterminismTest>>,
) {
    if test.is_none() {
        return;
    }
    commands.spawn((
        Window {
            resolution: bevy::window::WindowResolution::new(800, 600),
            ..default()
        },
        PrimaryWindow,
        CursorOptions {
            grab_mode: CursorGrabMode::Locked,
            visible: false,
            ..default()
        },
    ));
    info!("Ghost test: spawned fake window for headless mouse input");
}

/// FixedUpdate system: clears keyboard and mouse input during non-recording phases.
/// Prevents stale key presses (set by the bot in Update) from affecting the player
/// during transition phases when GhostReplayInput doesn't exist yet.
fn ghost_test_clear_input(
    test: Res<GhostDeterminismTest>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<AccumulatedMouseMotion>,
) {
    match test.phase {
        TestPhase::CollectOrbs | TestPhase::VariedMovement => {}
        _ => {
            keys.release_all();
            mouse.delta = Vec2::ZERO;
        }
    }
}

/// Update system: injects keyboard and mouse input for the test bot.
/// Runs before ghost_capture_mouse and update_player_look.
fn ghost_test_bot_input(
    test: Res<GhostDeterminismTest>,
    mut mouse: ResMut<AccumulatedMouseMotion>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    q_player: Query<&Transform, With<Player>>,
    q_orbs: Query<(&Transform, &Visibility), With<OrbParent>>,
    settings: Res<MovementSettings>,
    q_window: Query<&Window, With<PrimaryWindow>>,
) {
    keys.release_all();
    mouse.delta = Vec2::ZERO;

    match test.phase {
        TestPhase::CollectOrbs | TestPhase::VariedMovement => {}
        _ => return,
    }

    let Ok(player_tf) = q_player.single() else {
        return;
    };
    let Ok(window) = q_window.single() else {
        return;
    };
    let window_scale = window.height().min(window.width());

    if test.phase == TestPhase::CollectOrbs {
        // Navigate toward closest visible orb
        let player_pos = player_tf.translation;
        let mut closest: Option<(f32, Vec3)> = None;
        for (orb_tf, vis) in q_orbs.iter() {
            if *vis == Visibility::Hidden {
                continue;
            }
            let dist = player_pos.distance(orb_tf.translation);
            if closest.is_none() || dist < closest.unwrap().0 {
                closest = Some((dist, orb_tf.translation));
            }
        }

        if let Some((_, target_pos)) = closest {
            let to_target = Vec3::new(
                target_pos.x - player_pos.x,
                0.0,
                target_pos.z - player_pos.z,
            )
            .normalize_or_zero();

            if to_target.length_squared() > 0.001 {
                // forward = Quat::from_axis_angle(Y, yaw) * NEG_Z = (-sin(yaw), 0, -cos(yaw))
                let desired_yaw = f32::atan2(-to_target.x, -to_target.z);
                let (current_yaw, _, _) = player_tf.rotation.to_euler(EulerRot::YXZ);
                let mut yaw_delta = desired_yaw - current_yaw;
                // Normalize to [-pi, pi]
                yaw_delta = (yaw_delta + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;

                // yaw -= (delta.x * sens * scale).to_radians()
                // => delta.x = -yaw_delta.to_degrees() / (sens * scale)
                let mouse_x = -yaw_delta.to_degrees() / (settings.mouse_sens * window_scale);
                mouse.delta = Vec2::new(mouse_x, 0.0);
            }

            keys.press(KeyCode::KeyW);
        }
    } else {
        // VariedMovement: cycle through different input patterns
        let t = test.frame_tick.saturating_sub(test.varied_start);
        let pattern = t % 700;

        if pattern < 100 {
            // Forward
            keys.press(KeyCode::KeyW);
        } else if pattern < 200 {
            // Turn left + forward
            let turn_deg = 2.0;
            let mouse_x = -turn_deg / (settings.mouse_sens * window_scale);
            mouse.delta = Vec2::new(mouse_x, 0.0);
            keys.press(KeyCode::KeyW);
        } else if pattern < 300 {
            // Backward
            keys.press(KeyCode::KeyS);
        } else if pattern < 400 {
            // Strafe left
            keys.press(KeyCode::KeyA);
        } else if pattern < 500 {
            // Diagonal: forward + right
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::KeyD);
        } else if pattern < 600 {
            // Turn right + backward
            let turn_deg = -1.5;
            let mouse_x = -turn_deg / (settings.mouse_sens * window_scale);
            mouse.delta = Vec2::new(mouse_x, 0.0);
            keys.press(KeyCode::KeyS);
        } else {
            // Strafe right
            keys.press(KeyCode::KeyD);
        }
    }
}

/// FixedUpdate system: manages test phase transitions.
fn ghost_test_phase_check(
    mut test: ResMut<GhostDeterminismTest>,
    mut recorder: ResMut<GhostRecorder>,
    state: Res<GameState>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    test.frame_tick += 1;

    match test.phase {
        TestPhase::PreWait => {
            if test.frame_tick >= test.pre_wait_ticks {
                test.phase = TestPhase::CollectOrbs;
                info!("Ghost test: pre-wait complete, starting orb collection");
            }
        }
        TestPhase::CollectOrbs => {
            if state.score >= 4 {
                test.varied_start = test.frame_tick;
                test.phase = TestPhase::VariedMovement;
                info!(
                    "Ghost test: {} orbs collected at tick {}, starting varied movement",
                    state.score, test.frame_tick
                );
            } else if test.frame_tick >= test.max_recording_ticks {
                if state.score > 0 {
                    test.varied_start = test.frame_tick;
                    test.phase = TestPhase::VariedMovement;
                    warn!(
                        "Ghost test: timeout during orb collection ({} orbs), continuing with varied movement",
                        state.score
                    );
                } else {
                    error!("Ghost test: timeout with 0 orbs collected, aborting");
                    exit.write(AppExit::from_code(2));
                    return;
                }
            }
        }
        TestPhase::VariedMovement => {
            let varied_ticks = test.frame_tick - test.varied_start;
            if varied_ticks >= 1000 {
                test.phase = TestPhase::TransitionSave;
                info!(
                    "Ghost test: varied movement complete at tick {}, saving recording",
                    test.frame_tick
                );
            }
        }
        TestPhase::TransitionSave => {
            // Mark as cheated to prevent PB/run file saves on reset
            recorder.cheated = true;
            let recording = build_recording(&recorder, false);
            info!(
                "Ghost test: recording has {} entries, {:.2}s, {} orbs, {} pre-movement ticks",
                recording.frames.len(),
                recording.final_player_time,
                recording.score,
                recording.pre_movement_ticks,
            );
            test.recording_data = Some(recording);
            test.phase = TestPhase::TransitionReset;
        }
        TestPhase::TransitionReset => {
            commands.trigger(PlayerRespawnRequest);
            test.phase = TestPhase::TransitionSetup;
            info!("Ghost test: triggered respawn for verification");
        }
        TestPhase::TransitionSetup => {
            let Some(ref recording) = test.recording_data else {
                error!("Ghost test: no recording data for verification!");
                exit.write(AppExit::from_code(3));
                return;
            };
            let pre_movement_ticks = recording.pre_movement_ticks;
            commands.insert_resource(GhostVerifyState {
                recording: recording.clone(),
                entry_index: 0,
                idle_remaining: 0,
                tick_count: 0,
                current_frame: None,
                max_divergence: 0.0,
                total_frames_checked: 0,
                failed: false,
                pre_movement_wait: pre_movement_ticks,
                orb_event_index: 0,
                finished: false,
                auto_exit: true, // ghost-test always auto-exits
            });
            commands.insert_resource(GhostReplayInput {
                input_keys: 0,
                mouse_delta: [0.0, 0.0],
                expected_yaw: 0.0,
                expected_pitch: 0.0,
                rotation_yw: [0.0, 1.0],
            });
            let n_orb_events = recording.orb_events.len();
            test.phase = TestPhase::Verifying;
            info!("Ghost test: verification started ({} orb events)", n_orb_events);
        }
        TestPhase::Verifying => {
            // ghost_verify_check_position handles exit when done
        }
    }
}

// ---------------------------------------------------------------------------
// Lorentz visual position correction
// ---------------------------------------------------------------------------

/// PostUpdate system: adjusts the ghost's rendered position to match the
/// relativistic visual distortion applied to all world geometry by the shader.
fn ghost_apply_lorentz_visual(
    state: Res<GameState>,
    q_player: Query<&Transform, With<Player>>,
    mut q_ghost: Query<&mut Transform, (With<Ghost>, Without<Player>)>,
) {
    let Ok(player_transform) = q_player.single() else {
        return;
    };
    let Ok(mut ghost_transform) = q_ghost.single_mut() else {
        return;
    };

    let speed_sq =
        state.player_velocity_vector.length_squared() / (state.speed_of_light * state.speed_of_light);
    if speed_sq < 1e-8 {
        return; // No meaningful correction at near-zero velocity
    }

    let vpc = state.player_velocity_vector / state.speed_of_light;
    ghost_transform.translation = lorentz_transform_point(
        ghost_transform.translation,
        player_transform.translation,
        vpc,
        state.speed_of_light,
    );
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn ghost_determinism() {
        // Run the game binary with --ghost-test which records a bot run then
        // replays it through verification. Exits 0 on pass, non-zero on fail.
        let status = std::process::Command::new("cargo")
            .args([
                "run",
                "--",
                "--headless",
                "--no-audio",
                "--ghost-test",
                "--speed",
                "10",
                "--fps",
                "200",
            ])
            .status()
            .expect("Failed to run ghost determinism test");
        assert!(
            status.success(),
            "Ghost determinism test failed with exit code: {:?}",
            status.code()
        );
    }
}
