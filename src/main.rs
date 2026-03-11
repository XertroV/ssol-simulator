use std::time::Duration;

use bevy::{
    app::ScheduleRunnerPlugin,
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, ExitCondition, PresentMode, PrimaryWindow, WindowFocused},
    winit::{WinitPlugin, WinitSettings, UpdateMode},
};
use bevy_rapier3d::prelude::*;
use clap::Parser;
// use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use iyes_perf_ui::prelude::*;

use crate::{
    audio::GameAudioPlugin,
    camera_switcher::CameraSwitcherPlugin,
    key_mapping::KeyMappingPlugin,
    player::set_grab_mode,
    relativity::rel_material::RelativisticMaterialPlugin,
    scene::SceneCalcDataPlugin,
    ui::InGameUiPlugin,
};
#[cfg(feature = "ai")]
use crate::{ai::gizmos::AiGizmosPlugin, ai::observations::AiObservationPlugin};
// use crate::relativity::compute::RelativityComputePlugin;

mod scene_loader;
// mod fly_camera_simple;

#[cfg(feature = "ai")]
mod ai;
mod ai_support;
mod audio;
mod camera_switcher;
mod curriculum;
mod game_state;
mod key_mapping;
mod orb_curriculum;
mod physics_interpolation;
mod player;
mod relativity;
mod scene;
mod ui;
mod uv_fixer;

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

    /// Instance name for logging (helps identify logs from multiple instances)
    #[arg(long)]
    instance_name: Option<String>,

    /// Set the curriculum max_orbs on startup (number of orbs to spawn)
    #[arg(long)]
    num_orbs: Option<u32>,
}

/// Resource containing simulation configuration
#[derive(Resource, Debug, Clone)]
pub struct SimConfig {
    pub headless: bool,
    pub speed_multiplier: f32,
    pub show_gizmos: bool,
    pub target_fps: f64,
    #[cfg(feature = "ai")]
    pub ai_mode: bool,
    #[cfg(feature = "ai")]
    pub ai_test: bool,
    /// ZMQ port for Python bridge (None = disabled)
    #[cfg(feature = "ai")]
    pub zmq_port: Option<u16>,
    /// Instance name for logging
    pub instance_name: Option<String>,
    /// Initial curriculum max_orbs setting
    pub num_orbs: Option<u32>,
}

fn main() {
    let args = Args::parse();
    #[cfg(feature = "ai")]
    let ai_mode = args.ai_mode || args.ai_test || args.zmq_port.is_some();

    let config = SimConfig {
        headless: args.headless,
        speed_multiplier: args.speed,
        show_gizmos: false,
        target_fps: args.fps,
        #[cfg(feature = "ai")]
        ai_mode,
        #[cfg(feature = "ai")]
        ai_test: args.ai_test,
        #[cfg(feature = "ai")]
        zmq_port: args.zmq_port,
        instance_name: args.instance_name.clone(),
        num_orbs: args.num_orbs,
    };

    let mut app = App::new();

    // Configure 100Hz fixed timestep for deterministic physics
    app.insert_resource(Time::<Fixed>::from_hz(100.0));

    if config.headless {
        // Headless mode: no window, controlled loop
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1.0 / config.target_fps),
        ));
    } else {
        // Graphical mode: normal window
        app.insert_resource(ClearColor(COLOR_BLACK))
            .add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Open SSOL".into(),
                    present_mode: PresentMode::AutoNoVsync, // No VSync for consistent speed
                    focused: true,
                    desired_maximum_frame_latency: Some(1.try_into().unwrap()),
                    mode: bevy::window::WindowMode::Windowed,
                    ..default()
                }),
                primary_cursor_options: Some(CursorOptions {
                    grab_mode: CursorGrabMode::Confined,
                    visible: true,
                    ..default()
                }),
                ..default()
            }));
    }

    // Store config as resource for runtime access
    app.insert_resource(config.clone());

    // Configure continuous updates to prevent FPS drops when alt-tabbing
    app.insert_resource(WinitSettings {
        focused_mode: UpdateMode::Continuous,
        unfocused_mode: UpdateMode::Continuous,
    });

    app
        // Physics plugin in fixed schedule for determinism
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule());
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
        .add_plugins(SceneCalcDataPlugin);

    // Only add audio and UI plugins in graphical mode
    if !config.headless {
        app.add_plugins(GameAudioPlugin).add_plugins(InGameUiPlugin);
        #[cfg(feature = "ai")]
        app.add_plugins(ui::AiDebugUiPlugin);
    }

    // Always init CurriculumConfig (used by scene_loader even in non-AI mode)
    app.init_resource::<curriculum::CurriculumConfig>();

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
        .add_systems(Update, (sync_grab_with_focus,).run_if(not(is_headless)))
        .run();
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
    virtual_time.set_relative_speed(config.speed_multiplier);

    // Set max_delta very high to prevent skipping physics ticks at high speeds
    // Default is 250ms which limits fixed updates. We want unlimited catch-up.
    virtual_time.set_max_delta(Duration::MAX);

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
            shadows_enabled: false,
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
) {
    for event in focus_events.read() {
        let mut cursor_options = cursor_options
            .single_mut()
            .expect("Expected a single primary window");
        set_grab_mode(
            &mut cursor_options,
            match event.focused {
                true => CursorGrabMode::Locked,
                false => CursorGrabMode::None,
            },
        );
    }
}
