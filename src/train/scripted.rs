//! Scripted go-to-target controller (teacher for BC / residual RL).

use bevy::prelude::*;

use super::latent::PolicyState;
use super::obs::PrivilegedObs;
use super::rays::{free_side_bias, frontal_clearance};

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

impl TrainAction {
    pub fn as_array(&self) -> [f32; 3] {
        [self.move_dir.x, self.move_dir.y, self.yaw_rate]
    }
}

/// Goal-conditioned go-to teacher.
///
/// Pure proportional go-to by default (matches Phase 0 baseline quality).
/// Rays are in [`PrivilegedObs`] for BC/RL; heuristic avoidance is opt-in via
/// [`scripted_go_to_with_avoid`] because naive ray heuristics *hurt* open WR tours
/// (foliage/fence false positives).
///
/// Accepts private [`PolicyState`] for API parity; scripted baseline ignores `z`.
pub fn scripted_go_to(obs: &PrivilegedObs, z: &PolicyState) -> TrainAction {
    scripted_go_to_inner(obs, z, false)
}

/// Same as [`scripted_go_to`] but with soft frontal-ray avoidance (experimental).
#[allow(dead_code)] // public experiment for residual/teacher ablations
pub fn scripted_go_to_with_avoid(obs: &PrivilegedObs, z: &PolicyState) -> TrainAction {
    scripted_go_to_inner(obs, z, true)
}

fn scripted_go_to_inner(obs: &PrivilegedObs, _z: &PolicyState, avoid: bool) -> TrainAction {
    const MAX_YAW_RATE: f32 = 2.5;
    const ALIGN_FULL_THRUST: f32 = 0.45;
    const ALIGN_MIN_THRUST: f32 = 1.2;
    const BLOCK_NEAR: f32 = 0.12;
    const BLOCK_SOFT: f32 = 0.28;

    let yaw_err = obs.target_yaw_err;
    let mut yaw_rate = (yaw_err * 3.0).clamp(-MAX_YAW_RATE, MAX_YAW_RATE);

    let abs_err = yaw_err.abs();
    let mut forward = if abs_err < ALIGN_FULL_THRUST {
        1.0
    } else if abs_err < ALIGN_MIN_THRUST {
        1.0 - (abs_err - ALIGN_FULL_THRUST) / (ALIGN_MIN_THRUST - ALIGN_FULL_THRUST)
    } else {
        0.15
    };

    let mut strafe = (yaw_err * 0.35).clamp(-0.5, 0.5) * (1.0 - forward * 0.5);

    if avoid {
        let front = frontal_clearance(&obs.wall_rays);
        let side = free_side_bias(&obs.wall_rays);
        if front < BLOCK_SOFT {
            let t = ((front - BLOCK_NEAR) / (BLOCK_SOFT - BLOCK_NEAR)).clamp(0.0, 1.0);
            forward *= 0.35 + 0.65 * t;
            if side.abs() > 0.05 {
                let a = side.signum() * (1.0 - t);
                yaw_rate =
                    (yaw_rate + a * MAX_YAW_RATE * 0.35).clamp(-MAX_YAW_RATE, MAX_YAW_RATE);
                strafe = (strafe + a * 0.45).clamp(-1.0, 1.0);
            }
        }
    }

    TrainAction {
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
    use crate::train::rays::WALL_RAY_COUNT;

    #[test]
    fn turns_toward_positive_error() {
        let obs = PrivilegedObs {
            target_yaw_err: 0.8,
            target_dist: 20.0,
            wall_rays: [1.0; WALL_RAY_COUNT],
            ..default()
        };
        let a = scripted_go_to(&obs, &PolicyState::zeros());
        assert!(a.yaw_rate > 0.0);
        assert!(a.move_dir.y > 0.0);
    }

    #[test]
    fn full_thrust_when_aligned_and_clear() {
        let obs = PrivilegedObs {
            target_yaw_err: 0.05,
            target_dist: 10.0,
            wall_rays: [1.0; WALL_RAY_COUNT],
            ..default()
        };
        let a = scripted_go_to(&obs, &PolicyState::zeros());
        assert!((a.move_dir.y - 1.0).abs() < 1e-3);
    }

    #[test]
    fn avoid_mode_reduces_forward_when_blocked() {
        let mut rays = [1.0; WALL_RAY_COUNT];
        for i in [11, 12, 13] {
            rays[i] = 0.08;
        }
        for i in [13, 14, 15, 0] {
            rays[i] = 0.9;
        }
        let obs = PrivilegedObs {
            target_yaw_err: 0.0,
            target_dist: 10.0,
            wall_rays: rays,
            ..default()
        };
        let plain = scripted_go_to(&obs, &PolicyState::zeros());
        let avoid = scripted_go_to_with_avoid(&obs, &PolicyState::zeros());
        assert!((plain.move_dir.y - 1.0).abs() < 1e-3);
        assert!(avoid.move_dir.y < plain.move_dir.y);
    }

    #[test]
    fn yaw_error_wraps_and_matches_go_to_sign() {
        let err = yaw_error(0.0, 0.5);
        assert!((err - 0.5).abs() < 1e-5);
        let wrap = yaw_error(3.0, -3.0);
        assert!(wrap.abs() < 1.0);
        let desired = yaw_toward(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        let err2 = yaw_error(0.0, desired);
        assert!(err2.abs() > 0.5);
    }
}
