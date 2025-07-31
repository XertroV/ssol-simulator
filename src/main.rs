use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

mod scene_loader;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        // Add the Rapier physics plugin
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        // Add the debug renderer to see our colliders
        .add_plugins(RapierDebugRenderPlugin::default())
        // Our custom startup system that will load and spawn everything
        .add_systems(Startup, scene_loader::setup_scene)
        // A simple camera so we can see the scene
        .add_systems(Startup, setup_camera)
        .run();
}

// Spawns a basic 3D camera.
fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-0.0, 100.0, 0.0)
            .looking_at((10., 5., 30.).into(), Vec3::Y),
        GlobalTransform::default(),
    ));
}
