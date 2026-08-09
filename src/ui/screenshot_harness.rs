//! Test helpers for capturing screenshots of every in-game Bevy UI screen/state.
//!
//! Run via CLI:
//! ```text
//! cargo run -- --ui-screenshots screenshots/ui --no-audio
//! ```
//!
//! Or: `just ui-screenshots`

use std::fs;
use std::path::{Path, PathBuf};

use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    window::{WindowPlugin, WindowResolution},
    winit::{UpdateMode, WinitSettings},
};
use iyes_perf_ui::prelude::PerfUiPlugin;

use crate::{
    ai_support::ActionCounter,
    audio::AudioSettings,
    camera_switcher::ActiveCamera,
    config::GraphicsSettings,
    game_state::{GameState, OrbSplit},
    key_mapping::{KeyAction, KeyMapping},
    orb_curriculum::OrbId,
    player::MovementSettings,
    ui::{
        FinishScreenUiPlugin, InGameUiPlugin, PauseMenuUiPlugin, ToastUiPlugin,
        finish_screen::{FinishFlowState, FinishPhase},
        in_game::{BorderFlash, UiFlashEvent},
        pause_menu::{
            ConfirmChoice, FocusTarget, FooterAction, PauseMenuModal, PauseMenuState, SettingItem,
        },
        toasts::{ToastEntry, ToastEvent, ToastKind},
    },
};

/// Configuration for a UI screenshot capture run.
#[derive(Debug, Clone)]
pub struct UiScreenshotConfig {
    /// Directory where PNG files are written.
    pub output_dir: PathBuf,
    /// Window resolution used for capture.
    pub window_width: u32,
    pub window_height: u32,
    /// Frames to wait after startup before the first scenario (asset load / layout).
    pub boot_frames: u32,
    /// Frames to wait after applying a scenario before capturing (UI sync systems).
    pub settle_frames: u32,
    /// Frames to wait for a screenshot write before failing the scenario.
    pub capture_timeout_frames: u32,
}

impl Default for UiScreenshotConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("screenshots/ui"),
            window_width: 1600,
            window_height: 900,
            boot_frames: 12,
            settle_frames: 8,
            capture_timeout_frames: 120,
        }
    }
}

/// Identifier for a single UI screenshot scenario.
///
/// Every user-facing Bevy UI surface in `src/ui/` is represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiScreenshotScenario {
    /// Default in-game HUD (timer, orbs, speedometer).
    InGameHudDefault,
    /// In-game HUD with mid-run stats filled in.
    InGameHudMidRun,
    /// Border flash overlay (warning).
    InGameBorderFlash,
    /// Performance / physics HUD overlay enabled.
    InGamePerfHud,
    /// Info toast notification.
    ToastInfo,
    /// Warning toast notification.
    ToastWarning,
    /// Multiple stacked toasts.
    ToastStacked,
    /// Pause menu — settings focus (default open state).
    PauseMenuSettings,
    /// Pause menu — keybind row focused.
    PauseMenuKeybinds,
    /// Pause menu — footer button focused.
    PauseMenuFooter,
    /// Pause menu — rebind key capture modal.
    PauseMenuCaptureKey,
    /// Pause menu — confirm reset-all-bindings modal.
    PauseMenuConfirmReset,
    /// Finish flow — win HUD while still roaming (post all-orbs).
    FinishWonRoaming,
    /// Finish flow — white flash over win state.
    FinishWinFlash,
    /// Finish flow — end overlay with orb split table.
    FinishEndOverlay,
    /// Finish flow — end overlay with cheated world-time styling.
    FinishEndOverlayCheated,
    /// AI debug panels (requires `--features ai`).
    #[cfg(feature = "ai")]
    AiDebugPanels,
    /// AI "Waiting for AI..." indicator.
    #[cfg(feature = "ai")]
    AiWaitingIndicator,
}

impl UiScreenshotScenario {
    /// Stable file stem used for the screenshot PNG (no extension).
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::InGameHudDefault => "01_in_game_hud_default",
            Self::InGameHudMidRun => "02_in_game_hud_mid_run",
            Self::InGameBorderFlash => "03_in_game_border_flash",
            Self::InGamePerfHud => "04_in_game_perf_hud",
            Self::ToastInfo => "05_toast_info",
            Self::ToastWarning => "06_toast_warning",
            Self::ToastStacked => "07_toast_stacked",
            Self::PauseMenuSettings => "08_pause_menu_settings",
            Self::PauseMenuKeybinds => "09_pause_menu_keybinds",
            Self::PauseMenuFooter => "10_pause_menu_footer",
            Self::PauseMenuCaptureKey => "11_pause_menu_capture_key",
            Self::PauseMenuConfirmReset => "12_pause_menu_confirm_reset",
            Self::FinishWonRoaming => "13_finish_won_roaming",
            Self::FinishWinFlash => "14_finish_win_flash",
            Self::FinishEndOverlay => "15_finish_end_overlay",
            Self::FinishEndOverlayCheated => "16_finish_end_overlay_cheated",
            #[cfg(feature = "ai")]
            Self::AiDebugPanels => "17_ai_debug_panels",
            #[cfg(feature = "ai")]
            Self::AiWaitingIndicator => "18_ai_waiting_indicator",
        }
    }

    /// Human-readable description for logs / index files.
    pub fn description(self) -> &'static str {
        match self {
            Self::InGameHudDefault => "In-game HUD at default zeroed stats",
            Self::InGameHudMidRun => "In-game HUD with mid-run score, timers, and speed",
            Self::InGameBorderFlash => "In-game HUD with warning border flash active",
            Self::InGamePerfHud => "In-game HUD with performance/physics overlay",
            Self::ToastInfo => "Info toast notification (top-right)",
            Self::ToastWarning => "Warning toast notification (top-right)",
            Self::ToastStacked => "Stacked info + warning toasts",
            Self::PauseMenuSettings => "Pause menu open with settings focus",
            Self::PauseMenuKeybinds => "Pause menu with a keybind row focused",
            Self::PauseMenuFooter => "Pause menu with footer action focused",
            Self::PauseMenuCaptureKey => "Pause menu rebind-key capture modal",
            Self::PauseMenuConfirmReset => "Pause menu confirm reset-all-bindings modal",
            Self::FinishWonRoaming => "Win HUD after collecting all orbs (roaming)",
            Self::FinishWinFlash => "Win flash overlay after collecting all orbs",
            Self::FinishEndOverlay => "End-of-run overlay with orb split table",
            Self::FinishEndOverlayCheated => "End-of-run overlay with cheat-highlighted world time",
            #[cfg(feature = "ai")]
            Self::AiDebugPanels => "AI reward breakdown, orb checklist, actions, ray donut",
            #[cfg(feature = "ai")]
            Self::AiWaitingIndicator => "AI waiting-for-action centered indicator",
        }
    }
}

/// Full ordered list of scenarios covered by the harness.
pub fn all_ui_screenshot_scenarios() -> Vec<UiScreenshotScenario> {
    vec![
        UiScreenshotScenario::InGameHudDefault,
        UiScreenshotScenario::InGameHudMidRun,
        UiScreenshotScenario::InGameBorderFlash,
        UiScreenshotScenario::InGamePerfHud,
        UiScreenshotScenario::ToastInfo,
        UiScreenshotScenario::ToastWarning,
        UiScreenshotScenario::ToastStacked,
        UiScreenshotScenario::PauseMenuSettings,
        UiScreenshotScenario::PauseMenuKeybinds,
        UiScreenshotScenario::PauseMenuFooter,
        UiScreenshotScenario::PauseMenuCaptureKey,
        UiScreenshotScenario::PauseMenuConfirmReset,
        UiScreenshotScenario::FinishWonRoaming,
        UiScreenshotScenario::FinishWinFlash,
        UiScreenshotScenario::FinishEndOverlay,
        UiScreenshotScenario::FinishEndOverlayCheated,
        #[cfg(feature = "ai")]
        UiScreenshotScenario::AiDebugPanels,
        #[cfg(feature = "ai")]
        UiScreenshotScenario::AiWaitingIndicator,
    ]
}

/// Public helper: list of expected output filenames (for tests / external tooling).
#[allow(dead_code)]
pub fn expected_screenshot_filenames() -> Vec<String> {
    all_ui_screenshot_scenarios()
        .into_iter()
        .map(|s| format!("{}.png", s.file_stem()))
        .collect()
}

/// Entry point used by the CLI / justfile: build a minimal app, capture all scenarios, exit.
pub fn run_ui_screenshot_suite(config: UiScreenshotConfig) {
    if let Err(error) = fs::create_dir_all(&config.output_dir) {
        eprintln!(
            "Failed to create screenshot output directory {}: {error}",
            config.output_dir.display()
        );
        std::process::exit(1);
    }

    write_scenario_index(&config.output_dir);

    let scenarios = all_ui_screenshot_scenarios();
    info!(
        "UI screenshot suite: {} scenario(s) → {}",
        scenarios.len(),
        config.output_dir.display()
    );

    let mut app = App::new();

    let mut plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Open SSOL — UI Screenshots".into(),
            resolution: WindowResolution::new(config.window_width, config.window_height),
            resizable: false,
            ..default()
        }),
        ..default()
    });
    // Screenshots are silent; skip audio device init.
    plugins = plugins.disable::<bevy::audio::AudioPlugin>();

    app.add_plugins(plugins)
        .insert_resource(ClearColor(Color::srgb(0.12, 0.13, 0.16)))
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::Continuous,
        })
        .insert_resource(Time::<Fixed>::from_hz(100.0))
        // Resources required by UI plugins / sync systems
        .insert_resource(GameState::default())
        .insert_resource(GraphicsSettings {
            show_perf_hud: false,
            ..default()
        })
        .insert_resource(AudioSettings::default())
        .insert_resource(MovementSettings::default())
        .insert_resource(KeyMapping::default())
        .insert_resource(ActionCounter::default())
        // Required by pause-menu systems even when we force open state via resources.
        .insert_resource(ActiveCamera::default())
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin::default())
        .add_plugins(PerfUiPlugin)
        .add_plugins(InGameUiPlugin)
        .add_plugins(FinishScreenUiPlugin)
        .add_plugins(ToastUiPlugin)
        .add_plugins(PauseMenuUiPlugin);

    #[cfg(feature = "ai")]
    {
        use crate::ai::{
            AiActionInput, AiConfig, AiEpisodeControl, AiObservations, AiRewardSignal,
        };
        use crate::ui::AiDebugUiPlugin;
        use crate::SimConfig;

        app.insert_resource(SimConfig {
            headless: false,
            speed_multiplier: 1.0,
            target_fps: 60.0,
            ai_mode: true,
            ai_test: false,
            zmq_port: None,
            no_audio: true,
            instance_name: Some("ui-screenshots".into()),
            num_orbs: None,
            nearest_extra: None,
            verify_ghost: None,
            ghost_test: false,
            scripted_baseline: false,
            act_hz: 10.0,
            max_episode_secs: 120.0,
            wr_route: None,
            route_mode: "mix".into(),
            seed: 0,
            num_episodes: 1,
            dump_transitions: None,
            train_stdio: false,
            ghost_out: None,
            ghost_record: "sample".into(),
            ghost_sample_fail: 20,
            ghost_tag: "run".into(),
        })
        .insert_resource(AiConfig {
            enabled: true,
            waiting_for_action: false,
            ..default()
        })
        .insert_resource(AiObservations::default())
        .insert_resource(AiRewardSignal::default())
        .insert_resource(AiEpisodeControl::default())
        .insert_resource(AiActionInput::default())
        .add_plugins(AiDebugUiPlugin);
    }

    let exit = app
        .insert_resource(HarnessState::new(config, scenarios))
        .add_systems(Startup, setup_capture_scene)
        .add_systems(Update, drive_screenshot_harness)
        .run();

    if exit.is_error() {
        std::process::exit(1);
    }
}

fn write_scenario_index(output_dir: &Path) {
    let mut lines = vec![
        "# UI screenshot scenarios".to_string(),
        String::new(),
        "Generated by `ui::screenshot_harness`. Open the PNGs beside this file to inspect UI states."
            .to_string(),
        String::new(),
        "| File | Scenario | Description |".to_string(),
        "| --- | --- | --- |".to_string(),
    ];
    for scenario in all_ui_screenshot_scenarios() {
        lines.push(format!(
            "| `{}.png` | `{:?}` | {} |",
            scenario.file_stem(),
            scenario,
            scenario.description()
        ));
    }
    lines.push(String::new());
    lines.push("## Notes".to_string());
    lines.push(String::new());
    lines.push(
        "- Covers every Bevy UI surface under `src/ui/` (in-game HUD, pause menu, finish flow, toasts)."
            .to_string(),
    );
    lines.push(
        "- The minimap (`src/minimap.rs`) is a render-to-texture overlay, not part of the UI module tree, and is not captured here."
            .to_string(),
    );
    #[cfg(feature = "ai")]
    lines.push(
        "- Built with `--features ai`: AI debug panels and waiting indicator are included."
            .to_string(),
    );
    #[cfg(not(feature = "ai"))]
    lines.push(
        "- AI debug UI is feature-gated; run `just ui-screenshots-ai` to capture those screens."
            .to_string(),
    );
    lines.push(String::new());
    let path = output_dir.join("INDEX.md");
    if let Err(error) = fs::write(&path, lines.join("\n")) {
        warn!("Failed to write scenario index {}: {error}", path.display());
    }
}

fn setup_capture_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.5, 7.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-0.9)),
    ));

    // Quiet 3D backdrop so UI contrast is easy to inspect (not pure black).
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(24.0, 24.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.18, 0.2, 0.24))),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.4, 0.5))),
        Transform::from_xyz(0.0, 0.6, 0.0),
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessPhase {
    Boot,
    Settle,
    RequestCapture,
    WaitCapture,
    Advance,
    Done,
}

#[derive(Resource)]
struct HarnessState {
    config: UiScreenshotConfig,
    scenarios: Vec<UiScreenshotScenario>,
    index: usize,
    phase: HarnessPhase,
    frames_left: u32,
    pending_path: Option<PathBuf>,
    capture_requested: bool,
    successes: usize,
    failures: usize,
}

impl HarnessState {
    fn new(config: UiScreenshotConfig, scenarios: Vec<UiScreenshotScenario>) -> Self {
        let boot_frames = config.boot_frames;
        Self {
            config,
            scenarios,
            index: 0,
            phase: HarnessPhase::Boot,
            frames_left: boot_frames,
            pending_path: None,
            capture_requested: false,
            successes: 0,
            failures: 0,
        }
    }

    fn current(&self) -> Option<UiScreenshotScenario> {
        self.scenarios.get(self.index).copied()
    }

    fn path_for(&self, scenario: UiScreenshotScenario) -> PathBuf {
        self.config
            .output_dir
            .join(format!("{}.png", scenario.file_stem()))
    }
}

fn drive_screenshot_harness(world: &mut World) {
    let (phase, frames_left, index, total, capture_requested) = {
        let Some(state) = world.get_resource::<HarnessState>() else {
            return;
        };
        (
            state.phase,
            state.frames_left,
            state.index,
            state.scenarios.len(),
            state.capture_requested,
        )
    };

    match phase {
        HarnessPhase::Boot => {
            if frames_left > 0 {
                world.resource_mut::<HarnessState>().frames_left -= 1;
                return;
            }
            begin_scenario(world, 0);
        }
        HarnessPhase::Settle => {
            if frames_left > 0 {
                world.resource_mut::<HarnessState>().frames_left -= 1;
                return;
            }
            world.resource_mut::<HarnessState>().phase = HarnessPhase::RequestCapture;
        }
        HarnessPhase::RequestCapture => {
            if capture_requested {
                return;
            }
            let scenario = {
                let state = world.resource::<HarnessState>();
                state.current()
            };
            let Some(scenario) = scenario else {
                finish_suite(world);
                return;
            };
            let path = world.resource::<HarnessState>().path_for(scenario);
            let _ = fs::remove_file(&path);

            info!(
                "Capturing UI scenario {}/{}: {} → {}",
                index + 1,
                total,
                scenario.file_stem(),
                path.display()
            );

            {
                let mut state = world.resource_mut::<HarnessState>();
                state.pending_path = Some(path.clone());
                state.capture_requested = true;
                state.phase = HarnessPhase::WaitCapture;
                state.frames_left = state.config.capture_timeout_frames;
            }

            world.commands().spawn(Screenshot::primary_window()).observe(
                move |captured: On<ScreenshotCaptured>, mut harness: ResMut<HarnessState>| {
                    let Some(expected) = harness.pending_path.clone() else {
                        return;
                    };
                    match save_screenshot_image(&captured.image, &expected) {
                        Ok(()) => {
                            info!("Saved {}", expected.display());
                            harness.successes += 1;
                        }
                        Err(error) => {
                            error!("Failed to save {}: {error}", expected.display());
                            harness.failures += 1;
                        }
                    }
                    harness.phase = HarnessPhase::Advance;
                    harness.capture_requested = false;
                    harness.pending_path = None;
                },
            );
            world.flush();
        }
        HarnessPhase::WaitCapture => {
            if frames_left > 0 {
                world.resource_mut::<HarnessState>().frames_left -= 1;
                return;
            }
            let path = world
                .resource::<HarnessState>()
                .pending_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("<unknown>"));
            error!("Timed out waiting for screenshot {}", path.display());
            let mut state = world.resource_mut::<HarnessState>();
            state.failures += 1;
            state.phase = HarnessPhase::Advance;
            state.capture_requested = false;
            state.pending_path = None;
        }
        HarnessPhase::Advance => {
            let next = index + 1;
            if next >= total {
                finish_suite(world);
            } else {
                begin_scenario(world, next);
            }
        }
        HarnessPhase::Done => {}
    }
}

fn begin_scenario(world: &mut World, index: usize) {
    let scenario = {
        let state = world.resource::<HarnessState>();
        state.scenarios.get(index).copied()
    };
    let Some(scenario) = scenario else {
        finish_suite(world);
        return;
    };

    reset_ui_baseline(world);
    apply_scenario(world, scenario);

    let settle = world.resource::<HarnessState>().config.settle_frames;
    let mut state = world.resource_mut::<HarnessState>();
    state.index = index;
    state.phase = HarnessPhase::Settle;
    state.frames_left = settle;
    state.capture_requested = false;
    state.pending_path = None;
}

fn finish_suite(world: &mut World) {
    let (successes, failures, out_dir) = {
        let state = world.resource::<HarnessState>();
        (
            state.successes,
            state.failures,
            state.config.output_dir.display().to_string(),
        )
    };
    info!("UI screenshot suite finished: {successes} saved, {failures} failed → {out_dir}");
    world.resource_mut::<HarnessState>().phase = HarnessPhase::Done;
    world.write_message(AppExit::from_code(if failures == 0 { 0 } else { 1 }));
}

fn save_screenshot_image(image: &Image, path: &Path) -> Result<(), String> {
    let dyn_img = image
        .clone()
        .try_into_dynamic()
        .map_err(|e| format!("image conversion failed: {e:?}"))?;
    let rgb = dyn_img.to_rgb8();
    rgb.save(path).map_err(|e| format!("IO error: {e}"))
}

/// Reset all UI-driven resources to a quiet baseline before applying a scenario.
pub fn reset_ui_baseline(world: &mut World) {
    let toast_entities: Vec<Entity> = world
        .query_filtered::<Entity, With<ToastEntry>>()
        .iter(world)
        .collect();
    for entity in toast_entities {
        world.despawn(entity);
    }

    if let Some(mut flash) = world.get_resource_mut::<BorderFlash>() {
        flash.timer = None;
        flash.color = Color::linear_rgb(1.0, 1.0, 0.0);
    }

    if let Some(mut pause) = world.get_resource_mut::<PauseMenuState>() {
        *pause = PauseMenuState::default();
    }

    if let Some(mut finish) = world.get_resource_mut::<FinishFlowState>() {
        *finish = FinishFlowState::default();
    }

    if let Some(mut game) = world.get_resource_mut::<GameState>() {
        *game = sample_default_game_state();
    }

    if let Some(mut graphics) = world.get_resource_mut::<GraphicsSettings>() {
        *graphics = GraphicsSettings {
            show_perf_hud: false,
            ..default()
        };
    }

    if let Some(mut audio) = world.get_resource_mut::<AudioSettings>() {
        *audio = AudioSettings::default();
    }

    if let Some(mut movement) = world.get_resource_mut::<MovementSettings>() {
        *movement = MovementSettings::default();
    }

    if let Some(mut keys) = world.get_resource_mut::<KeyMapping>() {
        *keys = KeyMapping::default();
    }

    #[cfg(feature = "ai")]
    {
        use crate::ai::{AiActionInput, AiConfig, AiObservations, AiRewardSignal};

        if let Some(mut cfg) = world.get_resource_mut::<AiConfig>() {
            cfg.waiting_for_action = false;
            cfg.enabled = true;
            cfg.ray_height_offset = -2.0;
        }
        if let Some(mut obs) = world.get_resource_mut::<AiObservations>() {
            *obs = sample_ai_observations();
        }
        if let Some(mut reward) = world.get_resource_mut::<AiRewardSignal>() {
            *reward = sample_ai_rewards();
        }
        if let Some(mut action) = world.get_resource_mut::<AiActionInput>() {
            *action = AiActionInput {
                look: Vec2::ZERO,
                move_dir: Vec2::new(0.0, 1.0),
            };
        }
    }
}

/// Apply a single scenario's resource / event state. Used by the harness and unit-testable.
pub fn apply_scenario(world: &mut World, scenario: UiScreenshotScenario) {
    match scenario {
        UiScreenshotScenario::InGameHudDefault => {}
        UiScreenshotScenario::InGameHudMidRun => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_mid_run_game_state();
            }
        }
        UiScreenshotScenario::InGameBorderFlash => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_mid_run_game_state();
            }
            world.trigger(UiFlashEvent::warning());
        }
        UiScreenshotScenario::InGamePerfHud => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_mid_run_game_state();
            }
            if let Some(mut graphics) = world.get_resource_mut::<GraphicsSettings>() {
                graphics.show_perf_hud = true;
            }
        }
        UiScreenshotScenario::ToastInfo => {
            world.trigger(ToastEvent::info("Collected orb 12 — speed boost active"));
        }
        UiScreenshotScenario::ToastWarning => {
            world.trigger(ToastEvent::warning("Unbound FreeCam and added it to Forward."));
        }
        UiScreenshotScenario::ToastStacked => {
            world.trigger(ToastEvent {
                message: "Info: curriculum set to 10 orbs".into(),
                kind: ToastKind::Info,
                duration_secs: 10.0,
            });
            world.trigger(ToastEvent {
                message: "Warning: binding conflict resolved".into(),
                kind: ToastKind::Warning,
                duration_secs: 10.0,
            });
        }
        UiScreenshotScenario::PauseMenuSettings => {
            open_pause_menu(
                world,
                FocusTarget::Setting(SettingItem::MasterVolume),
                PauseMenuModal::None,
            );
            if let Some(mut audio) = world.get_resource_mut::<AudioSettings>() {
                audio.master_v = 0.65;
                audio.music_v = 0.4;
                audio.sfx_v = 0.8;
            }
        }
        UiScreenshotScenario::PauseMenuKeybinds => {
            open_pause_menu(
                world,
                FocusTarget::Keybind(KeyAction::FreeCam),
                PauseMenuModal::None,
            );
        }
        UiScreenshotScenario::PauseMenuFooter => {
            open_pause_menu(
                world,
                FocusTarget::Footer(FooterAction::ResetAllBindings),
                PauseMenuModal::None,
            );
        }
        UiScreenshotScenario::PauseMenuCaptureKey => {
            open_pause_menu(
                world,
                FocusTarget::Keybind(KeyAction::Forward),
                PauseMenuModal::CaptureKey(KeyAction::Forward),
            );
        }
        UiScreenshotScenario::PauseMenuConfirmReset => {
            open_pause_menu(
                world,
                FocusTarget::Footer(FooterAction::ResetAllBindings),
                PauseMenuModal::ConfirmResetAll {
                    selected: ConfirmChoice::Confirm,
                },
            );
        }
        UiScreenshotScenario::FinishWonRoaming => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_won_game_state(false);
            }
            if let Some(mut finish) = world.get_resource_mut::<FinishFlowState>() {
                finish.phase = FinishPhase::WonRoaming;
                finish.flash_alpha = 0;
            }
        }
        UiScreenshotScenario::FinishWinFlash => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_won_game_state(false);
            }
            if let Some(mut finish) = world.get_resource_mut::<FinishFlowState>() {
                finish.phase = FinishPhase::WonRoaming;
                finish.flash_alpha = 200;
            }
        }
        UiScreenshotScenario::FinishEndOverlay => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_won_game_state(false);
            }
            if let Some(mut finish) = world.get_resource_mut::<FinishFlowState>() {
                finish.phase = FinishPhase::EndOverlayOpen;
                finish.flash_alpha = 0;
            }
        }
        UiScreenshotScenario::FinishEndOverlayCheated => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_won_game_state(true);
            }
            if let Some(mut finish) = world.get_resource_mut::<FinishFlowState>() {
                finish.phase = FinishPhase::EndOverlayOpen;
                finish.flash_alpha = 0;
            }
        }
        #[cfg(feature = "ai")]
        UiScreenshotScenario::AiDebugPanels => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_mid_run_game_state();
            }
        }
        #[cfg(feature = "ai")]
        UiScreenshotScenario::AiWaitingIndicator => {
            if let Some(mut game) = world.get_resource_mut::<GameState>() {
                *game = sample_mid_run_game_state();
            }
            if let Some(mut cfg) = world.get_resource_mut::<crate::ai::AiConfig>() {
                cfg.waiting_for_action = true;
            }
        }
    }
}

fn open_pause_menu(world: &mut World, focus: FocusTarget, modal: PauseMenuModal) {
    if let Some(mut pause) = world.get_resource_mut::<PauseMenuState>() {
        pause.open = true;
        pause.focus = focus;
        pause.modal = modal;
        pause.resume_on_close = true;
        pause.capture_armed = matches!(modal, PauseMenuModal::CaptureKey(_));
    }
}

fn sample_default_game_state() -> GameState {
    GameState {
        score: 0,
        nb_orbs: 100,
        player_time: 0.0,
        world_time: 0.0,
        player_speed: 0.0,
        speed_of_light: 200.0,
        speed_multiplier: 0.95,
        max_player_speed: 32.0,
        game_win: false,
        orb_splits: Vec::new(),
        used_cheat_99_orbs: false,
        ..GameState::default()
    }
}

fn sample_mid_run_game_state() -> GameState {
    GameState {
        score: 37,
        nb_orbs: 100,
        player_time: 142.37,
        world_time: 198.12,
        player_speed: 22.5,
        speed_of_light: 180.0,
        speed_multiplier: 0.88,
        max_player_speed: 32.0,
        game_win: false,
        orb_splits: sample_orb_splits(6),
        used_cheat_99_orbs: false,
        ..GameState::default()
    }
}

fn sample_won_game_state(cheated: bool) -> GameState {
    GameState {
        score: 100,
        nb_orbs: 100,
        player_time: 312.045,
        world_time: 540.881,
        player_speed: 0.0,
        speed_of_light: 40.0,
        speed_multiplier: 1.0,
        max_player_speed: 32.0,
        game_win: true,
        orb_splits: sample_orb_splits(12),
        used_cheat_99_orbs: cheated,
        ..GameState::default()
    }
}

fn sample_orb_splits(count: u32) -> Vec<OrbSplit> {
    (0..count)
        .map(|i| {
            let player_time = 12.0 + i as f32 * 8.5;
            OrbSplit {
                sequence_index: i + 1,
                orb_id: OrbId((i % 100) as u8),
                player_time,
                world_time: player_time * 1.4,
                player_split_delta: 8.5,
                world_split_delta: 11.9,
            }
        })
        .collect()
}

#[cfg(feature = "ai")]
fn sample_ai_observations() -> crate::ai::AiObservations {
    use crate::ai::AiObservations;

    let mut obs = AiObservations::default();
    obs.player_position = Vec3::new(12.5, 1.0, -4.2);
    for i in 0..37 {
        obs.orb_checklist[i] = 0.0;
    }
    for i in 37..100 {
        obs.orb_checklist[i] = 1.0;
    }
    for i in 0..16 {
        obs.wall_rays[i] = (i as f32) / 15.0;
    }
    obs.orb_targets[0] = (Vec3::new(0.2, 0.0, -0.98), 18.5, 42.0);
    obs.ray_origin_y = -1.0;
    obs
}

#[cfg(feature = "ai")]
fn sample_ai_rewards() -> crate::ai::AiRewardSignal {
    use crate::ai::AiRewardSignal;

    AiRewardSignal {
        step_reward: 0.142,
        time_penalty: -0.010,
        orb_reward: 0.100,
        momentum_bonus: 0.030,
        approach_reward: 0.022,
        action_smoothness_penalty: -0.005,
        smooth_camera_bonus: 0.005,
        yaw_ema: 0.08,
        current_action_yaw: 0.12,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn scenarios_cover_unique_file_stems() {
        let stems: Vec<_> = all_ui_screenshot_scenarios()
            .into_iter()
            .map(UiScreenshotScenario::file_stem)
            .collect();
        let unique: HashSet<_> = stems.iter().copied().collect();
        assert_eq!(stems.len(), unique.len(), "duplicate screenshot file stems");
        assert!(!stems.is_empty());

        let files = expected_screenshot_filenames();
        assert_eq!(files.len(), stems.len());
        assert!(files.iter().all(|f| f.ends_with(".png")));
    }

    #[test]
    fn scenarios_include_core_ui_surfaces() {
        let set: HashSet<_> = all_ui_screenshot_scenarios().into_iter().collect();
        assert!(set.contains(&UiScreenshotScenario::InGameHudDefault));
        assert!(set.contains(&UiScreenshotScenario::PauseMenuSettings));
        assert!(set.contains(&UiScreenshotScenario::PauseMenuCaptureKey));
        assert!(set.contains(&UiScreenshotScenario::FinishEndOverlay));
        assert!(set.contains(&UiScreenshotScenario::ToastWarning));
    }

    #[test]
    fn apply_scenario_opens_pause_menu() {
        let mut app = App::new();
        app.init_resource::<PauseMenuState>()
            .init_resource::<GameState>()
            .init_resource::<GraphicsSettings>()
            .init_resource::<AudioSettings>()
            .init_resource::<MovementSettings>()
            .init_resource::<KeyMapping>()
            .init_resource::<BorderFlash>()
            .init_resource::<FinishFlowState>();

        apply_scenario(app.world_mut(), UiScreenshotScenario::PauseMenuCaptureKey);

        let pause = app.world().resource::<PauseMenuState>();
        assert!(pause.open);
        assert!(matches!(
            pause.modal,
            PauseMenuModal::CaptureKey(KeyAction::Forward)
        ));
    }

    #[test]
    fn apply_scenario_sets_finish_end_overlay() {
        let mut app = App::new();
        app.init_resource::<PauseMenuState>()
            .init_resource::<GameState>()
            .init_resource::<GraphicsSettings>()
            .init_resource::<AudioSettings>()
            .init_resource::<MovementSettings>()
            .init_resource::<KeyMapping>()
            .init_resource::<BorderFlash>()
            .init_resource::<FinishFlowState>();

        apply_scenario(
            app.world_mut(),
            UiScreenshotScenario::FinishEndOverlayCheated,
        );

        let finish = app.world().resource::<FinishFlowState>();
        let game = app.world().resource::<GameState>();
        assert_eq!(finish.phase, FinishPhase::EndOverlayOpen);
        assert!(game.game_win);
        assert!(game.used_cheat_99_orbs);
        assert!(!game.orb_splits.is_empty());
    }
}
