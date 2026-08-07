//! Privileged (non-pixel) observations for Phase 0 training.
//!
//! Does **not** include private residual latent `z` — that lives in
//! [`crate::train::PolicyState`] only. Env export length is independent of
//! [`crate::train::LATENT_DIM`].

use bevy::prelude::*;

/// Compact privileged observation vector (goal-conditioned).
///
/// Designed so a single policy can later run at different act rates by including
/// `control_dt` in the observation.
///
/// **No latent `z`:** bridges must not require PolicyState for reset/step obs.
/// Fields are filled every physics tick for the train loop and ML bridges; not
/// all are read by the scripted teacher yet.
#[derive(Resource, Clone, Debug, Default)]
#[allow(dead_code)] // intentional privileged obs schema; as_vec + future policies
pub struct PrivilegedObs {
    pub player_pos: Vec3,
    pub player_vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub speed_of_light: f32,
    pub speed_multiplier: f32,
    pub lorentz: f32,
    pub score: u32,
    pub nb_orbs: u32,
    pub player_time: f32,
    /// Seconds between policy decisions (variable act-rate feature).
    pub control_dt: f32,
    /// Relative vector to current high-level target (world xz-ish).
    pub target_rel: Vec3,
    pub target_dist: f32,
    /// Bearing error to target in radians (desired_yaw - yaw), wrapped to [-pi, pi].
    pub target_yaw_err: f32,
    /// Current target orb_id, or None when targeting finish arch / none.
    pub target_orb_id: Option<u8>,
    pub episode_tick: u32,
}

impl PrivilegedObs {
    /// Flat f32 vector for future ML bridges (stable field order).
    #[allow(dead_code)] // Phase 1+ ML bridge API; unit-tested
    pub fn as_vec(&self) -> Vec<f32> {
        vec![
            self.player_pos.x,
            self.player_pos.y,
            self.player_pos.z,
            self.player_vel.x,
            self.player_vel.y,
            self.player_vel.z,
            self.yaw,
            self.pitch,
            self.speed,
            self.speed_of_light,
            self.speed_multiplier,
            self.lorentz,
            self.score as f32,
            self.nb_orbs as f32,
            self.player_time,
            self.control_dt,
            self.target_rel.x,
            self.target_rel.y,
            self.target_rel.z,
            self.target_dist,
            self.target_yaw_err,
            self.target_orb_id.map(|id| id as f32).unwrap_or(-1.0),
            self.episode_tick as f32,
        ]
    }
}

/// Wrap angle to [-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let mut x = a;
    while x > std::f32::consts::PI {
        x -= 2.0 * std::f32::consts::PI;
    }
    while x < -std::f32::consts::PI {
        x += 2.0 * std::f32::consts::PI;
    }
    x
}

/// Yaw that faces from `from` toward `to` in Bevy (forward = -Z).
pub fn yaw_toward(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    // atan2(-x, -z): 0 yaw looks down -Z
    (-d.x).atan2(-d.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_vec_has_stable_len_and_sentinel() {
        let obs = PrivilegedObs {
            score: 2,
            target_orb_id: None,
            control_dt: 0.1,
            ..default()
        };
        let v = obs.as_vec();
        assert_eq!(v.len(), 23);
        assert_eq!(v[12], 2.0); // score
        assert_eq!(v[15], 0.1); // control_dt
        assert_eq!(v[21], -1.0); // no target orb
    }

    #[test]
    fn wrap_angle_bounds() {
        assert!(wrap_angle(4.0).abs() <= std::f32::consts::PI + 1e-5);
        assert!((wrap_angle(0.25) - 0.25).abs() < 1e-5);
    }
}
