//! Orb-related components and utilities
//!
//! This module centralizes orb ID management and curriculum-based orb filtering.
//!
//! ## Random-spawn nearest curriculum
//!
//! Pick a random orb as the spawn center, keep that orb plus the **N** nearest
//! other orbs → **N+1** active orbs total. Pure selection is in
//! [`select_nearest_n_plus_spawn`]; wire via CLI later.

use bevy::{ecs::entity_disabling::Disabled, prelude::*};

use crate::curriculum::CurriculumConfig;

/// Component to identify orbs by a numeric ID (0-99).
/// OrbId 0 is always the closest orb to the player spawn point.
/// This is added to OrbParent entities during scene loading.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrbId(pub u8);

/// Result of random-spawn + nearest-N selection (pure, no ECS).
#[derive(Clone, Debug, PartialEq)]
pub struct NearestSpawnSelection {
    /// Index into the input orb list used as spawn center.
    pub spawn_index: usize,
    /// World position of the spawn center orb (player spawn target).
    pub spawn_position: Vec3,
    /// Indices of active orbs (includes spawn center). Length = N+1 when enough orbs exist.
    pub active_indices: Vec<usize>,
}

/// Select spawn orb + N nearest others → N+1 active orbs.
///
/// - `orbs`: (id, world position) for every map orb
/// - `n_nearest`: number of *additional* orbs beyond the spawn center
/// - `spawn_index`: which orb is the spawn center (caller picks randomly)
///
/// Returns `None` if `orbs` is empty or `spawn_index` is out of range.
pub fn select_nearest_n_plus_spawn(
    orbs: &[(u8, Vec3)],
    n_nearest: usize,
    spawn_index: usize,
) -> Option<NearestSpawnSelection> {
    if orbs.is_empty() || spawn_index >= orbs.len() {
        return None;
    }
    let spawn_position = orbs[spawn_index].1;
    let mut others: Vec<(usize, f32)> = orbs
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != spawn_index)
        .map(|(i, (_, p))| (i, spawn_position.distance(*p)))
        .collect();
    others.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let take = n_nearest.min(others.len());
    let mut active_indices = Vec::with_capacity(1 + take);
    active_indices.push(spawn_index);
    for (i, _) in others.into_iter().take(take) {
        active_indices.push(i);
    }
    // Stable order by distance from spawn for deterministic OrbId reassignment if needed
    active_indices[1..].sort_by(|&a, &b| {
        let da = spawn_position.distance(orbs[a].1);
        let db = spawn_position.distance(orbs[b].1);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    Some(NearestSpawnSelection {
        spawn_index,
        spawn_position,
        active_indices,
    })
}

/// Expected total orbs for nearest-N curriculum: N+1 (spawn + N neighbors), capped by map size.
#[allow(dead_code)] // used by tests + Python/eval helpers; kept public for curriculum docs
pub fn nearest_curriculum_orb_count(map_orbs: usize, n_nearest: usize) -> usize {
    if map_orbs == 0 {
        return 0;
    }
    (1 + n_nearest).min(map_orbs)
}


/// Result of applying curriculum constraints to orbs
#[derive(Debug)]
#[allow(dead_code)]
pub struct CurriculumApplicationResult {
    /// Number of orbs that are active after applying constraints
    pub active_count: u32,
}

/// Sorts orb data by distance from player spawn position.
/// Returns indices in sorted order (closest first).
#[allow(dead_code)]
pub fn sort_orbs_by_distance<T>(
    orbs: &[(T, Vec3)],
    player_spawn: Vec3,
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..orbs.len()).collect();
    indices.sort_by(|&a, &b| {
        let dist_a = player_spawn.distance(orbs[a].1);
        let dist_b = player_spawn.distance(orbs[b].1);
        dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
    });
    indices
}

/// Determines if an orb should be active based on curriculum constraints.
/// 
/// # Arguments
/// * `orb_position` - World position of the orb
/// * `current_active_count` - How many orbs have already been marked active
/// * `curriculum` - The curriculum configuration
/// 
/// # Returns
/// `true` if the orb should be active, `false` if it should be disabled
pub fn should_orb_be_active(
    orb_position: Vec3,
    current_active_count: u32,
    curriculum: &CurriculumConfig,
) -> bool {
    let max_orbs = curriculum.max_orbs.unwrap_or(u32::MAX);
    let within_radius = curriculum.should_spawn_orb(orb_position);
    let within_limit = current_active_count < max_orbs;
    within_radius && within_limit
}

/// Applies curriculum constraints to a collection of orbs, returning which should be active.
/// 
/// Orbs should be pre-sorted by OrbId (which corresponds to distance from spawn).
/// 
/// # Arguments
/// * `orb_positions` - Iterator of (entity/id, position, is_currently_disabled) tuples, sorted by OrbId
/// * `curriculum` - The curriculum configuration
/// 
/// # Returns
/// Vector of (entity/id, should_be_active) tuples and the total active count
#[allow(dead_code)]
pub fn apply_curriculum_to_orbs<T: Copy>(
    orbs: impl IntoIterator<Item = (T, Vec3, bool)>,
    curriculum: &CurriculumConfig,
) -> (Vec<(T, bool)>, u32) {
    let max_orbs = curriculum.max_orbs.unwrap_or(u32::MAX);
    let mut active_count = 0u32;
    let mut results = Vec::new();

    for (id, position, _is_disabled) in orbs {
        let within_radius = curriculum.should_spawn_orb(position);
        let within_limit = active_count < max_orbs;
        let should_be_active = within_radius && within_limit;

        if should_be_active {
            active_count += 1;
        }
        results.push((id, should_be_active));
    }

    (results, active_count)
}

/// System helper to apply curriculum to spawned orbs.
/// Call this after orbs have been spawned or when curriculum changes.
pub fn apply_curriculum_to_spawned_orbs(
    commands: &mut Commands,
    orbs: &[(Entity, OrbId, Vec3, bool)], // (entity, orb_id, position, is_currently_disabled)
    curriculum: &mut CurriculumConfig,
) -> u32 {
    // Orbs should already be sorted by OrbId (which is assigned by distance)
    let mut sorted_orbs = orbs.to_vec();
    sorted_orbs.sort_by_key(|(_, id, _, _)| id.0);

    let max_orbs = curriculum.max_orbs.unwrap_or(u32::MAX);
    let mut active_count = 0u32;

    for (entity, _orb_id, position, is_disabled) in sorted_orbs {
        let within_radius = curriculum.should_spawn_orb(position);
        let within_limit = active_count < max_orbs;
        let should_be_active = within_radius && within_limit;

        if should_be_active {
            active_count += 1;
            if is_disabled {
                commands.entity(entity).remove::<Disabled>();
            }
            commands.entity(entity).insert(Visibility::Visible);
        } else {
            if !is_disabled {
                commands.entity(entity).insert(Disabled);
            }
            commands.entity(entity).insert(Visibility::Hidden);
        }
    }

    curriculum.active_orb_count = active_count;
    active_count
}

/// Collects orb data from a query into a sortable format.
pub fn collect_orb_data<'a>(
    query: impl IntoIterator<Item = (Entity, &'a OrbId, &'a GlobalTransform, bool)>,
) -> Vec<(Entity, OrbId, Vec3, bool)> {
    query
        .into_iter()
        .map(|(entity, orb_id, transform, is_disabled)| {
            (entity, *orb_id, transform.translation(), is_disabled)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_orbs() -> Vec<(u8, Vec3)> {
        // ids 0..4 on a line at x=0,10,20,30,100
        vec![
            (0, Vec3::new(0.0, 0.0, 0.0)),
            (1, Vec3::new(10.0, 0.0, 0.0)),
            (2, Vec3::new(20.0, 0.0, 0.0)),
            (3, Vec3::new(30.0, 0.0, 0.0)),
            (4, Vec3::new(100.0, 0.0, 0.0)),
        ]
    }

    #[test]
    fn nearest_n_plus_spawn_count_is_n_plus_one() {
        let orbs = sample_orbs();
        let sel = select_nearest_n_plus_spawn(&orbs, 6, 2).unwrap();
        // map only has 5 orbs → cap at 5, not 7
        assert_eq!(sel.active_indices.len(), 5);
        assert_eq!(nearest_curriculum_orb_count(orbs.len(), 6), 5);

        let sel2 = select_nearest_n_plus_spawn(&orbs, 2, 0).unwrap();
        assert_eq!(sel2.active_indices.len(), 3); // spawn + 2
        assert_eq!(nearest_curriculum_orb_count(orbs.len(), 2), 3);
        assert!(sel2.active_indices.contains(&0));
        // nearest to 0 are 1 then 2
        assert_eq!(sel2.active_indices[0], 0);
        assert_eq!(&sel2.active_indices[1..], &[1, 2]);
    }

    #[test]
    fn spawn_center_included_and_position_matches() {
        let orbs = sample_orbs();
        let sel = select_nearest_n_plus_spawn(&orbs, 2, 4).unwrap();
        assert_eq!(sel.spawn_index, 4);
        assert_eq!(sel.spawn_position, orbs[4].1);
        assert!(sel.active_indices.contains(&4));
        // nearest to 100 are 30 then 20
        assert_eq!(sel.active_indices.len(), 3);
        assert_eq!(sel.active_indices[1], 3);
        assert_eq!(sel.active_indices[2], 2);
    }

    #[test]
    fn empty_or_oob_returns_none() {
        assert!(select_nearest_n_plus_spawn(&[], 1, 0).is_none());
        let orbs = sample_orbs();
        assert!(select_nearest_n_plus_spawn(&orbs, 1, 99).is_none());
    }
}
