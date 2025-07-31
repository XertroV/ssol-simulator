use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

//==============================================================================
// PLUGIN
//==============================================================================

pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementSettings>()
            .add_systems(Startup, setup_camera)
            .add_systems(Update, (update_camera_look, update_camera_move, cursor_grab));
    }
}

//==============================================================================
// RESOURCES
//==============================================================================

#[derive(Resource)]
pub struct MovementSettings {
    pub sensitivity: f32,
    pub speed: f32,
}

impl Default for MovementSettings {
    fn default() -> Self {
        Self {
            sensitivity: 0.00012,
            speed: 80.,
        }
    }
}

//==============================================================================
// COMPONENTS
//==============================================================================

/// A marker component for our fly camera.
#[derive(Component)]
pub struct FlyCam;

//==============================================================================
// STARTUP SYSTEMS
//==============================================================================

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(20.0, 60.0, 20.0)
            .looking_at((-150., 30., 0.).into(), Vec3::Y),
        GlobalTransform::default(),
        FlyCam, // Add the marker
    ));
}

//==============================================================================
// UPDATE SYSTEMS
//==============================================================================

fn update_camera_move(
    mut q_camera: Query<&mut Transform, With<FlyCam>>,
    settings: Res<MovementSettings>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let Ok(mut transform) = q_camera.single_mut() else { return };
    let mut delta = Vec3::ZERO;

    let cam_fwd = transform.forward().as_vec3();
    let cam_right = transform.right().as_vec3();
    // let local_z = transform.local_z();
    // let forward = Vec3::new(local_z.x, 0., local_z.z).normalize();
    // let right = Vec3::new(local_z.z, 0., -local_z.x).normalize();

    if input.pressed(KeyCode::KeyW) {
        delta += cam_fwd;
    }
    if input.pressed(KeyCode::KeyS) {
        delta -= cam_fwd;
    }
    if input.pressed(KeyCode::KeyA) {
        delta -= cam_right;
    }
    if input.pressed(KeyCode::KeyD) {
        delta += cam_right;
    }
    if input.pressed(KeyCode::Space) {
        delta += Vec3::Y;
    }
    if input.pressed(KeyCode::ShiftLeft) {
        delta -= Vec3::Y;
    }

    transform.translation += delta.normalize_or_zero() * settings.speed * time.delta_secs();
}

fn update_camera_look(
    mut q_camera: Query<&mut Transform, With<FlyCam>>,
    mut mouse_motion_evr: EventReader<MouseMotion>,
    settings: Res<MovementSettings>,
    q_window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = q_window.single() else { return };
    if window.cursor_options.grab_mode == CursorGrabMode::None {
        return;
    }

    let Ok(mut transform) = q_camera.single_mut() else { return };

    for ev in mouse_motion_evr.read() {
        let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);

        // Using window.width() and window.height() for scaling is not ideal,
        // but it's a simple way to get decent sensitivity initially.
        let window_scale = window.height().min(window.width());
        pitch -= (ev.delta.y * settings.sensitivity * window_scale).to_radians();
        yaw -= (ev.delta.x * settings.sensitivity * window_scale).to_radians();

        pitch = pitch.clamp(-1.54, 1.54);

        transform.rotation =
            Quat::from_axis_angle(Vec3::Y, yaw) * Quat::from_axis_angle(Vec3::X, pitch);
    }
}

/// Grabs/releases the cursor when the user presses escape
fn cursor_grab(
    mut q_window: Query<&mut Window, With<PrimaryWindow>>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if !input.just_pressed(KeyCode::Escape) {
        return;
    }
    let Ok(mut window) = q_window.single_mut() else { return };

    window.cursor_options.grab_mode = match window.cursor_options.grab_mode {
        CursorGrabMode::None => CursorGrabMode::Locked,
        _ => CursorGrabMode::None,
    };
    window.cursor_options.visible = !window.cursor_options.visible;
}
