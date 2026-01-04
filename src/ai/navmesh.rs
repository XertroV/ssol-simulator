use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use vleue_navigator::prelude::*;

use crate::game_state::OrbParent;
use crate::player::Player;

use super::observations::{AiObservations, OrbId};

/// Resource tracking the state of the navigation mesh
#[derive(Resource, Default)]
pub struct NavMeshState {
    /// True when navmesh is built and ready for pathfinding
    pub ready: bool,
    /// Handle to the navmesh asset
    pub handle: Option<Handle<NavMesh>>,
    /// Whether we've attempted to build the navmesh
    pub build_attempted: bool,
}

/// Component attached to each orb for navigation data
#[derive(Component, Default)]
pub struct OrbNavData {
    /// Path distance to player along the navmesh
    pub path_distance: f32,
    /// Direction to the next waypoint (in player local space)
    pub direction_to_waypoint: Vec3,
    /// Whether this orb needs path recalculation
    pub dirty: bool,
}

/// Resource tracking player position for path invalidation
#[derive(Resource)]
pub struct PlayerNavState {
    /// Last position when paths were calculated
    pub last_position: Vec3,
    /// Distance threshold to invalidate all paths
    pub invalidation_distance: f32,
}

impl Default for PlayerNavState {
    fn default() -> Self {
        Self {
            last_position: Vec3::ZERO,
            invalidation_distance: 2.0,
        }
    }
}

/// Marker component for the maptile mesh (floor)
#[derive(Component)]
pub struct MapTileMesh;

/// Plugin for AI navigation mesh functionality
pub struct AiNavMeshPlugin;

impl Plugin for AiNavMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavMeshState>()
            .init_resource::<PlayerNavState>()
            .add_plugins(VleueNavigatorPlugin)
            .add_systems(
                Update,
                (
                    detect_maptile_mesh,
                    build_navmesh_from_maptile,
                    add_orb_nav_data_component,
                    invalidate_paths_on_player_move,
                    update_orb_paths_staggered,
                    populate_orb_targets_observation,
                ).chain(),
            );
    }
}

/// System to detect the maptile mesh after scene load
fn detect_maptile_mesh(
    mut commands: Commands,
    query: Query<(Entity, &Name), (With<Mesh3d>, Without<MapTileMesh>)>,
    navmesh_state: Res<NavMeshState>,
) {
    // Skip if navmesh already built or attempted
    if navmesh_state.build_attempted {
        return;
    }

    for (entity, name) in query.iter() {
        if name.as_str().contains("maptile") || name.as_str().contains("MapTile") {
            commands.entity(entity).insert(MapTileMesh);
            info!("Detected maptile mesh: {:?}", name);
        }
    }
}

/// System to build navmesh from the maptile mesh
fn build_navmesh_from_maptile(
    mut navmesh_state: ResMut<NavMeshState>,
    maptile_query: Query<&Mesh3d, With<MapTileMesh>>,
    meshes: Res<Assets<Mesh>>,
    mut navmeshes: ResMut<Assets<NavMesh>>,
    colliders: Query<(&GlobalTransform, &Collider), (Without<Sensor>, Without<OrbParent>)>,
) {
    // Skip if already built or attempted
    if navmesh_state.build_attempted {
        return;
    }

    // Try to get the maptile mesh
    let Ok(mesh_handle) = maptile_query.single() else {
        return;
    };

    let Some(mesh) = meshes.get(&mesh_handle.0) else {
        return;
    };

    navmesh_state.build_attempted = true;

    // Collect obstacle polygons from box colliders (XZ plane)
    let obstacles: Vec<Vec<Vec2>> = colliders
        .iter()
        .filter_map(|(global_transform, collider)| {
            // Try to get box collider half-extents
            if let Some(cuboid) = collider.as_cuboid() {
                let half_extents = cuboid.half_extents();
                let transform = global_transform.compute_transform();
                let pos = transform.translation;
                let rotation = transform.rotation;

                // Expand obstacles slightly for safer paths
                let margin = 0.3;
                let hx = half_extents.x + margin;
                let hz = half_extents.z + margin;

                // Create 2D polygon corners in local space (XZ plane)
                let corners = [
                    Vec3::new(-hx, 0.0, -hz),
                    Vec3::new(hx, 0.0, -hz),
                    Vec3::new(hx, 0.0, hz),
                    Vec3::new(-hx, 0.0, hz),
                ];

                // Transform to world space and project to XZ
                let world_corners: Vec<Vec2> = corners
                    .iter()
                    .map(|&corner| {
                        let world_pos = pos + rotation * corner;
                        Vec2::new(world_pos.x, world_pos.z)
                    })
                    .collect();

                Some(world_corners)
            } else {
                None
            }
        })
        .collect();

    info!("Building navmesh with {} obstacles from box colliders", obstacles.len());

    // Try to build NavMesh from mesh directly (returns Option<NavMesh>)
    if let Some(navmesh) = NavMesh::from_bevy_mesh(mesh) {
        let handle = navmeshes.add(navmesh);
        navmesh_state.handle = Some(handle);
        navmesh_state.ready = true;
        info!("NavMesh built successfully from maptile mesh");
    } else {
        info!("Could not build NavMesh from maptile mesh, using boundary fallback");

        // Fallback: use a large bounding box based on known level extents
        // The level-zero map is roughly -100 to +100 in XZ plane
        let boundary = vec![
            Vec2::new(-150.0, -150.0),
            Vec2::new(150.0, -150.0),
            Vec2::new(150.0, 150.0),
            Vec2::new(-150.0, 150.0),
        ];

        // Use from_edge_and_obstacles which returns NavMesh directly
        let navmesh = NavMesh::from_edge_and_obstacles(boundary, obstacles.clone());
        let handle = navmeshes.add(navmesh);
        navmesh_state.handle = Some(handle);
        navmesh_state.ready = true;
        info!("NavMesh built from boundary with {} obstacles", obstacles.len());
    }
}

/// System to add OrbNavData component to orbs that don't have it
fn add_orb_nav_data_component(
    mut commands: Commands,
    orbs_without_nav: Query<Entity, (With<OrbParent>, Without<OrbNavData>)>,
) {
    for entity in orbs_without_nav.iter() {
        commands.entity(entity).insert(OrbNavData {
            path_distance: f32::MAX,
            direction_to_waypoint: Vec3::ZERO,
            dirty: true,
        });
    }
}

/// System to invalidate all orb paths when player moves beyond threshold
fn invalidate_paths_on_player_move(
    mut player_nav_state: ResMut<PlayerNavState>,
    player_query: Query<&Transform, With<Player>>,
    mut orb_query: Query<&mut OrbNavData>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_pos = player_transform.translation;
    let distance_moved = player_pos.distance(player_nav_state.last_position);

    if distance_moved > player_nav_state.invalidation_distance {
        // Mark all orb paths as dirty
        for mut nav_data in orb_query.iter_mut() {
            nav_data.dirty = true;
        }
        player_nav_state.last_position = player_pos;
    }
}

/// System to update orb paths in a staggered manner (10-15 per frame)
fn update_orb_paths_staggered(
    navmesh_state: Res<NavMeshState>,
    navmeshes: Res<Assets<NavMesh>>,
    player_query: Query<&GlobalTransform, With<Player>>,
    mut orb_query: Query<(&GlobalTransform, &mut OrbNavData), With<OrbParent>>,
) {
    // Skip if navmesh not ready
    if !navmesh_state.ready {
        return;
    }

    let Some(navmesh_handle) = &navmesh_state.handle else {
        return;
    };

    let Some(navmesh) = navmeshes.get(navmesh_handle) else {
        return;
    };

    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_pos = player_transform.translation();
    let player_pos_2d = Vec2::new(player_pos.x, player_pos.z);

    let mut updated_count = 0;
    const MAX_UPDATES_PER_FRAME: usize = 15;

    for (orb_transform, mut nav_data) in orb_query.iter_mut() {
        if !nav_data.dirty {
            continue;
        }

        if updated_count >= MAX_UPDATES_PER_FRAME {
            break;
        }

        let orb_pos = orb_transform.translation();
        let orb_pos_2d = Vec2::new(orb_pos.x, orb_pos.z);

        // Find path from player to orb
        if let Some(path) = navmesh.path(player_pos_2d, orb_pos_2d) {
            // Calculate total path distance
            let waypoints = &path.path;
            let mut total_distance = 0.0;

            if !waypoints.is_empty() {
                let mut prev = player_pos_2d;
                for waypoint in waypoints.iter() {
                    total_distance += prev.distance(*waypoint);
                    prev = *waypoint;
                }
            }

            nav_data.path_distance = total_distance;

            // Get direction to first waypoint (or directly to orb)
            let next_waypoint_2d = waypoints.first().copied().unwrap_or(orb_pos_2d);
            let next_waypoint_3d = Vec3::new(next_waypoint_2d.x, orb_pos.y, next_waypoint_2d.y);
            let direction = (next_waypoint_3d - player_pos).normalize_or_zero();
            nav_data.direction_to_waypoint = direction;
        } else {
            // No path found, use direct line
            nav_data.path_distance = orb_pos.distance(player_pos);
            nav_data.direction_to_waypoint = (orb_pos - player_pos).normalize_or_zero();
        }

        nav_data.dirty = false;
        updated_count += 1;
    }
}

/// System that populates AiObservations.orb_targets using simple Euclidean distance
/// This bypasses the navmesh and calculates direct distances to orbs
pub fn populate_orb_targets_observation(
    mut observations: ResMut<AiObservations>,
    player_query: Query<&GlobalTransform, With<Player>>,
    orb_query: Query<(&GlobalTransform, &OrbId, &Visibility), With<OrbParent>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_pos = player_transform.translation();

    // Collect all visible orbs with direct distance calculation
    let mut orbs: Vec<(f32, Vec3, u8)> = orb_query
        .iter()
        .filter(|(_, _, vis)| *vis != Visibility::Hidden)
        .map(|(orb_transform, orb_id, _)| {
            let orb_pos = orb_transform.translation();
            let distance = player_pos.distance(orb_pos);
            let direction = (orb_pos - player_pos).normalize_or_zero();
            (distance, direction, orb_id.0)
        })
        .collect();

    // Sort by distance (nearest first)
    orbs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Get player's inverse rotation to transform directions to local space
    let player_rotation = player_transform.to_scale_rotation_translation().1;
    let player_rotation_inv = player_rotation.inverse();

    // Populate up to 10 nearest orbs
    for (i, (distance, world_direction, orb_id)) in orbs.iter().take(10).enumerate() {
        // Transform direction from world space to player local space
        let local_direction = player_rotation_inv * *world_direction;

        observations.orb_targets[i] = (
            local_direction,
            *distance,
            *orb_id as f32,
        );
    }

    // Mark remaining slots as empty with -1.0
    for i in orbs.len().min(10)..10 {
        observations.orb_targets[i] = (Vec3::ZERO, 0.0, -1.0);
    }
}
