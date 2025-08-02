use bevy::{
    core_pipeline::{experimental::taa::TemporalAntiAliasPlugin, smaa::{Smaa, SmaaPlugin}},
    pbr::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PresentMode},
};
use bevy_rapier3d::prelude::*;
// use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use iyes_perf_ui::prelude::*;

use crate::{camera_switcher::CameraSwitcherPlugin, key_mapping::KeyMappingPlugin};

mod scene_loader;
// mod fly_camera_simple;
mod camera_switcher;
mod game_state;
mod key_mapping;
mod player;
mod relativity;
mod uv_fixer;

fn main() {
    let mut app = App::new();
    app
        .insert_resource(ClearColor(Color::srgba(0.16, 0.16, 0.19, 1.0)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Open SSOL".into(), // You can set other window properties here
                // present_mode: PresentMode::Immediate, // This turns VSync off
                focused: true,
                desired_maximum_frame_latency: Some(1.try_into().unwrap()),
                mode: bevy::window::WindowMode::Windowed,
                cursor_options: CursorOptions {
                    grab_mode: CursorGrabMode::Locked,
                    visible: false,
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
        .add_plugins(uv_fixer::UvFixerPlugin)
        .add_plugins(game_state::GameStatePlugin)
        .add_plugins(KeyMappingPlugin)
        .add_plugins(CameraSwitcherPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_systems(Startup, scene_loader::setup_scene)
        .add_systems(Startup, setup_light)
        .insert_resource(DirectionalLightShadowMap { size: 4096 })
        // .add_systems(Startup, player::spawn_player.after(scene_loader::setup_scene))
        // .add_systems(Update, player::move_player)
        // .add_observer(scene_loader::change_material)
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
