//! Goal-conditioned reward for Phase 0 act steps (sim-side).
//!
//! Potential-based distance shaping: `-dist_coef * (d' - d)` so moving closer
//! yields non-negative shaping relative to the step cost.

use bevy::prelude::*;

use super::obs::PrivilegedObs;

/// Weights for [`act_reward`].
#[derive(Resource, Clone, Debug)]
pub struct RewardConfig {
    /// Per orb collected during the act window (default 1.0).
    pub orb: f32,
    /// Bonus when the episode finishes successfully (default 10.0).
    pub finish: f32,
    /// Fixed cost applied each act (default -0.001).
    pub step_cost: f32,
    /// Potential-based shaping: `-dist_coef * (d' - d)` (default 0.01).
    pub dist_coef: f32,
    /// Collision / contact penalty scale (default 0.0 until contact is exposed).
    pub collision_coef: f32,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            orb: 1.0,
            finish: 10.0,
            step_cost: -0.001,
            dist_coef: 0.01,
            collision_coef: 0.0,
        }
    }
}

/// Dense reward for one policy act given previous target distance and current obs.
///
/// ```text
/// r = step_cost
///   + orb * orbs_gained
///   + finish * finished
///   - dist_coef * (target_dist' - prev_dist)
///   (+ collision term when contact is available; currently 0)
/// ```
pub fn act_reward(
    cfg: &RewardConfig,
    prev_dist: f32,
    obs: &PrivilegedObs,
    orbs_gained: u32,
    finished: bool,
) -> f32 {
    let shaping = -cfg.dist_coef * (obs.target_dist - prev_dist);
    let mut r = cfg.step_cost + cfg.orb * orbs_gained as f32 + shaping;
    if finished {
        r += cfg.finish;
    }
    // collision_coef reserved until contact signal is exposed on PrivilegedObs.
    let _ = cfg.collision_coef;
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::default;

    fn obs_with_dist(dist: f32) -> PrivilegedObs {
        PrivilegedObs {
            target_dist: dist,
            ..default()
        }
    }

    #[test]
    fn closer_to_goal_is_non_negative_shaping() {
        let cfg = RewardConfig::default();
        let r = act_reward(&cfg, 10.0, &obs_with_dist(8.0), 0, false);
        assert!(r > cfg.step_cost);
    }

    #[test]
    fn farther_from_goal_is_negative_shaping() {
        let cfg = RewardConfig::default();
        let r = act_reward(&cfg, 8.0, &obs_with_dist(10.0), 0, false);
        assert!(r < cfg.step_cost);
    }

    #[test]
    fn orb_and_finish_bonuses() {
        let cfg = RewardConfig::default();
        let r = act_reward(&cfg, 5.0, &obs_with_dist(5.0), 2, true);
        // no shaping (same dist): step_cost + 2*orb + finish
        let expected = cfg.step_cost + 2.0 * cfg.orb + cfg.finish;
        assert!((r - expected).abs() < 1e-6);
    }

    #[test]
    fn shaping_matches_potential_formula() {
        let cfg = RewardConfig {
            dist_coef: 0.5,
            step_cost: 0.0,
            orb: 0.0,
            finish: 0.0,
            collision_coef: 0.0,
        };
        let prev = 10.0;
        let next = 7.0;
        let r = act_reward(&cfg, prev, &obs_with_dist(next), 0, false);
        let expected = -cfg.dist_coef * (next - prev);
        assert!((r - expected).abs() < 1e-6);
        assert!(r > 0.0);
    }
}
