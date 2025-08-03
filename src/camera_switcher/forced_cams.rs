use bevy::{math::bounding::BoundingVolume, prelude::*};

use crate::{key_mapping::KeyMapping, player::Player, scene::{self, CalculatedData}};

use super::{ActiveCamera, FreeCam, CameraMode};

/// Updates the forced camera transform based on the active camera mode.
pub fn update_forced_cam(
    mut commands: Commands,
    mut q_freecam: Query<(&mut Transform,), (With<FreeCam>, Without<Player>)>,
    q_player: Query<&Transform, (With<Player>, Without<FreeCam>)>,
    active_cam: Res<ActiveCamera>,
    scene_data: Res<CalculatedData>,
    time: Res<Time>,
) {
    if !active_cam.0.is_forced() { return; }
    let Ok((mut fc_transform,)) = q_freecam.single_mut() else { return; };

    match active_cam.0 {
        CameraMode::OrbitPlayer(p_ent) => update_orbit_player_cam(
            &mut commands,
            &mut fc_transform,
            p_ent,
            &q_player,
            &time,
        ),
        CameraMode::MenuBg => update_menu_bg_cam(&mut fc_transform, &scene_data, &time),
        _ => {}
    }
}

const MENU_BG_CAM_DIST: f32 = 10.0;

fn update_menu_bg_cam(
    transform: &mut Transform,
    scene_data: &CalculatedData,
    time: &Time,
) {
    // The camera target follows an elipse based on the orbs AABB in scene_data.
    // The elipse is in the XZ plane. Radii are 75% of axis half-sizes.
    let bb = scene_data.orbs_bb();
    let bb_half_size = bb.half_size();
    let rs = bb_half_size.xz() * 0.75;
    // Use polar coordinates to calculate the position.
    let angle = time.elapsed_secs() * 0.1; // Adjust speed as needed
    let xz = rs * Vec2::from(angle.sin_cos());
    let bb_center_a = bb.center();
    let bb_center: Vec3 = bb_center_a.into();
    let ellipse_pos = Vec3::new(xz.x, bb_center.y, xz.y);
    let dir_out = (ellipse_pos - bb_center).normalize();
    // The camera is outside and above the ellipse, looking down at target.
    transform.translation = ellipse_pos + dir_out.with_y(0.3) * MENU_BG_CAM_DIST;
    transform.look_at(ellipse_pos, Vec3::Y);
}

// Height offset for the orbit camera
const ORBIT_CAM_HEIGHT: f32 = 3.0;
// Rotation rate (radians per second) for the orbit camera
const ORBIT_CAM_ROTATE_RATE: f32 = 0.5;

fn update_orbit_player_cam(
    _commands: &mut Commands,
    fc_transform: &mut Transform,
    player_ent: Entity,
    q_player: &Query<&Transform, (With<Player>, Without<FreeCam>)>,
    time: &Time,
) {
    let Ok(player_transform) = q_player.get(player_ent) else { return };
    // Example implementation for orbiting around the player
    let orbit_radius = 5.0;
    let angle = time.elapsed_secs() * ORBIT_CAM_ROTATE_RATE;
    fc_transform.translation = player_transform.translation + Vec3::new(
        orbit_radius * angle.cos(),
        ORBIT_CAM_HEIGHT,
        orbit_radius * angle.sin(),
    );
    fc_transform.look_at(player_transform.translation, Vec3::Y);
}


pub fn process_forced_cam_input(
    // mut commands: Commands,
    // mut q_freecam: Query<(&mut Transform, &mut Projection), With<FreeCam>>,
    active_cam: Res<ActiveCamera>,
    // input: Res<ButtonInput<KeyCode>>,
    // keys: Res<KeyMapping>,
) {
    if !active_cam.0.is_forced() { return; }

    // let Ok((mut transform, mut projection)) = q_freecam.single_mut() else { return; };
}
