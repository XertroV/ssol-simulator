
use bevy::{gltf::GltfMesh, prelude::*, scene::SceneInstanceReady};
use bevy_rapier3d::prelude::*;
use serde::Deserialize;
use core::f32;
use iyes_perf_ui::entries::PerfUiDefaultEntries;
use std::{fs::read_to_string, mem};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SceneObject {
    pub name: String,
    // pub tag: String,
    /// Should always be "LevelZero"
    // pub scene_name: String,
    // pub prefab_name: String,
    // pub prefab_path: String,
    pub position: [f32; 3],
    pub rotation: [f32; 3], // Euler angles (degrees)
    pub scale: [f32; 3],
    // pub layer: i32,
    pub box_collider: Option<BoxCol>,
    pub sphere_collider: Option<SphereCol>,
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
pub struct Orb;

#[derive(Component)]
pub struct PlayerStart;

pub fn setup_scene(mut commands: Commands, asset_server: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>, mut meshes: ResMut<Assets<Mesh>>, mut gizmo_config_store: ResMut<GizmoConfigStore>) {
    // gizmo_config_store.config_mut::<AabbGizmoConfigGroup>().1.draw_all = true;

    let scene_data = load_scene_data_from_file("assets/scenes/level-zero.json");
    let skip_prefixes = ["pCube", "group", "Long_Pole", "Sphere", "Cube", "polySurface", "leftTop", "leftB", "rightTop", "rightB", "transform", "Camera"];
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
    Vec3::new(-pos[0], pos[1], pos[2])
}
fn json_collider_pos(pos: [f32; 3]) -> Vec3 {
    Vec3::new(pos[0], pos[1], -pos[2])
}

fn json_rot(rot: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        -rot[1].to_radians() + f32::consts::PI,
        rot[0].to_radians(),
        rot[2].to_radians(),
    )
}

fn spawn_object(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    object: &SceneObject,
) {
    // Don't spawn the player mesh itself, just mark its starting position.
    if object.name == "Player" {
        commands.spawn((
            PlayerStart,
            Transform {
                translation: json_pos(object.position),
                rotation: json_rot(object.rotation),
                scale: object.scale.into(),
            },
        ));
        return; // Stop here for the player object.
    }

    let model_path = format!("models/{}.gltf", object.name);
    // let mesh: Handle<Mesh> = asset_server.load(GltfAssetLabel::Mesh(0).from_asset(model_path));

    let mut entity_commands = commands.spawn((
        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(model_path))),
        Transform {
            translation: json_pos(object.position),
            rotation: json_rot(object.rotation),
            scale: object.scale.into(),
        },
        GlobalTransform::default(),
        RigidBody::Fixed,
    ));

    entity_commands.with_children(|children| {
        // Add a collider if one is defined in the JSON.
        if let Some(collider_data) = &object.box_collider {
            let size = &collider_data.size;
            children.spawn((
                Collider::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0),
                Transform::from_translation(json_collider_pos(collider_data.center)),
            )); // Collider for box
        } else if let Some(collider_data) = &object.sphere_collider {
            children.spawn((
                Collider::ball(collider_data.radius),
                Transform::from_translation(json_collider_pos(collider_data.center)),
            ));
        }
    });

    // entity_commands.insert((ShowAabbGizmo::default(),));

    // Add a marker component if the object is an orb.
    if object.name == "orb" {
        entity_commands.insert(Orb);
        // Orbs need to detect collisions, so we enable collision events.
        entity_commands.insert(ActiveEvents::COLLISION_EVENTS);
        // Orbs are sensors so you can pass through them.
        entity_commands.insert(Sensor);
    }
}
