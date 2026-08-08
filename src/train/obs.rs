//! Privileged (non-pixel) observations for training.
//!
//! Does **not** include private residual latent `z` — that lives in
//! [`crate::train::PolicyState`] only. Env export length is independent of
//! [`crate::train::LATENT_DIM`].
//!
//! **Schema v2:** base 23 floats + [`WALL_RAY_COUNT`] wall rays. See [`OBS_DIM`].

use bevy::prelude::*;

use super::rays::WALL_RAY_COUNT;

/// Schema version for dumps / Python loaders (bump when field order changes).
pub const OBS_SCHEMA_VERSION: u32 = 2;

/// Base privileged fields (without rays).
pub const OBS_BASE_DIM: usize = 23;

/// Full flat observation length = base + wall rays.
pub const OBS_DIM: usize = OBS_BASE_DIM + WALL_RAY_COUNT;

/// Compact privileged observation vector (goal-conditioned + local geometry).
///
/// Designed so a single policy can later run at different act rates by including
/// `control_dt` in the observation.
///
/// **No latent `z`:** bridges must not require PolicyState for reset/step obs.
#[derive(Resource, Clone, Debug)]
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
    /// Relative vector to current high-level target (world).
    pub target_rel: Vec3,
    pub target_dist: f32,
    /// Bearing error to target in radians (desired_yaw - yaw), wrapped to [-pi, pi].
    pub target_yaw_err: f32,
    /// Current target orb_id, or None when targeting finish arch / none.
    pub target_orb_id: Option<u8>,
    pub episode_tick: u32,
    /// Horizontal wall rays: **0 = touch**, **1 = clear** (toi / max range).
    pub wall_rays: [f32; WALL_RAY_COUNT],
}

impl Default for PrivilegedObs {
    fn default() -> Self {
        Self {
            player_pos: Vec3::ZERO,
            player_vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            speed: 0.0,
            speed_of_light: 0.0,
            speed_multiplier: 0.0,
            lorentz: 1.0,
            score: 0,
            nb_orbs: 0,
            player_time: 0.0,
            control_dt: 0.1,
            target_rel: Vec3::ZERO,
            target_dist: 0.0,
            target_yaw_err: 0.0,
            target_orb_id: None,
            episode_tick: 0,
            wall_rays: [1.0; WALL_RAY_COUNT],
        }
    }
}

impl PrivilegedObs {
    /// Flat f32 vector for ML bridges (stable field order; schema v2).
    ///
    /// Layout: 23 base fields, then 16 wall rays.
    pub fn as_vec(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(OBS_DIM);
        v.extend_from_slice(&[
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
        ]);
        v.extend_from_slice(&self.wall_rays);
        debug_assert_eq!(v.len(), OBS_DIM);
        v
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
    (-d.x).atan2(-d.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::train::latent::LATENT_DIM;

    #[test]
    fn as_vec_schema_v2_len() {
        let obs = PrivilegedObs {
            score: 2,
            target_orb_id: None,
            control_dt: 0.1,
            wall_rays: [0.5; WALL_RAY_COUNT],
            ..default()
        };
        let v = obs.as_vec();
        assert_eq!(v.len(), OBS_DIM);
        assert_eq!(OBS_DIM, 39);
        assert_eq!(v[12], 2.0); // score
        assert_eq!(v[15], 0.1); // control_dt
        assert_eq!(v[21], -1.0); // no target orb
        assert_eq!(v[23], 0.5); // first ray
        // Latent dim independent of env export.
        assert_ne!(LATENT_DIM, OBS_DIM);
    }

    #[test]
    fn wrap_angle_bounds() {
        assert!(wrap_angle(4.0).abs() <= std::f32::consts::PI + 1e-5);
        assert!((wrap_angle(0.25) - 0.25).abs() < 1e-5);
    }
}
