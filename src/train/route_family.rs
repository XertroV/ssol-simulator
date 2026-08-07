//! Multi-route family sampler for train generalization.
//!
//! Episode samples a route mode (WR, greedy, noisy WR, random NN, reverse WR,
//! or a weighted mix) so low-level control is goal-conditioned and cannot
//! overfit a single open-loop tour.

use std::collections::HashSet;
use std::str::FromStr;

use bevy::prelude::*;
use rand::Rng;

use super::route::{RouteStop, WrRoute};

/// High-level route construction mode for an episode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum RouteMode {
    Wr,
    Greedy,
    WrNoisy,
    RandomNn,
    ReverseWr,
    /// Weighted mixture of the above (not a leaf construction).
    #[default]
    Mix,
}

impl RouteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wr => "wr",
            Self::Greedy => "greedy",
            Self::WrNoisy => "wr_noisy",
            Self::RandomNn => "random_nn",
            Self::ReverseWr => "reverse_wr",
            Self::Mix => "mix",
        }
    }

    /// True if this mode is a concrete tour (not Mix).
    #[cfg_attr(not(test), allow(dead_code))] // used in unit tests + public API
    pub fn is_leaf(self) -> bool {
        !matches!(self, Self::Mix)
    }
}

impl FromStr for RouteMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wr" => Ok(Self::Wr),
            "greedy" => Ok(Self::Greedy),
            "wr_noisy" | "wr-noisy" | "noisy" => Ok(Self::WrNoisy),
            "random_nn" | "random-nn" | "nn" => Ok(Self::RandomNn),
            "reverse_wr" | "reverse-wr" | "reverse" => Ok(Self::ReverseWr),
            "mix" => Ok(Self::Mix),
            other => Err(format!(
                "unknown route mode '{other}' (expected wr|greedy|wr_noisy|random_nn|reverse_wr|mix)"
            )),
        }
    }
}

impl std::fmt::Display for RouteMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Mix weights from Global Constraints (train distribution).
/// Sums to 0.95; residual mass is unused (eval_wr is eval-only).
pub const MIX_WEIGHTS: &[(RouteMode, f32)] = &[
    (RouteMode::Wr, 0.25),
    (RouteMode::Greedy, 0.25),
    (RouteMode::WrNoisy, 0.20),
    (RouteMode::RandomNn, 0.15),
    (RouteMode::ReverseWr, 0.10),
];

/// Resolved route for the current episode.
#[derive(Clone, Debug, Resource)]
pub struct ActiveRoute {
    /// Concrete mode after Mix sampling (never `Mix`).
    pub mode: RouteMode,
    /// Static tour order; empty when `dynamic_greedy`.
    pub stops: Vec<RouteStop>,
    /// When true, `next_target` recomputes nearest remaining each call.
    pub dynamic_greedy: bool,
}

impl Default for ActiveRoute {
    fn default() -> Self {
        Self {
            mode: RouteMode::Wr,
            stops: Vec::new(),
            dynamic_greedy: false,
        }
    }
}

impl ActiveRoute {
    /// Next target orb for the low-level go-to controller.
    ///
    /// `remaining_positions` should list live uncollected active orb positions
    /// `(orb_id, world_pos)`. Static modes prefer live positions over stop coords.
    pub fn next_target(
        &self,
        player: Vec3,
        collected: &HashSet<u8>,
        active: &HashSet<u8>,
        remaining_positions: &[(u8, Vec3)],
    ) -> Option<(u8, Vec3)> {
        if self.dynamic_greedy {
            return greedy_next(player, collected, remaining_positions);
        }

        for stop in &self.stops {
            if collected.contains(&stop.orb_id) {
                continue;
            }
            if !active.contains(&stop.orb_id) {
                continue;
            }
            let pos = remaining_positions
                .iter()
                .find(|(id, _)| *id == stop.orb_id)
                .map(|(_, p)| *p)
                .unwrap_or_else(|| stop.position());
            return Some((stop.orb_id, pos));
        }
        None
    }
}

/// Nearest uncollected orb among `active` positions (euclidean).
pub fn greedy_next(
    player: Vec3,
    collected: &HashSet<u8>,
    active: &[(u8, Vec3)],
) -> Option<(u8, Vec3)> {
    active
        .iter()
        .filter(|(id, _)| !collected.contains(id))
        .min_by(|a, b| {
            let da = player.distance_squared(a.1);
            let db = player.distance_squared(b.1);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// Reverse WR order, optionally filtered to an active curriculum set.
pub fn build_reverse_wr(wr: &WrRoute, active: Option<&HashSet<u8>>) -> ActiveRoute {
    let mut stops: Vec<RouteStop> = wr
        .stops
        .iter()
        .filter(|s| active.map(|a| a.contains(&s.orb_id)).unwrap_or(true))
        .cloned()
        .collect();
    stops.reverse();
    ActiveRoute {
        mode: RouteMode::ReverseWr,
        stops,
        dynamic_greedy: false,
    }
}

/// WR order filtered to active orbs (preserves relative order).
pub fn build_wr(wr: &WrRoute, active_ids: &HashSet<u8>) -> ActiveRoute {
    let stops: Vec<RouteStop> = wr
        .stops
        .iter()
        .filter(|s| active_ids.contains(&s.orb_id))
        .cloned()
        .collect();
    ActiveRoute {
        mode: RouteMode::Wr,
        stops,
        dynamic_greedy: false,
    }
}

/// WR with `k ~ Uniform(1, 5)` random adjacent swaps on the filtered order.
pub fn build_wr_noisy(
    wr: &WrRoute,
    active_ids: &HashSet<u8>,
    rng: &mut impl Rng,
) -> ActiveRoute {
    let mut stops: Vec<RouteStop> = wr
        .stops
        .iter()
        .filter(|s| active_ids.contains(&s.orb_id))
        .cloned()
        .collect();
    if stops.len() >= 2 {
        let k = rng.gen_range(1..=5);
        for _ in 0..k {
            let i = rng.gen_range(0..stops.len() - 1);
            stops.swap(i, i + 1);
        }
    }
    ActiveRoute {
        mode: RouteMode::WrNoisy,
        stops,
        dynamic_greedy: false,
    }
}

/// Random start among active, then classic nearest-neighbor tour.
pub fn build_random_nn(active_orbs: &[(u8, Vec3)], rng: &mut impl Rng) -> ActiveRoute {
    if active_orbs.is_empty() {
        return ActiveRoute {
            mode: RouteMode::RandomNn,
            stops: Vec::new(),
            dynamic_greedy: false,
        };
    }

    let mut remaining: Vec<(u8, Vec3)> = active_orbs.to_vec();
    let start_idx = rng.gen_range(0..remaining.len());
    let (start_id, start_pos) = remaining.swap_remove(start_idx);

    let mut tour: Vec<RouteStop> = Vec::with_capacity(active_orbs.len());
    tour.push(RouteStop {
        seq: 0,
        orb_id: start_id,
        x: start_pos.x,
        y: start_pos.y,
        z: start_pos.z,
        phase: String::new(),
    });

    let mut cursor = start_pos;
    while !remaining.is_empty() {
        let (best_i, _) = remaining
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = cursor.distance_squared(a.1);
                let db = cursor.distance_squared(b.1);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        let (id, pos) = remaining.swap_remove(best_i);
        tour.push(RouteStop {
            seq: tour.len() as u32,
            orb_id: id,
            x: pos.x,
            y: pos.y,
            z: pos.z,
            phase: String::new(),
        });
        cursor = pos;
    }

    ActiveRoute {
        mode: RouteMode::RandomNn,
        stops: tour,
        dynamic_greedy: false,
    }
}

fn active_id_set(active_orbs: &[(u8, Vec3)]) -> HashSet<u8> {
    active_orbs.iter().map(|(id, _)| *id).collect()
}

/// Sample a concrete `ActiveRoute` for the episode.
///
/// `Mix` draws a leaf mode using [`MIX_WEIGHTS`]. Leaf modes never return
/// `mode == Mix`.
pub fn sample_route(
    mode: RouteMode,
    wr: &WrRoute,
    active_orbs: &[(u8, Vec3)],
    rng: &mut impl Rng,
) -> ActiveRoute {
    let leaf = if mode == RouteMode::Mix {
        sample_mix_mode(rng)
    } else {
        mode
    };
    let ids = active_id_set(active_orbs);
    match leaf {
        RouteMode::Wr => build_wr(wr, &ids),
        RouteMode::Greedy => ActiveRoute {
            mode: RouteMode::Greedy,
            stops: Vec::new(),
            dynamic_greedy: true,
        },
        RouteMode::WrNoisy => build_wr_noisy(wr, &ids, rng),
        RouteMode::RandomNn => build_random_nn(active_orbs, rng),
        RouteMode::ReverseWr => build_reverse_wr(wr, Some(&ids)),
        RouteMode::Mix => unreachable!("Mix resolved to leaf above"),
    }
}

fn sample_mix_mode(rng: &mut impl Rng) -> RouteMode {
    let total: f32 = MIX_WEIGHTS.iter().map(|(_, w)| *w).sum();
    // `gen` is reserved in Rust 2024; use gen_range for unit interval.
    let mut r = rng.gen_range(0.0f32..1.0) * total;
    for &(mode, w) in MIX_WEIGHTS {
        if r < w {
            return mode;
        }
        r -= w;
    }
    MIX_WEIGHTS.last().map(|(m, _)| *m).unwrap_or(RouteMode::Wr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    const MINI_ROUTE: &str = r#"{
        "last_orb_id": 2,
        "order": [
            {"seq": 0, "orb_id": 4, "x": 1.0, "y": 0.0, "z": 2.0, "phase": "blue_path"},
            {"seq": 1, "orb_id": 1, "x": 3.0, "y": 0.0, "z": 4.0, "phase": "blue_path"},
            {"seq": 2, "orb_id": 2, "x": 5.0, "y": 0.0, "z": 6.0, "phase": "straight_shot"}
        ]
    }"#;

    #[test]
    fn greedy_picks_nearest_uncollected() {
        let active = vec![
            (1, Vec3::new(10.0, 0.0, 0.0)),
            (2, Vec3::new(3.0, 0.0, 0.0)),
        ];
        let collected = HashSet::new();
        let player = Vec3::ZERO;
        let (id, _) = greedy_next(player, &collected, &active).unwrap();
        assert_eq!(id, 2);
    }

    #[test]
    fn reverse_wr_order_is_reversed_ids() {
        let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let r = build_reverse_wr(&wr, None);
        let ids: Vec<_> = r.stops.iter().map(|s| s.orb_id).collect();
        assert_eq!(ids, vec![2, 1, 4]);
    }

    #[test]
    fn greedy_skips_collected() {
        let active = vec![
            (1, Vec3::new(10.0, 0.0, 0.0)),
            (2, Vec3::new(3.0, 0.0, 0.0)),
        ];
        let collected: HashSet<u8> = [2].into_iter().collect();
        let (id, _) = greedy_next(Vec3::ZERO, &collected, &active).unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn wr_filters_to_active() {
        let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let active: HashSet<u8> = [1u8, 2].into_iter().collect();
        let r = build_wr(&wr, &active);
        let ids: Vec<_> = r.stops.iter().map(|s| s.orb_id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(!r.dynamic_greedy);
        assert_eq!(r.mode, RouteMode::Wr);
    }

    #[test]
    fn sample_greedy_is_dynamic() {
        let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let active = vec![(1, Vec3::ZERO), (2, Vec3::X)];
        let mut rng = StdRng::seed_from_u64(0);
        let r = sample_route(RouteMode::Greedy, &wr, &active, &mut rng);
        assert!(r.dynamic_greedy);
        assert_eq!(r.mode, RouteMode::Greedy);
    }

    #[test]
    fn sample_mix_never_returns_mix_mode() {
        let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let active = vec![
            (4, Vec3::new(1.0, 0.0, 2.0)),
            (1, Vec3::new(3.0, 0.0, 4.0)),
            (2, Vec3::new(5.0, 0.0, 6.0)),
        ];
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..32 {
            let r = sample_route(RouteMode::Mix, &wr, &active, &mut rng);
            assert!(r.mode.is_leaf(), "got {:?}", r.mode);
            assert_ne!(r.mode, RouteMode::Mix);
        }
    }

    #[test]
    fn random_nn_visits_all_active() {
        let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let active = vec![
            (4, Vec3::new(0.0, 0.0, 0.0)),
            (1, Vec3::new(10.0, 0.0, 0.0)),
            (2, Vec3::new(11.0, 0.0, 0.0)),
        ];
        let mut rng = StdRng::seed_from_u64(7);
        let r = sample_route(RouteMode::RandomNn, &wr, &active, &mut rng);
        let mut ids: Vec<u8> = r.stops.iter().map(|s| s.orb_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 4]);
        assert_eq!(r.stops.len(), 3);
    }

    #[test]
    fn active_route_static_next_target_uses_live_pos() {
        let wr = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let active_ids: HashSet<u8> = [4u8, 1, 2].into_iter().collect();
        let r = build_wr(&wr, &active_ids);
        let remaining = vec![(4, Vec3::new(99.0, 0.0, 0.0))];
        let (id, pos) = r
            .next_target(Vec3::ZERO, &HashSet::new(), &active_ids, &remaining)
            .unwrap();
        assert_eq!(id, 4);
        assert!((pos.x - 99.0).abs() < 1e-5);
    }

    #[test]
    fn route_mode_parses_cli_aliases() {
        assert_eq!("wr".parse::<RouteMode>().unwrap(), RouteMode::Wr);
        assert_eq!("greedy".parse::<RouteMode>().unwrap(), RouteMode::Greedy);
        assert_eq!("wr-noisy".parse::<RouteMode>().unwrap(), RouteMode::WrNoisy);
        assert_eq!("mix".parse::<RouteMode>().unwrap(), RouteMode::Mix);
        assert!("nope".parse::<RouteMode>().is_err());
    }
}
