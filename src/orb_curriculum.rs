//! Orb-related components and utilities
//!
//! This module centralizes orb ID management and curriculum-based orb filtering.

use bevy::{ecs::entity_disabling::Disabled, prelude::*};

use crate::curriculum::CurriculumConfig;

/// Component to identify orbs by a numeric ID (0-99).
/// OrbId 0 is always the closest orb to the player spawn point.
/// This is added to OrbParent entities during scene loading.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrbId(pub u8);


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
