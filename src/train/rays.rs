//! Horizontal wall rays for train obs (no `--features ai`).

use std::f32::consts::PI;

use bevy::prelude::*;
use bevy_rapier3d::prelude::{QueryFilter, RapierContext};

/// Number of horizontal wall rays (22.5° spacing, full 360°).
pub const WALL_RAY_COUNT: usize = 16;

/// Max ray distance in world units.
pub const MAX_RAY_DISTANCE: f32 = 40.0;

/// Vertical offset from player origin (player collider ~5 tall; slightly below center).
pub const RAY_HEIGHT_OFFSET: f32 = -2.0;

/// Indices near local forward (−Z). Angle i * π/8; forward ≈ 3π/2 → index 12.
pub const FORWARD_RAY_INDICES: [usize; 3] = [11, 12, 13];

/// Cast 16 body-relative horizontal rays. Encoding: **0 = touch**, **1 = clear** (toi / max).
pub fn cast_wall_rays(
    player_tf: &Transform,
    rapier: &RapierContext,
    player_entity: Entity,
) -> [f32; WALL_RAY_COUNT] {
    let mut rays = [1.0_f32; WALL_RAY_COUNT];
    let origin = player_tf.translation + Vec3::Y * RAY_HEIGHT_OFFSET;
    let filter = QueryFilter::default()
        .exclude_sensors()
        .exclude_collider(player_entity);

    for i in 0..WALL_RAY_COUNT {
        let angle = (i as f32) * (PI / 8.0);
        let local_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
        let world_dir = player_tf.rotation * local_dir;
        if let Some((_e, toi)) =
            rapier.cast_ray(origin, world_dir, MAX_RAY_DISTANCE, true, filter)
        {
            rays[i] = (toi / MAX_RAY_DISTANCE).clamp(0.0, 1.0);
        }
    }
    rays
}

/// Minimum clearance among forward-facing rays (0 = blocked, 1 = open).
pub fn frontal_clearance(rays: &[f32; WALL_RAY_COUNT]) -> f32 {
    FORWARD_RAY_INDICES
        .iter()
        .map(|&i| rays[i])
        .fold(1.0_f32, f32::min)
}

/// Which side is freer: positive → prefer left strafe / left yaw, negative → right.
pub fn free_side_bias(rays: &[f32; WALL_RAY_COUNT]) -> f32 {
    // Leftish: indices 13–15,0–2; rightish: 6–10
    let left: f32 = [13, 14, 15, 0, 1, 2].iter().map(|&i| rays[i]).sum::<f32>() / 6.0;
    let right: f32 = [6, 7, 8, 9, 10].iter().map(|&i| rays[i]).sum::<f32>() / 5.0;
    left - right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontal_clearance_picks_min_forward() {
        let mut r = [1.0; WALL_RAY_COUNT];
        r[12] = 0.2;
        r[11] = 0.5;
        assert!((frontal_clearance(&r) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn free_side_bias_sign() {
        let mut r = [0.5; WALL_RAY_COUNT];
        for i in [13, 14, 15, 0, 1, 2] {
            r[i] = 0.9;
        }
        for i in [6, 7, 8, 9, 10] {
            r[i] = 0.1;
        }
        assert!(free_side_bias(&r) > 0.0);
    }
}
