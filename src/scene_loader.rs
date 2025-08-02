
use bevy::{gltf::GltfMesh, prelude::*, render::mesh::PrimitiveTopology, scene::SceneInstanceReady};
use bevy_rapier3d::prelude::*;
use serde::Deserialize;
use core::f32;
use iyes_perf_ui::entries::PerfUiDefaultEntries;
use std::{fs::read_to_string, mem};

use crate::game_state::{Orb, OrbParent};

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


pub fn setup_scene(mut commands: Commands, asset_server: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>, mut meshes: ResMut<Assets<Mesh>>, mut gizmo_config_store: ResMut<GizmoConfigStore>) {
    // gizmo_config_store.config_mut::<AabbGizmoConfigGroup>().1.draw_all = true;

    // load_meshes = RenderAssetUsages::all();

    let scene_data = load_scene_data_from_file("assets/scenes/level-zero.json");
    let mut skip_prefixes = vec!["pCube", "group", "Long_Pole", "polySurface", "leftTop", "leftB", "rightTop", "rightB", "transform", "Camera"];
    // Cubes are (duplicate) markers for villager receivers and the sphere is at player spawn; Sphere has the player info we want.
    skip_prefixes.extend(["Cube", "Player"]); //, "Sphere"]);
    for object in scene_data {
        if skip_prefixes.iter().any(|prefix| object.name.starts_with(prefix)) {
            continue;
        }
        spawn_object(&mut commands, &asset_server, &mut meshes, &mut materials, &object);
    }
    commands.spawn(PerfUiDefaultEntries::default());

    // commands.add_system(apply_material_properties);
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
) {
    // Don't spawn the player mesh itself, just mark its starting position.
    if object.tag.as_ref().map(|t| t.as_str() == "Playermesh").unwrap_or(false) {
        commands.spawn((
            PlayerStart,
            Transform {
                translation: json_pos(object.position),
                rotation: json_quat(object.quat),
                scale: object.scale.into(),
            },
        ));
        return; // Stop here for the player spawn.
    }

    let components = (
        Transform {
            translation: json_pos(object.position),
            rotation: json_quat(object.quat),
            scale: object.scale.into(),
        },
        GlobalTransform::default(),
        RigidBody::Fixed
    );
    let mut entity_commands;

    if object.name == "Sphere" || object.name == "Cube" {
        let mesh = match object.name.as_str() {
            "Sphere" => meshes.add(Sphere::new(1.0)),
            "Cube" => meshes.add(Cuboid::from_length(1.0)),
            _ => panic!("Unexpected object name: {}", object.name),
        };
        warn!("Spawning object: {} at {}", object.name, json_pos(object.position));
        entity_commands = commands.spawn((
            // Mesh3d(mesh),
            // MeshMaterial3d(materials.add(StandardMaterial {
            //     base_color: Color::linear_rgba(0., 0., 0., 0.),
            //     alpha_mode: AlphaMode::Mask(0.5),
            //     cull_mode: None,
            //     ..default()
            // })),
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
    }

    entity_commands.with_children(|children| {
        // Add a collider if one is defined in the JSON.
        let mut cmds = if let Some(collider_data) = &object.box_collider {
            let size = &collider_data.size;
            children.spawn((
                Collider::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0),
                Transform::from_translation(json_collider_pos(collider_data.center)),
            ))
        } else if let Some(collider_data) = &object.sphere_collider {
            children.spawn((
                Collider::ball(collider_data.radius),
                Transform::from_translation(json_collider_pos(collider_data.center)),
            ))
        } else {
            return;
        };
        // Add a marker component if the object is an orb.
        if object.is_orb() {
            cmds.insert(Orb);
            // Orbs need to detect collisions, so we enable collision events.
            cmds.insert(ActiveEvents::COLLISION_EVENTS);
            // Orbs are sensors so you can pass through them.
            cmds.insert(Sensor);
        }
    });

    // entity_commands.insert((ShowAabbGizmo::default(),));
}
