use bevy::{
    camera::visibility::{InheritedVisibility, ViewVisibility},
    prelude::*,
};
use bevy_rapier3d::prelude::*;

use crate::{
    game_state::GameState,
    player::{Player, PlayerRespawnRequest},
    relativity::rel_material::{
        NeedsRelativisticMaterial, RelativisticMaterial, RelativisticObject,
    },
};

// -- Collision group definitions --
// Static world geometry and sensors use WORLD_GROUP.
// Villagers and receiver triggers use VILLAGER_GROUP.
// The player belongs to both so it can interact with everything.
pub const WORLD_GROUP: Group = Group::GROUP_1;
pub const VILLAGER_GROUP: Group = Group::GROUP_2;

pub struct VillagerPlugin;

impl Plugin for VillagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup_villager_spawners.after(crate::scene_loader::setup_scene),
        )
        .add_systems(
            FixedUpdate,
            (
                villager_spawn_system,
                villager_velocity_update,
                villager_receiver_trigger_system,
                villager_relativistic_despawn,
            )
                .chain()
                .after(PhysicsSet::Writeback),
        )
        .add_systems(
            Update,
            (
                setup_villager_materials,
                update_villager_materials
                    .after(crate::relativity::rel_material::update_relativistic_materials),
            ),
        )
        .add_observer(reset_villagers);
    }
}

// -- Components --

#[derive(Component)]
pub struct VillagerSender;

#[derive(Component)]
pub struct VillagerReceiver;

#[derive(Component)]
pub struct VillagerSpawner {
    launch_timer_secs: f32,
    counter_secs: f32,
    viw_max: f32,
    direction: Vec3,
    receiver_pos: Vec3,
}

#[derive(Component)]
pub struct Villager;

#[derive(Component)]
pub struct VillagerMovement {
    pub viw: Vec3,
    pub start_time: f32,
    pub death_time: Option<f32>,
    pub receiver_pos: Vec3,
}

/// Marker: this villager has had its relativistic material cloned for per-instance rendering.
#[derive(Component)]
struct VillagerMaterialReady;

// -- Hardcoded sender/receiver pairing data (from LevelZero.unity) --
// Positions are in Unity scene space; Z is negated by `unity_to_bevy` for Bevy.

struct PairingData {
    sender_pos: [f32; 3],
    receiver_pos: [f32; 3],
    launch_timer: f32,
    viw_max: f32,
}

const PAIRINGS: &[PairingData] = &[
    PairingData { sender_pos: [135.2, -4.4, -94.3],  receiver_pos: [242.2, -4.4, -84.1],   launch_timer: 25.0, viw_max: 3.0 },
    PairingData { sender_pos: [306.1, -4.3, -106.6], receiver_pos: [369.8, -4.3, 56.7],    launch_timer: 30.0, viw_max: 3.0 },
    PairingData { sender_pos: [140.3, -4.4, 36.6],   receiver_pos: [226.5, -4.4, 74.7],    launch_timer: 15.0, viw_max: 3.0 },
    PairingData { sender_pos: [240.9, -4.4, -66.9],  receiver_pos: [134.9, -4.4, -117.8],  launch_timer: 20.0, viw_max: 3.0 },
    PairingData { sender_pos: [366.9, -4.4, -113.5], receiver_pos: [388.1, -4.4, 30.3],    launch_timer: 40.0, viw_max: 3.0 },
    PairingData { sender_pos: [143.2, -4.4, -72.5],  receiver_pos: [224.5, -4.4, -109.8],  launch_timer: 30.0, viw_max: 3.0 },
    PairingData { sender_pos: [401.6, -4.4, -71.0],  receiver_pos: [294.9, -4.4, 57.2],    launch_timer: 35.0, viw_max: 3.0 },
    PairingData { sender_pos: [335.4, -4.3, 67.9],   receiver_pos: [337.3, -4.3, -111.7],  launch_timer: 20.0, viw_max: 3.0 },
    PairingData { sender_pos: [246.0, -4.4, 11.7],   receiver_pos: [179.4, -5.0, 96.5],    launch_timer: 20.0, viw_max: 3.0 },
];

fn unity_to_bevy(pos: [f32; 3]) -> Vec3 {
    Vec3::new(pos[0], pos[1], -pos[2])
}

// -- Villager collider from the Moving Person prefab --
const VILLAGER_COLLIDER_HALF_EXTENTS: Vec3 = Vec3::new(1.1039985, 2.550293, 1.1331792);
const VILLAGER_MESH_HALF_HEIGHT: f32 = 2.550293;

// ============================================================
// Systems
// ============================================================

/// Startup system: pair senders with receivers, orient them, attach spawners + trigger sensors.
fn setup_villager_spawners(
    mut commands: Commands,
    mut q_senders: Query<(Entity, &mut Transform), With<VillagerSender>>,
    mut q_receivers: Query<
        (Entity, &mut Transform),
        (With<VillagerReceiver>, Without<VillagerSender>),
    >,
) {
    // Collect positions to avoid borrow issues during matching
    let sender_data: Vec<_> = q_senders.iter().map(|(e, t)| (e, t.translation)).collect();
    let receiver_data: Vec<_> = q_receivers.iter().map(|(e, t)| (e, t.translation)).collect();

    let mut paired = 0u32;

    for pairing in PAIRINGS {
        let sender_target = unity_to_bevy(pairing.sender_pos);
        let receiver_target = unity_to_bevy(pairing.receiver_pos);

        let sender_entity = sender_data
            .iter()
            .min_by(|a, b| {
                a.1.distance_squared(sender_target)
                    .partial_cmp(&b.1.distance_squared(sender_target))
                    .unwrap()
            })
            .filter(|(_, pos)| pos.distance(sender_target) < 2.0)
            .map(|(e, _)| *e);

        let receiver_entity = receiver_data
            .iter()
            .min_by(|a, b| {
                a.1.distance_squared(receiver_target)
                    .partial_cmp(&b.1.distance_squared(receiver_target))
                    .unwrap()
            })
            .filter(|(_, pos)| pos.distance(receiver_target) < 2.0)
            .map(|(e, _)| *e);

        let Some(sender_entity) = sender_entity else {
            warn!("No sender entity near {:?}", sender_target);
            continue;
        };
        let Some(receiver_entity) = receiver_entity else {
            warn!("No receiver entity near {:?}", receiver_target);
            continue;
        };

        let direction = (receiver_target - sender_target).normalize();

        // Orient sender to face receiver
        if let Ok((_, mut t)) = q_senders.get_mut(sender_entity) {
            t.look_at(receiver_target, Vec3::Y);
        }
        // Orient receiver to face sender
        if let Ok((_, mut t)) = q_receivers.get_mut(receiver_entity) {
            t.look_at(sender_target, Vec3::Y);
        }

        commands.entity(sender_entity).insert(VillagerSpawner {
            launch_timer_secs: pairing.launch_timer,
            counter_secs: 0.0,
            viw_max: pairing.viw_max.min(7.99),
            direction,
            receiver_pos: receiver_target,
        });

        paired += 1;
    }

    info!("Villager spawners: {paired}/{} pairings established", PAIRINGS.len());
}

/// FixedUpdate: advance sender timers and spawn villagers.
fn villager_spawn_system(
    mut commands: Commands,
    mut q_spawners: Query<(&Transform, &mut VillagerSpawner)>,
    state: Res<GameState>,
    time: Res<Time<Fixed>>,
    asset_server: Res<AssetServer>,
) {
    if state.movement_frozen.is_some() {
        return; // Timers pause while movement is frozen
    }

    for (sender_transform, mut spawner) in q_spawners.iter_mut() {
        spawner.counter_secs += time.delta_secs();
        if spawner.counter_secs < spawner.launch_timer_secs {
            continue;
        }
        spawner.counter_secs = 0.0;

        let spawn_pos = sender_transform.translation + Vec3::Y * VILLAGER_MESH_HALF_HEIGHT;
        let viw = spawner.direction * spawner.viw_max;

        commands.spawn((
            Villager,
            VillagerMovement {
                viw,
                start_time: state.world_time,
                death_time: None,
                receiver_pos: spawner.receiver_pos,
            },
            WorldAssetRoot(
                asset_server
                    .load(GltfAssetLabel::Scene(0).from_asset("models/MovingPerson.gltf")),
            ),
            NeedsRelativisticMaterial,
            Transform::from_translation(spawn_pos)
                .looking_at(spawn_pos + spawner.direction, Vec3::Y),
            GlobalTransform::default(),
            RigidBody::KinematicVelocityBased,
            Collider::cuboid(
                VILLAGER_COLLIDER_HALF_EXTENTS.x,
                VILLAGER_COLLIDER_HALF_EXTENTS.y,
                VILLAGER_COLLIDER_HALF_EXTENTS.z,
            ),
            Velocity::zero(),
            CollisionGroups::new(VILLAGER_GROUP, VILLAGER_GROUP),
            ActiveEvents::COLLISION_EVENTS,
            Visibility::Inherited,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Name::new("Villager"),
        ));
    }
}

/// FixedUpdate: set villager velocity based on frozen / relativistic state.
fn villager_velocity_update(
    mut q_villagers: Query<(&VillagerMovement, &mut Velocity), With<Villager>>,
    state: Res<GameState>,
) {
    let is_frozen = state.movement_frozen.is_some();

    for (movement, mut velocity) in q_villagers.iter_mut() {
        if is_frozen {
            velocity.linear = Vec3::ZERO;
        } else if !state.lorentz_factor.is_nan() && state.lorentz_factor != 0.0 {
            velocity.linear = movement.viw / state.lorentz_factor;
        } else {
            velocity.linear = Vec3::ZERO;
        }
    }
}

/// FixedUpdate: detect villager reaching receiver via position-based check -> mark death_time.
fn villager_receiver_trigger_system(
    mut q_villagers: Query<(&Transform, &mut VillagerMovement), With<Villager>>,
    state: Res<GameState>,
) {
    for (transform, mut movement) in q_villagers.iter_mut() {
        if movement.death_time.is_some() {
            continue;
        }
        // Vector from villager to receiver, projected onto travel direction.
        // When <= 2.0, the villager has reached or passed the receiver plane.
        let to_receiver = movement.receiver_pos - transform.translation;
        let direction = movement.viw.normalize();
        let remaining = to_receiver.dot(direction);
        if remaining <= 2.0 {
            movement.death_time = Some(state.world_time);
        }
    }
}

/// FixedUpdate: despawn dead villagers once the player's light cone reaches the death event.
fn villager_relativistic_despawn(
    mut commands: Commands,
    q_villagers: Query<(Entity, &Transform, &VillagerMovement), With<Villager>>,
    q_player: Query<&Transform, With<Player>>,
    state: Res<GameState>,
) {
    // Don't run while frozen (matches Unity: death check is inside !MovementFrozen block)
    if state.movement_frozen.is_some() {
        return;
    }

    let Ok(player_transform) = q_player.single() else {
        return;
    };
    let c_sq = state.speed_of_light * state.speed_of_light;

    for (entity, transform, movement) in q_villagers.iter() {
        // Safety fallback: despawn villagers alive longer than 120s
        // (longest travel is ~180 units at 3.0 u/s = 60s, doubled for Lorentz margin)
        let age = state.world_time - movement.start_time;
        if age > 120.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let Some(death_time) = movement.death_time else {
            continue;
        };

        let r = transform.translation - player_transform.translation;
        let v = movement.viw;
        let qa = -r.dot(r);
        let qb = -(2.0 * r.dot(v));
        let qc = c_sq - v.dot(v);
        let discriminant = qb * qb - 4.0 * qc * qa;

        if discriminant >= 0.0 {
            let t = (-qb - discriminant.sqrt()) / (2.0 * qc);
            if state.world_time + t > death_time {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Update: after a villager's GLTF scene loads and the shared relativistic material is created
/// by the existing observer, clone that material into a per-instance copy so each villager can
/// have its own viw / strt_time uniforms.  The global `update_relativistic_materials` system
/// will fill in vpc / player_offset / etc. on every material each frame.
fn setup_villager_materials(
    mut commands: Commands,
    q_villagers: Query<
        (Entity, &RelativisticObject, &VillagerMovement),
        (With<Villager>, Without<VillagerMaterialReady>),
    >,
    q_children: Query<&Children>,
    mut q_mat: Query<&mut MeshMaterial3d<RelativisticMaterial>>,
    mut rel_mats: ResMut<Assets<RelativisticMaterial>>,
    state: Res<GameState>,
) {
    for (entity, rel_obj, movement) in q_villagers.iter() {
        let Some(existing) = rel_mats.get(rel_obj.material_handle.id()) else {
            continue;
        };
        let mut mat = existing.clone();

        // Per-instance uniforms (the global system does NOT touch these fields)
        mat.uniform_data.viw = (movement.viw / state.speed_of_light).extend(0.0);
        mat.uniform_data.strt_time = movement.start_time;

        let new_handle = rel_mats.add(mat);

        // Point all mesh children at the per-instance material
        for child in q_children.iter_descendants(entity) {
            if let Ok(mut h) = q_mat.get_mut(child) {
                *h = MeshMaterial3d(new_handle.clone());
            }
        }

        commands.entity(entity).insert((
            RelativisticObject {
                viw: movement.viw,
                start_time: movement.start_time,
                material_handle: new_handle,
            },
            VillagerMaterialReady,
        ));
    }
}

/// Update (after global material update): keep each villager's material viw in sync.
fn update_villager_materials(
    q_villagers: Query<
        (&VillagerMovement, &RelativisticObject),
        (With<Villager>, With<VillagerMaterialReady>),
    >,
    mut rel_mats: ResMut<Assets<RelativisticMaterial>>,
    state: Res<GameState>,
) {
    let is_frozen = state.movement_frozen.is_some();

    for (movement, rel_obj) in q_villagers.iter() {
        let Some(mut mat) = rel_mats.get_mut(rel_obj.material_handle.id()) else {
            continue;
        };
        if is_frozen {
            mat.uniform_data.viw = Vec4::ZERO;
        } else {
            mat.uniform_data.viw = (movement.viw / state.speed_of_light).extend(0.0);
        }
    }
}

/// Observer: on PlayerRespawnRequest, despawn all villagers and reset spawner timers.
fn reset_villagers(
    _trigger: On<PlayerRespawnRequest>,
    mut commands: Commands,
    q_villagers: Query<Entity, With<Villager>>,
    mut q_spawners: Query<&mut VillagerSpawner>,
) {
    let count = q_villagers.iter().count();
    for entity in q_villagers.iter() {
        commands.entity(entity).despawn();
    }
    for mut spawner in q_spawners.iter_mut() {
        spawner.counter_secs = 0.0;
    }
    if count > 0 {
        info!("Villagers reset: despawned {count}, timers zeroed");
    }
}
