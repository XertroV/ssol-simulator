//! AI Gizmos - Visual debug aids for AI mode
//!
//! Draws arrows and other visual indicators to help visualize what the AI agent "sees".

use std::f32::consts::PI;

use bevy::prelude::*;

use super::{AiConfig, AiObservations};
use crate::player::{Player, PlayerCamera};

/// Maximum ray distance for wall detection (matches observations.rs)
const MAX_RAY_DISTANCE: f32 = 150.0;

/// Plugin that adds AI visualization gizmos
pub struct AiGizmosPlugin;

impl Plugin for AiGizmosPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                draw_closest_orb_arrow,
                draw_wall_ray_visualization,
            )
                .run_if(is_ai_mode_with_gizmos),
        );
    }
}

/// Run condition: only draw gizmos when AI mode is enabled
fn is_ai_mode_with_gizmos(config: Res<AiConfig>) -> bool {
    config.enabled
}

/// Draw an arrow from the player towards the closest orb
fn draw_closest_orb_arrow(
    mut gizmos: Gizmos,
    observations: Res<AiObservations>,
    q_player: Query<&Transform, With<Player>>,
    q_camera: Query<&Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    // Get closest orb target from observations
    // orb_targets[0] is: (direction_local, distance, orb_id)
    let (direction_local, distance, orb_id) = observations.orb_targets[0];

    // Skip if no valid target (orb_id == -1)
    if orb_id < 0.0 || distance <= 0.0 {
        return;
    }

    let Ok(player_transform) = q_player.single() else {
        return;
    };

    // Get camera pitch for full rotation
    let camera_pitch = if let Ok(camera_transform) = q_camera.single() {
        let (_, pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);
        pitch
    } else {
        0.0
    };

    // The direction is in local player coordinates (including camera pitch)
    // We need to transform it back to world coordinates
    // Player yaw is from player_transform.rotation
    // Camera pitch needs to be combined

    // Build the full camera rotation (player yaw + camera pitch)
    let (yaw, _, _) = player_transform.rotation.to_euler(EulerRot::YXZ);
    let full_rotation = Quat::from_euler(EulerRot::YXZ, yaw, camera_pitch, 0.0);

    // Transform local direction to world direction
    let direction_world = full_rotation * direction_local;

    // Arrow start position: slightly in front of the player at eye level
    let start_pos = player_transform.translation + Vec3::Y * 1.5;

    // Arrow length: scale with distance but cap it
    let arrow_length = distance.min(15.0).max(2.0);
    let end_pos = start_pos + direction_world * arrow_length;

    // Draw the arrow with a distinctive color (cyan for orb targets)
    let arrow_color = Color::srgba(0.0, 1.0, 1.0, 0.9);

    // Draw main arrow line
    gizmos.line(start_pos, end_pos, arrow_color);

    // Draw arrowhead (two lines forming a V)
    let arrowhead_size = arrow_length * 0.15;
    let arrow_dir = direction_world.normalize();

    // Create perpendicular vectors for arrowhead
    let up = Vec3::Y;
    let right = arrow_dir.cross(up).normalize_or_zero();
    let local_up = right.cross(arrow_dir).normalize_or_zero();

    // Arrowhead points
    let head1 = end_pos - arrow_dir * arrowhead_size + right * arrowhead_size * 0.5;
    let head2 = end_pos - arrow_dir * arrowhead_size - right * arrowhead_size * 0.5;
    let head3 = end_pos - arrow_dir * arrowhead_size + local_up * arrowhead_size * 0.5;
    let head4 = end_pos - arrow_dir * arrowhead_size - local_up * arrowhead_size * 0.5;

    gizmos.line(end_pos, head1, arrow_color);
    gizmos.line(end_pos, head2, arrow_color);
    gizmos.line(end_pos, head3, arrow_color);
    gizmos.line(end_pos, head4, arrow_color);

    // Draw a small sphere at the target position if close enough
    if distance < 50.0 {
        let target_pos = start_pos + direction_world * distance;
        gizmos.sphere(Isometry3d::from_translation(target_pos), 0.5, arrow_color);
    }
}

/// Draw wall ray visualization as a donut/pie chart around the player
/// and 3D ray lines showing wall detection distances.
fn draw_wall_ray_visualization(
    mut gizmos: Gizmos,
    observations: Res<AiObservations>,
    q_player: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = q_player.single() else {
        return;
    };

    let wall_rays = &observations.wall_rays;
    let num_rays = wall_rays.len();
    if num_rays == 0 {
        return;
    }

    // Vertical offset above player position
    let vertical_offset = -0.25;
    let center = player_transform.translation + Vec3::Y * vertical_offset;

    // Get player's yaw for rotating rays to world space
    let (yaw, _, _) = player_transform.rotation.to_euler(EulerRot::YXZ);

    // Donut parameters
    let inner_radius = 1.0;
    let max_outer_radius = 3.0; // Maximum outer radius when distance = 1.0 (far/safe)
    let num_arcs = 8; // Number of concentric arcs per wedge for filled appearance

    // Angle per ray
    let angle_per_ray = 2.0 * PI / num_rays as f32;

    for (i, &distance) in wall_rays.iter().enumerate() {
        // Color based on distance: red (close/danger) to green (far/safe)
        // HSL: hue 0 = red, hue 120 = green
        let hue = distance * 120.0;
        let color = Color::hsl(hue, 1.0, 0.5);

        // Calculate ray direction in local space (same as observations.rs)
        // Use angle_per_ray to work for any number of rays
        let ray_angle = (i as f32) * angle_per_ray;
        let local_dir = Vec3::new(ray_angle.cos(), 0.0, ray_angle.sin());

        // Rotate to world space using player's yaw
        let world_dir = player_transform.rotation * local_dir;

        // === 3D Ray Line ===
        let ray_length = distance * MAX_RAY_DISTANCE;
        let ray_end = center + world_dir * ray_length;
        gizmos.line(center, ray_end, color);

        // === Donut Wedge ===
        // Outer radius scales with distance (far = larger, close = smaller)
        let outer_radius = inner_radius + (max_outer_radius - inner_radius) * distance;

        // Draw concentric arcs to fill the wedge
        for arc_idx in 0..num_arcs {
            let t = (arc_idx as f32 + 0.5) / num_arcs as f32;
            let arc_radius = inner_radius + (outer_radius - inner_radius) * t;

            // Center angle of this wedge (offset by half wedge for centering)
            // The ray points at angle ray_angle in local space, wedge should be centered on it
            let wedge_center_angle = ray_angle;

            // Rotation to orient the arc:
            // Arc is drawn in XZ plane, we need to rotate around Y axis
            // The arc_3d draws from -arc_angle/2 to +arc_angle/2 around the isometry's forward
            // We need to rotate so the arc is centered on the ray direction
            let arc_rotation = Quat::from_rotation_y(yaw + wedge_center_angle);

            // Position the arc at player position + vertical offset
            let arc_isometry = Isometry3d::new(center, arc_rotation);

            // Draw the arc with small resolution for smooth appearance
            gizmos
                .arc_3d(angle_per_ray, arc_radius, arc_isometry, color)
                .resolution(8);
        }
    }
}
