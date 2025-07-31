use bevy::{gizmos::aabb::AabbGizmoPlugin, pbr::{CascadeShadowConfig, CascadeShadowConfigBuilder}, prelude::*};
use bevy_rapier3d::prelude::*;
// use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use iyes_perf_ui::prelude::*;

mod scene_loader;
mod camera_controller;
mod uv_fixer;


fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(UiDebugOptions {
            enabled: true,
            ..default()
        })
        // physics plugin
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        // debug for physics bodies
        .add_plugins(RapierDebugRenderPlugin::default())

        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin)
        .add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin)
        .add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
        .add_plugins(PerfUiPlugin)
        // .add_plugins(AabbGizmoPlugin)

        .add_plugins(camera_controller::CameraControllerPlugin)
        .add_plugins(uv_fixer::UvFixerPlugin)

        .add_systems(Startup, scene_loader::setup_scene)
        .add_systems(Startup, setup_light)
        // .add_observer(scene_loader::change_material)

        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(20.0, 60.0, 20.0)
            .looking_at((-150., 30., 0.).into(), Vec3::Y),
        GlobalTransform::default(),
    ));
}



fn setup_light(mut commands: Commands) {
    let config: CascadeShadowConfig = CascadeShadowConfigBuilder {
        maximum_distance: 800.0,
        ..default()
    }.into();

    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            shadow_depth_bias: 0.01,
            shadow_normal_bias: 1.0,
            ..default()
        },
        config,
        Transform {
            // A light source rotated to cast light down and from an angle.
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4),
            ..default()
        },
    ));
}
