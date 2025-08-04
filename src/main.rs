use bevy::{
    pbr::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PresentMode, PrimaryWindow, WindowFocused}
};
use bevy_rapier3d::prelude::*;
// use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use iyes_perf_ui::prelude::*;

use crate::{audio::GameAudioPlugin, camera_switcher::CameraSwitcherPlugin, key_mapping::KeyMappingPlugin, player::set_grab_mode, relativity::rel_material, scene::SceneCalcDataPlugin};
// use crate::relativity::compute::RelativityComputePlugin;

mod scene_loader;
// mod fly_camera_simple;

mod camera_switcher;
mod game_state;
mod key_mapping;
mod audio;
mod player;
mod relativity;
mod uv_fixer;
mod scene;

fn main() {
    let mut app = App::new();

    app
        .insert_resource(ClearColor(Color::srgba(0.16, 0.16, 0.19, 1.0)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Open SSOL".into(),
                // present_mode: PresentMode::Immediate, // This turns VSync off
                present_mode: PresentMode::Mailbox, // "Fast VSync" (many FPS, but no tearing)
                focused: false,
                desired_maximum_frame_latency: Some(1.try_into().unwrap()), // How many frames to buffer (default 2)
                mode: bevy::window::WindowMode::Windowed,
                cursor_options: CursorOptions {
                    grab_mode: CursorGrabMode::None,
                    visible: true,
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        // .insert_resource(UiDebugOptions {
        //     enabled: true,
        //     ..default()
        // })
        // physics plugin
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        // debug for physics bodies
        // .add_plugins(RapierDebugRenderPlugin::default())
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin)
        .add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin)
        .add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
        .add_plugins(PerfUiPlugin);
        // .add_plugins(AabbGizmoPlugin)

    // app
    //     // TAA?
    //     // .add_plugins(TemporalAntiAliasPlugin)
    //     .add_plugins(SmaaPlugin);

    app
        // .add_plugins(RelativityComputePlugin)
        .add_plugins(uv_fixer::UvFixerPlugin)
        .add_plugins(game_state::GameStatePlugin)
        .add_plugins(KeyMappingPlugin)
        .add_plugins(CameraSwitcherPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(GameAudioPlugin)
        .add_plugins(SceneCalcDataPlugin)
        .add_plugins(rel_material::RelativisticMaterialPlugin)
        .add_systems(Startup, scene_loader::setup_scene)
        .add_systems(Startup, setup_light)
        .insert_resource(DirectionalLightShadowMap { size: 4096 })
        // .add_systems(Startup, player::spawn_player.after(scene_loader::setup_scene))
        // .add_systems(Update, player::move_player)
        // .add_observer(scene_loader::change_material)
        .add_systems(Update, (
            sync_grab_with_focus,
        ))
        .run();
}

/*
    Note: had shadow glitches when num_cascades > 1 and shadows_enabled = true.
*/

fn setup_light(mut commands: Commands) {
    let config: CascadeShadowConfig = CascadeShadowConfigBuilder {
        maximum_distance: 800.0,
        // num_cascades: 1,
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
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut focus_events: EventReader<WindowFocused>,
) {
    for event in focus_events.read() {
        let window = window.single_mut().expect("Expected a single primary window");
        set_grab_mode(window, if event.focused {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        });
    }
}
