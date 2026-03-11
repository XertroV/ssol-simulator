
use bevy::{camera::visibility::{InheritedVisibility, ViewVisibility}, ecs::entity_disabling::Disabled, gizmos::config::DefaultGizmoConfigGroup, prelude::*};
use bevy_rapier3d::prelude::*;
use serde::Deserialize;
use core::f32;

use crate::orb_curriculum::{should_orb_be_active, OrbId};
use crate::{
    curriculum::CurriculumConfig,
    game_state::{Orb, OrbParent},
    relativity::rel_material::NeedsRelativisticMaterial,
};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SceneObject {
    pub name: String,
    pub tag: Option<String>,

    pub position: [f32; 3],
    // pub rotation: [f32; 3], // Euler angles (degrees)
    pub quat: [f32; 4], // Quaternion (x, y, z, w)
    pub scale: [f32; 3],
    // pub layer: i32,
    pub box_collider: Option<BoxCol>,
    pub sphere_collider: Option<SphereCol>,
}
impl SceneObject {
    fn is_orb(&self) -> bool {
        self.name == "orb"
    }

    fn is_white_arch(&self) -> bool {
        self.name == "whiteArch"
    }

    fn ignore(&self) -> bool {
        self.name.starts_with("pCube") || self.name.starts_with("group")
            || self.name.starts_with("Long_Pole") || self.name.starts_with("polySurface")
            || self.name.starts_with("leftTop") || self.name.starts_with("leftB")
            || self.name.starts_with("rightTop") || self.name.starts_with("rightB")
            || self.name.starts_with("transform") || self.name == "Camera"
            || self.name == "Cube" || self.name == "Player"
    }

    fn has_tag(&self, tag: &str) -> bool {
        self.tag.as_ref().map_or(false, |t| t == tag)
    }
}

#[derive(Deserialize, Debug)]
pub struct BoxCol {
    pub center: [f32; 3],
    pub size: [f32; 3],
}

#[derive(Deserialize, Debug)]
pub struct SphereCol {
    pub center: [f32; 3],
    pub radius: f32,
}

pub type SceneObjList = Vec<SceneObject>;

pub fn load_scene_data_from_file(file_path: &str) -> SceneObjList {
    let file = std::fs::File::open(file_path).expect("Failed to open scene file");
    let reader = std::io::BufReader::new(file);
    let scene_data: SceneObjList = serde_json::from_reader(reader).expect("Failed to parse scene data");
    scene_data
}


#[derive(Component)]
pub struct PlayerStart;

#[derive(Component)]
pub struct WhiteFinishArch;

#[derive(Component)]
pub struct WhiteFinishArchSensor;


pub fn setup_scene(mut commands: Commands, asset_server: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>, mut meshes: ResMut<Assets<Mesh>>, mut gizmo_config_store: ResMut<GizmoConfigStore>, mut curriculum_config: ResMut<CurriculumConfig>) {
    // Configure gizmos to render on top of everything (disable depth testing)
    let (gizmo_config, _) = gizmo_config_store.config_mut::<DefaultGizmoConfigGroup>();
    gizmo_config.depth_bias = -1.0; // Render on top

    // load_meshes = RenderAssetUsages::all();

    let scene_data = load_scene_data_from_file("assets/scenes/level-zero.json");

    // Separate orbs from other objects
    let mut orbs: Vec<&SceneObject> = Vec::new();
    let mut player_spawn: Option<Vec3> = None;

    for object in &scene_data {
        if object.ignore() {
            continue;
        }

        // Check for player spawn
        if object.tag.as_ref().map(|t| t.as_str() == "Playermesh").unwrap_or(false) {
            player_spawn = Some(json_pos(object.position));
        }

        if object.is_orb() {
            orbs.push(object);
        } else {
            spawn_object(&mut commands, &asset_server, &mut meshes, &mut materials, object, None, false);
        }
    }

    // Store player spawn position in curriculum config
    if let Some(pos) = player_spawn {
        curriculum_config.player_spawn_position = pos;
    }

    // Sort orbs by distance from player spawn - OrbId 0 is closest to spawn
    let player_pos = curriculum_config.player_spawn_position;
    orbs.sort_by(|a, b| {
        let pos_a = json_pos(a.position);
        let pos_b = json_pos(b.position);
        let dist_a = player_pos.distance(pos_a);
        let dist_b = player_pos.distance(pos_b);
        dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply curriculum constraints: radius filter and max_orbs limit
    // Since orbs are sorted by distance, we just take the first max_orbs that pass the radius check
    let mut active_count = 0u32;

    for (idx, orb_obj) in orbs.iter().enumerate() {
        let orb_id = OrbId(idx as u8);
        let orb_pos = json_pos(orb_obj.position);

        // Check if this orb should be active based on curriculum
        let is_active = should_orb_be_active(orb_pos, active_count, &curriculum_config);

        // Always spawn the orb (for consistent IDs), but disable if not active
        spawn_object(&mut commands, &asset_server, &mut meshes, &mut materials, orb_obj, Some(orb_id), !is_active);

        if is_active {
            active_count += 1;
        }
    }
    curriculum_config.active_orb_count = active_count;
    info!("Spawned {} orbs ({} active based on curriculum)", orbs.len(), active_count);
}

fn json_pos(pos: [f32; 3]) -> Vec3 {
    Vec3::new(pos[0], pos[1], -pos[2])
}
fn json_collider_pos(pos: [f32; 3]) -> Vec3 {
    Vec3::new(pos[0], pos[1], -pos[2])
}

// fn json_rot(rot: [f32; 3]) -> Quat {
//     Quat::from_euler(
//         EulerRot::YXZ,
//         -rot[1].to_radians() + f32::consts::PI,
//         rot[0].to_radians(),
//         -rot[2].to_radians(),
//     )
// }

fn json_quat(quat: [f32; 4]) -> Quat {
    Quat::from_xyzw(quat[0], quat[1], -quat[2], -quat[3])
}

fn spawn_object(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    object: &SceneObject,
    orb_id: Option<OrbId>,
    orb_disabled: bool,
) {
    // Don't spawn the player mesh itself, just mark its starting position.
    // if object.name == "Playermesh" {
    if object.tag.as_ref().map(|t| t.as_str() == "Playermesh").unwrap_or(false) {
        let translation = json_pos(object.position);
        commands.spawn((
            PlayerStart,
            Transform {
                translation,
                rotation: json_quat(object.quat),
                scale: object.scale.into(),
            },
        ));
        // info!("Player Spawn at: {}", translation);
        return; // Stop here for the player spawn.
    }

    let components = (
        Transform {
            translation: json_pos(object.position),
            rotation: json_quat(object.quat),
            scale: object.scale.into(),
        },
        GlobalTransform::default(),
        RigidBody::Fixed,
        NeedsRelativisticMaterial,
        // Add visibility components so children can inherit visibility
        Visibility::Inherited,
        InheritedVisibility::default(),
        ViewVisibility::default(),
    );
    let mut entity_commands;

    if object.name == "SOME SHAPE THING" {
        let mesh = match object.name.as_str() {
            "A mesh name" => meshes.add(Cuboid::from_length(1.0)),
            _ => panic!("Unexpected object name: {}", object.name),
        };
        warn!("Spawning object: {} at {}", object.name, json_pos(object.position));
        entity_commands = commands.spawn((
            // These are Recievers and debug shapes, so we don't want to show them.
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Name::new("Blah"),
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::linear_rgba(0., 0., 0., 0.),
                alpha_mode: AlphaMode::Mask(0.5),
                cull_mode: None,
                ..default()
            })),

        ));
    } else {
        let model_path = format!("models/{}.gltf", object.name);
        // let mesh: Handle<Mesh> = asset_server.load(GltfAssetLabel::Mesh(0).from_asset(model_path));
        entity_commands = commands.spawn(
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(model_path))),
        );
    }
    entity_commands.insert(components);
    if object.is_orb() {
        entity_commands.insert(OrbParent);
        // Add OrbId if provided (should always be provided for orbs)
        if let Some(id) = orb_id {
            entity_commands.insert(id);
        }
        // Disable orbs outside curriculum limits (max_orbs or radius)
        if orb_disabled {
            entity_commands.insert(Disabled);
            entity_commands.insert(Visibility::Hidden);
        }
    }
    if object.is_white_arch() {
        entity_commands.insert((WhiteFinishArch, Visibility::Hidden));
    }

    entity_commands.with_children(|children| {
        // Add a collider if one is defined in the JSON.
        // Include visibility components to prevent B0004 warnings about inheritance chain
        let mut child_cmds = if let Some(collider_data) = &object.box_collider {
            let size = &collider_data.size;
            children.spawn((
                Collider::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0),
                Transform::from_translation(json_collider_pos(collider_data.center)),
                Visibility::Inherited,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
        } else if let Some(collider_data) = &object.sphere_collider {
            children.spawn((
                Collider::ball(collider_data.radius),
                Transform::from_translation(json_collider_pos(collider_data.center)),
                Visibility::Inherited,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
        } else {
            return;
        };
        // Add a marker component if the object is an orb.
        if object.is_orb() {
            child_cmds.insert(Orb);
            child_cmds.insert(ActiveEvents::COLLISION_EVENTS);
            child_cmds.insert(Sensor);
        } else if object.is_white_arch() {
            child_cmds.insert(WhiteFinishArchSensor);
            child_cmds.insert(ActiveEvents::COLLISION_EVENTS);
            child_cmds.insert(Sensor);
        }
    });

    // entity_commands.insert((ShowAabbGizmo::default(),));
}
