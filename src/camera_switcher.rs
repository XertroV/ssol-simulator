use std::f32::consts::FRAC_PI_2;

use bevy::core_pipeline::smaa::Smaa;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use iyes_perf_ui::prelude::{PerfUiDefaultEntries, PerfUiRoot};

use crate::key_mapping::KeyMapping;
use crate::player::{MovementSettings, PlayerCamera, PlayerModelEnt};
use crate::scene_loader::{PlayerStart, setup_scene};

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
                    update_switch_camera,
                    (
                        move_free_cam,
                        look_free_cam,
                        zoom_free_cam
                    ).run_if(is_free_cam_mode),
                ),
            );
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum CameraMode {
    FirstPerson,
    Free,
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
            far: 10000.0,
            ..default()
        }),
        transform.clone(),
        Name::new("FreeCam"),
        IsDefaultUiCamera,
    ));
    commands.spawn((
        FreeCamPerfUI,
        PerfUiDefaultEntries::default(),
    ));
    debug!("Free camera spawned at {:?}", transform.translation);
}

// fn attach_perf_ui_to_free_cam(
//     mut commands: Commands,
//     q_free_cam: Query<Entity, With<FreeCam>>,
//     q_perf_ui: Query<Entity, (With<FreeCamPerfUI>, With<PerfUiRoot>)>,
// ) {
//     let cam_ent = q_free_cam.single().expect("FreeCam exists");
//     let perf_ui_ent = q_perf_ui.single().expect("PerfUiRoot exists");
//     // commands.entity(perf_ui_ent).insert(UiTargetCamera(cam_ent));
// }

fn update_switch_camera(
    mut commands: Commands,
    mut q_fpv_cam: Query<(Entity, &mut Camera, Option<&IsDefaultUiCamera>), (With<PlayerCamera>, Without<FreeCam>)>,
    mut q_free_cam: Query<(Entity, &mut Camera, &mut Projection, Option<&IsDefaultUiCamera>), (With<FreeCam>, Without<PlayerCamera>)>,
    mut q_player_model: Query<&mut Visibility, With<PlayerModelEnt>>,
    mut active_cam: ResMut<ActiveCamera>,
    keys: Res<KeyMapping>,
    input: Res<ButtonInput<KeyCode>>,
) {
    // swap cams
    if input.just_pressed(keys.free_cam) {
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
        };
        info!("Switched to {:?} camera", active_cam.0);
    }
}

fn move_free_cam(
    mut commands: Commands,
    mut q_camera: Query<(Entity, &mut Transform), With<FreeCam>>,
    mut perf_vis: Query<&mut Visibility, With<FreeCamPerfUI>>,
    settings: Res<MovementSettings>,
    input: Res<ButtonInput<KeyCode>>,
    keys: Res<KeyMapping>,
    time: Res<Time>,
) {
    let Ok((cam_ent, mut transform)) = q_camera.single_mut() else {
        return;
    };
    let mut delta = Vec3::ZERO;

    if input.pressed(keys.forward) {
        delta += transform.forward().as_vec3();
    }
    if input.pressed(keys.backward) {
        delta -= transform.forward().as_vec3();
    }
    if input.pressed(keys.left) {
        delta -= transform.right().as_vec3();
    }
    if input.pressed(keys.right) {
        delta += transform.right().as_vec3();
    }
    if input.pressed(keys.free_cam_up) {
        delta += Vec3::Y;
    }
    if input.pressed(keys.free_cam_down) {
        delta -= Vec3::Y;
    }

    transform.translation +=
        delta.normalize_or_zero() * settings.free_cam_speed * time.delta_secs();

    if input.just_pressed(keys.fps_stats) {
        if let Ok(mut vis) = perf_vis.single_mut() {
            // toggle visibility of the performance UI
            *vis = match *vis {
                Visibility::Visible => Visibility::Hidden,
                Visibility::Hidden => Visibility::Visible,
                Visibility::Inherited => Visibility::Hidden,
            };
            info!("Performance UI visibility toggled to {:?}", vis);
        }
    }
}

fn look_free_cam(
    mut q_camera: Query<&mut Transform, With<FreeCam>>,
    mouse: Res<AccumulatedMouseMotion>,
    settings: Res<MovementSettings>,
    q_window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = q_window.single() else {
        return;
    };
    if window.cursor_options.grab_mode == CursorGrabMode::None {
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
    mut i_wheel: EventReader<MouseWheel>,
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
