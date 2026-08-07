//! Scripted go-to-target controller (Phase 0 baseline / teacher).

use bevy::prelude::*;

use super::latent::PolicyState;
use super::obs::PrivilegedObs;

#[cfg(test)]
use super::obs::wrap_angle;

/// Continuous action used by the sim bridge (`AiActionInput` layout).
#[derive(Clone, Debug, Default)]
pub struct TrainAction {
    /// move_dir: x = strafe, y = forward (see player AI path).
    pub move_dir: Vec2,
    /// Yaw rate in radians/second (applied each physics tick).
    pub yaw_rate: f32,
}

/// Simple proportional go-to: turn toward target, thrust when roughly aligned.
///
/// Accepts private [`PolicyState`] for API parity with learned policies; Phase 0
/// scripted baseline **ignores** `z` (identity residual lives in the train loop).
pub fn scripted_go_to(obs: &PrivilegedObs, _z: &PolicyState) -> TrainAction {
    const MAX_YAW_RATE: f32 = 2.5; // rad/s
    const ALIGN_FULL_THRUST: f32 = 0.45; // rad
    const ALIGN_MIN_THRUST: f32 = 1.2; // rad

    let yaw_err = obs.target_yaw_err;
    let yaw_rate = (yaw_err * 3.0).clamp(-MAX_YAW_RATE, MAX_YAW_RATE);

    let abs_err = yaw_err.abs();
    let forward = if abs_err < ALIGN_FULL_THRUST {
        1.0
    } else if abs_err < ALIGN_MIN_THRUST {
        1.0 - (abs_err - ALIGN_FULL_THRUST) / (ALIGN_MIN_THRUST - ALIGN_FULL_THRUST)
    } else {
        0.15 // still creep so we don't stall while spinning
    };

    // Mild strafe toward target when yaw error is moderate (helps corners).
    let strafe = (yaw_err * 0.35).clamp(-0.5, 0.5) * (1.0 - forward * 0.5);

    TrainAction {
        // player AI: move_dir.x = right, move_dir.y = forward (then negated in player.rs)
        // Human WASD forward uses -forward in accel; AI uses move_forward = -move_dir.y
        // so positive move_dir.y means forward. Keep positive for go-to.
        move_dir: Vec2::new(-strafe, forward),
        yaw_rate,
    }
}

/// Unit test helpers: desired yaw error from two headings.
#[cfg(test)]
pub fn yaw_error(current: f32, desired: f32) -> f32 {
    wrap_angle(desired - current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::train::latent::PolicyState;
    use crate::train::obs::yaw_toward;

    #[test]
    fn turns_toward_positive_error() {
        let obs = PrivilegedObs {
            target_yaw_err: 0.8,
            target_dist: 20.0,
            ..default()
        };
        let a = scripted_go_to(&obs, &PolicyState::zeros());
        assert!(a.yaw_rate > 0.0);
        assert!(a.move_dir.y > 0.0);
    }

    #[test]
    fn full_thrust_when_aligned() {
        let obs = PrivilegedObs {
            target_yaw_err: 0.05,
            target_dist: 10.0,
            ..default()
        };
        let a = scripted_go_to(&obs, &PolicyState::zeros());
        assert!((a.move_dir.y - 1.0).abs() < 1e-3);
    }

    #[test]
    fn yaw_error_wraps_and_matches_go_to_sign() {
        let err = yaw_error(0.0, 0.5);
        assert!((err - 0.5).abs() < 1e-5);
        let wrap = yaw_error(3.0, -3.0);
        assert!(wrap.abs() < 1.0); // shortest path across ±π
        let desired = yaw_toward(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        let err2 = yaw_error(0.0, desired);
        assert!(err2.abs() > 0.5);
    }
}
