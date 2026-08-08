//! Private residual latent (`PolicyState` / `z`) for hierarchical thinking.
//!
//! **`z` is private:** bridges must not require it for reset/step observations.
//! It is not part of [`PrivilegedObs`] / [`PrivilegedObs::as_vec`] env export.
//! Lives on the trainer / episode only; learned residual `f_θ` comes later.

use super::obs::PrivilegedObs;
use super::scripted::TrainAction;

/// Default latent dimension (Python / RL should mirror).
pub const LATENT_DIM: usize = 32;

/// Private residual latent — NOT part of PrivilegedObs / env export.
#[derive(Clone, Debug)]
pub struct PolicyState {
    /// Length [`LATENT_DIM`].
    pub z: Vec<f32>,
}

impl PolicyState {
    pub fn zeros() -> Self {
        Self {
            z: vec![0.0; LATENT_DIM],
        }
    }
}

impl Default for PolicyState {
    fn default() -> Self {
        Self::zeros()
    }
}

/// Residual update: `z <- normalize(z + f(s,g,z))`.
///
/// Phase 0 scripted: `f = 0` ([`IdentityLatent`]). Learned `f` comes in RL phase.
pub trait LatentUpdate {
    fn update(&self, z: &PolicyState, obs: &PrivilegedObs, action: &TrainAction) -> PolicyState;
}

/// Identity residual (`f = 0`): leaves `z` unchanged.
pub struct IdentityLatent;

impl LatentUpdate for IdentityLatent {
    fn update(&self, z: &PolicyState, _: &PrivilegedObs, _: &TrainAction) -> PolicyState {
        z.clone()
    }
}

/// Apply residual delta: `z' = normalize(z + f)` (elementwise, then L2 normalize).
///
/// `f` is truncated or zero-padded to [`LATENT_DIM`]. Near-zero norms stay zero.
#[allow(dead_code)] // unit-tested; used by learned residual later
pub fn residual_apply(z: &PolicyState, f: &[f32]) -> PolicyState {
    let mut out = vec![0.0; LATENT_DIM];
    for i in 0..LATENT_DIM {
        let zi = z.z.get(i).copied().unwrap_or(0.0);
        let fi = f.get(i).copied().unwrap_or(0.0);
        out[i] = zi + fi;
    }
    l2_normalize_in_place(&mut out);
    PolicyState { z: out }
}

fn l2_normalize_in_place(v: &mut [f32]) {
    let mut ss = 0.0f32;
    for x in v.iter() {
        ss += *x * *x;
    }
    let n = ss.sqrt();
    if n < 1e-8 {
        for x in v.iter_mut() {
            *x = 0.0;
        }
        return;
    }
    for x in v.iter_mut() {
        *x /= n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::train::obs::PrivilegedObs;

    #[test]
    fn privileged_obs_vec_has_no_latent_slots() {
        let obs = PrivilegedObs::default();
        let v = obs.as_vec();
        // Document exact length; must not grow when LATENT_DIM changes.
        assert_eq!(v.len(), crate::train::obs::OBS_DIM);
        // Sanity: latent dim is independent of env export width.
        assert_ne!(LATENT_DIM, crate::train::obs::OBS_DIM);
        assert_eq!(v.len() + LATENT_DIM, crate::train::obs::OBS_DIM + LATENT_DIM);
    }

    #[test]
    fn residual_add_changes_z() {
        let z = PolicyState::zeros();
        let f = vec![0.1; LATENT_DIM];
        let z2 = residual_apply(&z, &f);
        assert!(z2.z.iter().any(|x| *x != 0.0));
        assert_eq!(z2.z.len(), LATENT_DIM);
    }

    #[test]
    fn identity_latent_leaves_z() {
        let z = residual_apply(&PolicyState::zeros(), &[0.25; LATENT_DIM]);
        let action = TrainAction::default();
        let obs = PrivilegedObs::default();
        let z2 = IdentityLatent.update(&z, &obs, &action);
        assert_eq!(z2.z, z.z);
    }

    #[test]
    fn zeros_has_latent_dim() {
        let z = PolicyState::zeros();
        assert_eq!(z.z.len(), LATENT_DIM);
        assert!(z.z.iter().all(|x| *x == 0.0));
    }
}
