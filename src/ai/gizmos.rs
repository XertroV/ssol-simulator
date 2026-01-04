//! AI Gizmos - Visual debug aids for AI mode
//!
//! Draws arrows and other visual indicators to help visualize what the AI agent "sees".

use bevy::prelude::*;

use super::{AiConfig, AiObservations};
use crate::player::{Player, PlayerCamera};

/// Plugin that adds AI visualization gizmos
pub struct AiGizmosPlugin;

impl Plugin for AiGizmosPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw_closest_orb_arrow.run_if(is_ai_mode_with_gizmos));
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
