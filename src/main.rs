use std::time::Duration;
use std::sync::mpsc;

use bevy::{
    app::ScheduleRunnerPlugin,
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, ExitCondition, PrimaryWindow, WindowFocused},
    winit::{WinitPlugin, WinitSettings, UpdateMode},
};
use bevy_rapier3d::prelude::*;
use clap::Parser;
// use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use iyes_perf_ui::prelude::*;

use crate::{
    audio::GameAudioPlugin,
    camera_switcher::CameraSwitcherPlugin,
    config::{AppConfigStore, ConfigPlugin},
    key_mapping::KeyMappingPlugin,
    player::set_grab_mode,
    relativity::rel_material::RelativisticMaterialPlugin,
    scene::SceneCalcDataPlugin,
    ui::{FinishScreenUiPlugin, InGameUiPlugin, PauseMenuUiPlugin, ToastUiPlugin},
};
#[cfg(feature = "ai")]
use crate::{ai::gizmos::AiGizmosPlugin, ai::observations::AiObservationPlugin};
// use crate::relativity::compute::RelativityComputePlugin;

mod scene_loader;
// mod fly_camera_simple;

#[cfg(feature = "ai")]
mod ai;
mod ai_support;
mod asset_paths;
mod audio;
mod camera_switcher;
mod config;
mod curriculum;
mod game_state;
mod ghost;
mod key_mapping;
mod minimap;
mod orb_curriculum;
mod physics_interpolation;
mod player;
mod relativity;
mod scene;
mod train;
mod ui;
mod uv_fixer;
mod villagers;

pub const CLEAR_COLOR: Color = Color::srgba(0.16, 0.16, 0.19, 1.0);
pub const COLOR_BLACK: Color = Color::srgba(0.0, 0.0, 0.0, 1.0);

/// Simulation configuration parsed from CLI arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run in headless mode (no window/rendering)
    #[arg(long, default_value_t = false)]
    headless: bool,

    /// Simulation speed multiplier (1.0 = real-time, higher = faster)
    /// Use a very large value (e.g., 999999) to run as fast as possible
    #[arg(long, default_value_t = 1.0)]
    speed: f32,

    /// Target FPS for rendering (only applies in graphical mode)
    #[arg(long, default_value_t = 60.0)]
    fps: f64,

    #[cfg(feature = "ai")]
    /// Enable AI control mode (disables keyboard/mouse input, enables AI action input)
    #[arg(long, default_value_t = false)]
    ai_mode: bool,

    #[cfg(feature = "ai")]
    /// Run AI test mode (random actions, logs observations/rewards)
    #[arg(long, default_value_t = false)]
    ai_test: bool,

    #[cfg(feature = "ai")]
    /// ZMQ port for Python bridge communication (enables bridge when set)
    #[arg(long)]
    zmq_port: Option<u16>,

    /// Disable audio (workaround for ALSA/PipeWire hangs)
    #[arg(long, default_value_t = false)]
    no_audio: bool,

    /// Instance name for logging (helps identify logs from multiple instances)
    #[arg(long)]
    instance_name: Option<String>,

    /// Set the curriculum max_orbs on startup (number of orbs to spawn)
    #[arg(long)]
    num_orbs: Option<u32>,

    /// Verify a ghost recording by replaying its inputs and checking positions
    #[arg(long)]
    verify_ghost: Option<String>,

    /// Run the ghost determinism test (record a bot run, then verify it)
    #[arg(long, default_value_t = false)]
    ghost_test: bool,

    /// Capture screenshots of every in-game UI screen/state into DIR, then exit.
    /// Uses a minimal app (UI plugins only) so captures are fast and deterministic.
    #[arg(long, value_name = "DIR")]
    ui_screenshots: Option<std::path::PathBuf>,

    /// Phase 0 training harness: AI control + privileged obs + WR high-level targets.
    /// Pair with `--headless --no-audio` for smoke runs. Does not require `--features ai`.
    #[arg(long, default_value_t = false)]
    scripted_baseline: bool,

    /// Policy decision rate (Hz) for the training harness. Physics stays at 100 Hz.
    #[arg(long, default_value_t = 10.0)]
    act_hz: f32,

    /// Max episode length in sim seconds for the training harness.
    #[arg(long, default_value_t = 120.0)]
    max_episode_secs: f32,

    /// Path to WR route JSON (default: assets/wr_route_level_zero.json).
    #[arg(long)]
    wr_route: Option<std::path::PathBuf>,

    /// High-level route family: wr|greedy|wr_noisy|random_nn|reverse_wr|mix.
    /// Default `mix` samples WR/greedy/noisy/NN/reverse for train generalization.
    /// Use `wr` for WR-only eval.
    #[arg(long, default_value = "mix")]
    route_mode: String,

    /// RNG seed for train route sampling (and multi-seed eval harness).
    #[arg(long, default_value_t = 0)]
    seed: u64,

    /// Episodes per process for the train harness (soft respawn between episodes).
    #[arg(long, default_value_t = 1)]
    num_episodes: u32,

    /// Append MDP transitions as JSONL (schema v2 obs + action + reward).
    #[arg(long, value_name = "PATH")]
    dump_transitions: Option<std::path::PathBuf>,

    /// Live RL step protocol: TRAIN_STEP_JSON on stdout, action JSON on stdin each act.
    /// Disables pure scripted control (actions come from the external agent).
    #[arg(long, default_value_t = false)]
    train_stdio: bool,
}

/// Resource containing simulation configuration
#[derive(Resource, Debug, Clone)]
pub struct SimConfig {
    pub headless: bool,
    pub speed_multiplier: f32,
    pub target_fps: f64,
    #[cfg(feature = "ai")]
    pub ai_mode: bool,
    #[cfg(feature = "ai")]
    pub ai_test: bool,
    /// ZMQ port for Python bridge (None = disabled)
    #[cfg(feature = "ai")]
    pub zmq_port: Option<u16>,
    /// Disable audio entirely
    pub no_audio: bool,
    /// Instance name for logging
    pub instance_name: Option<String>,
    /// Initial curriculum max_orbs setting
    pub num_orbs: Option<u32>,
    /// Path to ghost file for verification replay
    pub verify_ghost: Option<String>,
    /// Run the ghost determinism test
    pub ghost_test: bool,
    /// Phase 0 scripted baseline / train harness
    pub scripted_baseline: bool,
    pub act_hz: f32,
    pub max_episode_secs: f32,
    pub wr_route: Option<std::path::PathBuf>,
    /// Parsed route mode string (`mix` default); see `--route-mode`.
    pub route_mode: String,
    /// RNG seed for train route sampling.
    pub seed: u64,
    pub num_episodes: u32,
    pub dump_transitions: Option<std::path::PathBuf>,
    pub train_stdio: bool,
}

/// Probe for audio output devices with a timeout.
/// Returns `true` if an audio device was found, `false` otherwise.
fn probe_audio_available() -> bool {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        use cpal::traits::{HostTrait, DeviceTrait};
        let host = cpal::default_host();
        let result = host.default_output_device().and_then(|d| {
            d.default_output_config().ok()
        });
        let _ = tx.send(result.is_some());
    });
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(available) => available,
        Err(_) => {
            eprintln!("Warning: Audio device probe timed out after 2s. Disabling audio.");
            eprintln!("         Use --no-audio to skip this check.");
            false
        }
    }
}

fn main() {
    let args = Args::parse();

    if let Some(output_dir) = args.ui_screenshots {
        ui::run_ui_screenshot_suite(ui::UiScreenshotConfig {
            output_dir,
            ..Default::default()
        });
        return;
    }

    #[cfg(feature = "ai")]
    let ai_mode = args.ai_mode || args.ai_test || args.zmq_port.is_some();

    let config_store = AppConfigStore::load();
    let app_config = config_store.config.clone();
    let graphics_settings = app_config.graphics.clone();

    let no_audio = if args.no_audio {
        true
    } else {
        let available = probe_audio_available();
        if !available {
            eprintln!("Warning: No audio output device found. Audio disabled.");
        }
        !available
    };

    let config = SimConfig {
        headless: args.headless,
        speed_multiplier: args.speed,
        target_fps: args.fps,
        #[cfg(feature = "ai")]
        ai_mode,
        #[cfg(feature = "ai")]
        ai_test: args.ai_test,
        #[cfg(feature = "ai")]
        zmq_port: args.zmq_port,
        no_audio,
        instance_name: args.instance_name.clone(),
        num_orbs: args.num_orbs,
        verify_ghost: args.verify_ghost.clone(),
        ghost_test: args.ghost_test,
        scripted_baseline: args.scripted_baseline,
        act_hz: args.act_hz,
        max_episode_secs: args.max_episode_secs,
        wr_route: args.wr_route.clone(),
        route_mode: args.route_mode.clone(),
        seed: args.seed,
        num_episodes: args.num_episodes,
        dump_transitions: args.dump_transitions.clone(),
        train_stdio: args.train_stdio,
    };

    let mut app = App::new();

    // Configure 100Hz fixed timestep for deterministic physics
    app.insert_resource(Time::<Fixed>::from_hz(100.0));

    if config.headless {
        // Headless mode: no window, controlled loop
        let mut plugins = DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>();
        if config.no_audio {
            plugins = plugins.disable::<bevy::audio::AudioPlugin>();
        }
        // Headless verification: run as fast as possible (no sleep between iterations)
        let loop_wait = if config.verify_ghost.is_some() {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(1.0 / config.target_fps)
        };
        app.add_plugins(plugins)
            .add_plugins(ScheduleRunnerPlugin::run_loop(loop_wait));
    } else {
        // Graphical mode: normal window
        let mut plugins = DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Open SSOL".into(),
                present_mode: graphics_settings.present_mode(),
                focused: true,
                desired_maximum_frame_latency: Some(1.try_into().unwrap()),
                mode: graphics_settings.window_mode(),
                ..default()
            }),
            primary_cursor_options: Some(CursorOptions {
                grab_mode: CursorGrabMode::Confined,
                visible: true,
                ..default()
            }),
            ..default()
        });
        if config.no_audio {
            plugins = plugins.disable::<bevy::audio::AudioPlugin>();
        }
        app.insert_resource(ClearColor(COLOR_BLACK))
            .add_plugins(plugins);
    }

    // Store config as resource for runtime access
    app.insert_resource(config.clone());
    app.insert_resource(config_store);
    app.insert_resource(app_config.key_mapping.clone());
    app.insert_resource(app_config.movement.clone());
    app.insert_resource(app_config.audio.clone());
    app.insert_resource(graphics_settings.clone());

    // Configure continuous updates to prevent FPS drops when alt-tabbing
    app.insert_resource(WinitSettings {
        focused_mode: UpdateMode::Continuous,
        unfocused_mode: UpdateMode::Continuous,
    });

    app
        // Physics plugin in fixed schedule for determinism
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule())
        .add_plugins(RapierDebugRenderPlugin::default().disabled())
        .add_plugins(ConfigPlugin);
        // debug for physics bodies
        // .add_plugins(RapierDebugRenderPlugin::default())

    // Only add diagnostic/perf plugins in graphical mode
    if !config.headless {
        app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
            .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin::default())
            .add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin)
            .add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
            .add_plugins(PerfUiPlugin);
    }
    // .add_plugins(AabbGizmoPlugin)

    // app
    //     // TAA?
    //     // .add_plugins(TemporalAntiAliasPlugin)
    //     .add_plugins(SmaaPlugin);

    app.add_plugins(uv_fixer::UvFixerPlugin)
        .add_plugins(game_state::GameStatePlugin)
        .add_plugins(RelativisticMaterialPlugin)
        .add_plugins(KeyMappingPlugin)
        .add_plugins(CameraSwitcherPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(physics_interpolation::PhysicsInterpolationPlugin)
        .add_plugins(SceneCalcDataPlugin)
        .add_plugins(villagers::VillagerPlugin)
        .add_plugins(ghost::GhostPlugin);

    // Only add audio and UI plugins in graphical mode
    if !config.headless {
        if !config.no_audio {
            app.add_plugins(GameAudioPlugin);
        }
        app.add_plugins(InGameUiPlugin)
            .add_plugins(FinishScreenUiPlugin)
            .add_plugins(ToastUiPlugin)
            .add_plugins(PauseMenuUiPlugin)
            .add_plugins(minimap::MinimapPlugin);
        #[cfg(feature = "ai")]
        app.add_plugins(ui::AiDebugUiPlugin);
    }

    // Always init CurriculumConfig (used by scene_loader even in non-AI mode)
    app.init_resource::<curriculum::CurriculumConfig>();

    // Train harness (scripted baseline and/or live stdio RL; no `--features ai`)
    if config.scripted_baseline || config.train_stdio {
        let route_mode = match config.route_mode.parse::<train::RouteMode>() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        };
        let mut train_cfg = train::TrainConfig {
            enabled: true,
            // stdio mode drives actions externally; scripted is fallback if stdin fails
            scripted: config.scripted_baseline && !config.train_stdio,
            act_hz: config.act_hz,
            max_episode_secs: config.max_episode_secs,
            route_mode,
            seed: config.seed,
            exit_on_done: true,
            metrics_json: true,
            num_episodes: config.num_episodes.max(1),
            dump_transitions: config.dump_transitions.clone(),
            train_stdio: config.train_stdio,
            ..Default::default()
        };
        if let Some(ref path) = config.wr_route {
            train_cfg.wr_route_path = path.clone();
        }
        info!(
            "Train harness enabled (scripted={} stdio={} act_hz={} route_mode={} seed={} episodes={})",
            train_cfg.scripted,
            train_cfg.train_stdio,
            train_cfg.act_hz,
            train_cfg.route_mode,
            train_cfg.seed,
            train_cfg.num_episodes
        );
        app.insert_resource(train_cfg)
            .init_resource::<ai_support::AiConfig>()
            .init_resource::<ai_support::AiActionInput>()
            .add_plugins(train::TrainPlugin);
    }

    #[cfg(feature = "ai")]
    // always add AI gizmos (disabled by default)
    app.add_plugins(AiGizmosPlugin)
        .add_plugins(AiObservationPlugin);

    #[cfg(feature = "ai")]
    // Add AI plugin if ai_mode or ai_test is enabled
    if config.ai_mode || config.ai_test {
        app.add_plugins(ai::AiPlugin);
        if !config.headless {
            app.add_plugins(bevy_framepace::FramepacePlugin);
            app.add_systems(Startup, set_framepace_for_training);
        }

        // Add testing plugin for random action testing
        if config.ai_test {
            app.add_plugins(ai::AiTestingPlugin);
            info!("AI Testing mode enabled - random actions will be applied");
        }
    } else {
        // we need to add observation updates because it's configured in AiPlugin
        app.add_systems(
                FixedUpdate,
                ai::observations::update_observations,
        );
    }

    app
        .add_systems(Startup, apply_initial_curriculum.before(scene_loader::setup_scene))
        .add_systems(Startup, scene_loader::setup_scene)
        .add_systems(Startup, setup_light)
        .add_systems(Startup, configure_simulation_speed)
        // .insert_resource(DirectionalLightShadowMap { size: 4096 })
        // .add_systems(Startup, player::spawn_player.after(scene_loader::setup_scene))
        // .add_systems(Update, player::move_player)
        // .add_observer(scene_loader::change_material)
        .add_systems(Update, (sync_grab_with_focus,).run_if(not(is_headless)));

    if let Some(ref ghost_path) = config.verify_ghost {
        ghost::setup_verify_ghost(&mut app, ghost_path, config.headless);
    }

    if config.ghost_test {
        ghost::setup_ghost_test(&mut app);
    }

    app.run();
}

#[cfg(feature = "ai")]
fn set_framepace_for_training(
    mut _commands: Commands,
    mut settings: ResMut<bevy_framepace::FramepaceSettings>,
) {
    settings.limiter = bevy_framepace::Limiter::from_framerate(100.0);
}

/// Returns true if running in headless mode
fn is_headless(config: Res<SimConfig>) -> bool {
    config.headless
}

/// Configure simulation speed based on CLI arguments
fn configure_simulation_speed(
    config: Res<SimConfig>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    // Set the relative speed - this affects how fast virtual time passes
    // A speed of 10.0 means 10 simulated seconds per real second
    // For very high speeds (like 999999), physics will run many ticks per frame
    let speed = if config.headless && config.verify_ghost.is_some() {
        // Headless verification: high speed, limited by max_delta cap below
        // to prevent spiral-of-death (each frame's processing time inflated
        // into ever-larger virtual deltas → ever-more ticks → slower frames).
        100_000.0
    } else {
        config.speed_multiplier
    };
    virtual_time.set_relative_speed(speed);

    if config.headless && config.verify_ghost.is_some() {
        // Cap virtual time delta to 200ms per frame (= max 20 physics ticks/frame).
        // Combined with Duration::ZERO loop sleep, this yields ~3000+ ticks/s
        // without the spiral-of-death that Duration::MAX would cause.
        virtual_time.set_max_delta(Duration::from_millis(200));
    } else {
        // Normal mode: unlimited catch-up to prevent skipping physics ticks at high speeds.
        virtual_time.set_max_delta(Duration::MAX);
    }

    let instance_str = config.instance_name.as_deref().unwrap_or("default");
    #[cfg(feature = "ai")]
    info!(
        "[{}] Simulation configured: headless={}, ai_mode={}, speed={}x, target_fps={}",
        instance_str, config.headless, config.ai_mode, config.speed_multiplier, config.target_fps
    );
    #[cfg(not(feature = "ai"))]
    info!(
        "[{}] Simulation configured: headless={}, speed={}x, target_fps={}",
        instance_str, config.headless, config.speed_multiplier, config.target_fps
    );
}

/// Apply initial curriculum settings from CLI arguments
fn apply_initial_curriculum(
    config: Res<SimConfig>,
    mut curriculum: ResMut<curriculum::CurriculumConfig>,
) {
    if let Some(num_orbs) = config.num_orbs {
        curriculum.max_orbs = Some(num_orbs);
        let instance_str = config.instance_name.as_deref().unwrap_or("default");
        info!("[{}] Curriculum set from CLI: max_orbs = {}", instance_str, num_orbs);
    }
}

/*
    Note: had shadow glitches when num_cascades > 1 and shadows_enabled = true.
*/

fn setup_light(mut commands: Commands) {
    let config: CascadeShadowConfig = CascadeShadowConfigBuilder {
        maximum_distance: 800.0,
        // num_cascades: 4,
        // minimum_distance: 0.01,
        // first_cascade_far_bound: 10.0,
        // overlap_proportion: 0.5,
        ..default()
    }
    .into();

    commands.spawn((
        DirectionalLight {
            illuminance: 7500.0,
            shadow_maps_enabled: false,
            shadow_depth_bias: 0.1,
            shadow_normal_bias: 1.9,
            ..default()
        },
        config,
        Transform {
            // A light source rotated to cast light down and from an angle.
            translation: Vec3::new(0.0, 100.0, 0.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4 * 2.5),
            ..default()
        },
    ));
}

/// Sets the cursor grab mode based on the current window state.
fn sync_grab_with_focus(
    mut cursor_options: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut focus_events: MessageReader<WindowFocused>,
    pause_menu: Option<Res<ui::PauseMenuState>>,
) {
    let pause_menu_open = ui::is_pause_menu_open(pause_menu.as_deref());
    for event in focus_events.read() {
        let Ok(mut cursor_options) = cursor_options.single_mut() else {
            return;
        };
        set_grab_mode(
            &mut cursor_options,
            match (event.focused, pause_menu_open) {
                (true, true) => CursorGrabMode::None,
                (true, false) => CursorGrabMode::Locked,
                _ => CursorGrabMode::None,
            },
        );
    }
}
