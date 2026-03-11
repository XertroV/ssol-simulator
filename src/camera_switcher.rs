use std::f32::consts::FRAC_PI_2;

use bevy::anti_alias::smaa::Smaa;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::{
    key_mapping::{KeyAction, KeyMapping},
    ui::{PauseMenuState, is_pause_menu_open},
};
use crate::player::{MovementSettings, PlayerCamera, PlayerModelEnt};
use crate::scene_loader::{PlayerStart, setup_scene};

pub use forced_cams::*;

mod forced_cams;

pub struct CameraSwitcherPlugin;

impl Plugin for CameraSwitcherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveCamera>()
            .add_systems(Startup, (
                setup_switch_camera.after(setup_scene),
                // attach_perf_ui_to_free_cam,
            ).chain())
            .add_systems(
                Update,
                (
                    (
                        update_switch_camera,
                        (
                            move_free_cam,
                            look_free_cam,
                            zoom_free_cam
                        ).chain().run_if(is_free_cam_mode),
                    ).run_if(is_camera_player_controlled),
                    (
                        update_forced_cam,
                        process_forced_cam_input,
                    ).run_if(is_camera_forced),
                ),
            );
    }
}

#[derive(PartialEq, Eq, Debug)]
enum CamCtrlType {
    PlayerControlled,
    Forced,
}

#[derive(PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum CameraMode {
    FirstPerson,
    Free,
    OrbitPlayer(Entity),
    MenuBg,
}

impl CameraMode {
    /// Player controlled => not forced
    pub fn is_player_controlled(&self) -> bool {
        self.ctrl_type() == CamCtrlType::PlayerControlled
    }

    /// Forced camera => not player controlled
    pub fn is_forced(&self) -> bool {
        self.ctrl_type() == CamCtrlType::Forced
    }

    fn ctrl_type(&self) -> CamCtrlType {
        match self {
            CameraMode::FirstPerson | CameraMode::Free => CamCtrlType::PlayerControlled,
            CameraMode::OrbitPlayer(_) | CameraMode::MenuBg => CamCtrlType::Forced,
        }
    }
}

#[derive(Resource)]
pub struct ActiveCamera(pub CameraMode);

impl Default for ActiveCamera {
    fn default() -> Self {
        Self(CameraMode::FirstPerson)
    }
}

#[derive(Component)]
pub struct FreeCam;

#[derive(Component)]
pub struct FreeCamPerfUI;

pub fn is_1st_person_mode(mode: Res<ActiveCamera>) -> bool {
    mode.0 == CameraMode::FirstPerson
}

pub fn is_free_cam_mode(mode: Res<ActiveCamera>) -> bool {
    mode.0 == CameraMode::Free
}

pub fn is_camera_player_controlled(mode: Res<ActiveCamera>) -> bool {
    mode.0.is_player_controlled()
}

pub fn is_camera_forced(mode: Res<ActiveCamera>) -> bool {
    mode.0.is_forced()
}

// spawn the free camera
fn setup_switch_camera(
    mut commands: Commands,
    q_start: Query<&Transform, With<PlayerStart>>,
) {
    let Ok(transform) = q_start.single() else {
        return;
    };
    commands.spawn((
        FreeCam,
        Camera3d::default(),
        Camera {
            is_active: false,
            ..default()
        },
        Smaa::default(),
        Projection::from(PerspectiveProjection {
            fov: 60.0f32.to_radians(),
            near: 0.3,
            far: 100_000.0,
            ..default()
        }),
        transform.clone(),
        Name::new("FreeCam"),
        // IsDefaultUiCamera,
    ));
    debug!("Free camera spawned at {:?}", transform.translation);
}

fn update_switch_camera(
    mut commands: Commands,
    mut q_fpv_cam: Query<(Entity, &mut Camera, Option<&IsDefaultUiCamera>), (With<PlayerCamera>, Without<FreeCam>)>,
    mut q_free_cam: Query<(Entity, &mut Camera, &mut Projection, Option<&IsDefaultUiCamera>), (With<FreeCam>, Without<PlayerCamera>)>,
    mut q_player_model: Query<&mut Visibility, With<PlayerModelEnt>>,
    mut active_cam: ResMut<ActiveCamera>,
    keys: Res<KeyMapping>,
    input: Res<ButtonInput<KeyCode>>,
    pause_menu: Option<Res<PauseMenuState>>,
) {
    if is_pause_menu_open(pause_menu.as_deref()) {
        return;
    }

    // We'll only change cameras if the active cam is player controlled.
    if !active_cam.0.is_player_controlled() {
        return;
    }

    // swap cams
    if keys.just_pressed(&input, KeyAction::FreeCam) {
        let Ok(mut fpv_cam) = q_fpv_cam.single_mut() else { return };
        let Ok(mut free_cam) = q_free_cam.single_mut() else { return };
        let Ok(mut p_model_vis) = q_player_model.single_mut() else { return };

        if fpv_cam.2.is_some() {
            commands.entity(fpv_cam.0).remove::<IsDefaultUiCamera>();
        } else if free_cam.3.is_some() {
            commands.entity(free_cam.0).remove::<IsDefaultUiCamera>();
        }

        active_cam.0 = match active_cam.0 {
            CameraMode::FirstPerson => {
                *p_model_vis = Visibility::Visible;
                fpv_cam.1.is_active = false;
                free_cam.1.is_active = true;
                if let Projection::Perspective(ref mut perspective) = *free_cam.2 {
                    // reset FoV
                    perspective.fov = 60.0f32.to_radians();
                }
                commands.entity(free_cam.0).insert(IsDefaultUiCamera);
                CameraMode::Free
            }
            CameraMode::Free => {
                *p_model_vis = Visibility::Hidden;
                fpv_cam.1.is_active = true;
                free_cam.1.is_active = false;
                commands.entity(fpv_cam.0).insert(IsDefaultUiCamera);
                CameraMode::FirstPerson
            }
            _ => return,
        };
        info!("Switched to {:?} camera", active_cam.0);
    }
}

fn move_free_cam(
    mut q_camera: Query<&mut Transform, With<FreeCam>>,
    settings: Res<MovementSettings>,
    input: Res<ButtonInput<KeyCode>>,
    keys: Res<KeyMapping>,
    pause_menu: Option<Res<PauseMenuState>>,
    time: Res<Time>,
) {
    if is_pause_menu_open(pause_menu.as_deref()) {
        return;
    }

    let Ok(mut transform) = q_camera.single_mut() else {
        return;
    };
    let mut delta = Vec3::ZERO;

    if keys.pressed(&input, KeyAction::Forward) {
        delta += transform.forward().as_vec3();
    }
    if keys.pressed(&input, KeyAction::Backward) {
        delta -= transform.forward().as_vec3();
    }
    if keys.pressed(&input, KeyAction::Left) {
        delta -= transform.right().as_vec3();
    }
    if keys.pressed(&input, KeyAction::Right) {
        delta += transform.right().as_vec3();
    }
    if keys.pressed(&input, KeyAction::FreeCamUp) {
        delta += Vec3::Y;
    }
    if keys.pressed(&input, KeyAction::FreeCamDown) {
        delta -= Vec3::Y;
    }

    transform.translation +=
        delta.normalize_or_zero() * settings.free_cam_speed * time.delta_secs();
}

fn look_free_cam(
    mut q_camera: Query<&mut Transform, With<FreeCam>>,
    mouse: Res<AccumulatedMouseMotion>,
    settings: Res<MovementSettings>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_cursor: Query<&CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(window) = q_window.single() else {
        return;
    };
    let Ok(cursor_options) = q_cursor.single() else {
        return;
    };
    if cursor_options.grab_mode == CursorGrabMode::None {
        return;
    }
    let Ok(mut transform) = q_camera.single_mut() else {
        return;
    };

    let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    let window_scale = window.height().min(window.width());
    pitch -= (mouse.delta.y * settings.mouse_sens * window_scale).to_radians();
    yaw -= (mouse.delta.x * settings.mouse_sens * window_scale).to_radians();
    pitch = pitch.clamp(-FRAC_PI_2, FRAC_PI_2);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    // transform.rotation = Quat::from_axis_angle(Vec3::Y, yaw) * Quat::from_axis_angle(Vec3::X, pitch);
}

fn zoom_free_cam(
    mut q_camera: Query<&mut Projection, With<FreeCam>>,
    mut i_wheel: MessageReader<MouseWheel>,
    // keys: Res<KeyMapping>,
) {
    let Ok(mut projection) = q_camera.single_mut() else {
        return;
    };
    for w in i_wheel.read() {
        if let Projection::Perspective(ref mut perspective) = *projection {
            perspective.fov = (perspective.fov.to_degrees() - w.y * 5.0f32)
                .clamp(10.0, 120.0)
                .to_radians();
        }
    }
}




#[allow(dead_code)]
pub trait HasFov {
    fn get_fov(&self) -> f32;
    fn get_aspect(&self) -> f32;
}

impl HasFov for Projection {
    fn get_fov(&self) -> f32 {
        match self {
            Projection::Perspective(perspective) => perspective.fov,
            Projection::Orthographic(_) => 0.0, // orthographic.scale * 2.0,
            Projection::Custom(c) => {
                match c.get::<PerspectiveProjection>() {
                    Some(p) => p.fov,
                    None => 0.0,
                }
            }
        }
    }

    fn get_aspect(&self) -> f32 {
        match self {
            Projection::Perspective(perspective) => perspective.aspect_ratio,
            Projection::Orthographic(_) => 1.0,
            Projection::Custom(c) => {
                match c.get::<PerspectiveProjection>() {
                    Some(p) => p.aspect_ratio,
                    None => 1.0, // Default aspect ratio
                }
            }
        }
    }
}
