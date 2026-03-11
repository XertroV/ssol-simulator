use std::{cell::OnceCell, ffi::{OsStr}};

use bevy::{camera::{primitives::Aabb, visibility::{InheritedVisibility, ViewVisibility}}, mesh::MeshVertexBufferLayoutRef, platform::collections::HashMap, prelude::*, render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError}, scene::SceneInstanceReady, shader::ShaderRef};

use crate::{config::GraphicsSettings, game_state::GameState, player::Player, CLEAR_COLOR};

pub struct RelativisticMaterialPlugin;

impl Plugin for RelativisticMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<RelativisticMaterial>::default())
            .add_plugins(MaterialPlugin::<SkyboxMaterial>::default())
            .add_systems(Startup, (setup_skybox,))
            .add_systems(Update, (update_relativistic_materials,))
            .add_observer(swap_to_relativistic_material)
            .init_resource::<RelativisticMatLookup>()
            // .add_systems(Startup, setup_test_cube)
            ;
    }
}

/// A map of entity IDs to their relativistic material handles.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct RelativisticMatLookup(pub HashMap<String, Handle<RelativisticMaterial>>);


/// A component to mark objects that should use our relativistic material after loading from GLTF.
#[derive(Component)]
pub struct NeedsRelativisticMaterial;

pub fn update_relativistic_materials(
    mut materials: ResMut<Assets<RelativisticMaterial>>,
    mut skybox_mat: ResMut<Assets<SkyboxMaterial>>,
    graphics: Res<GraphicsSettings>,
    state: Res<GameState>,
    q_player: Query<&Transform, With<Player>>,
    // q_player_cam: Query<&Projection, With<PlayerCamera>>,
) {
    let Ok(p_transform) = q_player.single() else { return };
    // let Ok(projection) = q_player_cam.single() else { return };
    // velocity player
    let vpc = (state.player_velocity_vector / state.speed_of_light).extend(0.0) * -1.0;
    let p_offset = p_transform.translation.extend(0.0);
    let color_shift = state.color_shift();
    let desaturation_enabled = u32::from(graphics.desaturation_enabled);
    // let xs_xyr: Vec2 = Vec2::new(
    //     (projection.get_fov() / 2.0).tan(), // tan(fov/2)
    //     projection.get_aspect()
    // );
    // Iterate over all loaded materials of our custom type.
    for (_, material) in materials.iter_mut() {
        // Copy the current game state into the material's uniform block.
        material.uniform_data.vpc = vpc;
        material.uniform_data.player_offset = p_offset;
        material.uniform_data.spd_of_light = state.speed_of_light;
        material.uniform_data.wrld_time = state.world_time;
        material.uniform_data.color_shift = color_shift; // 1 for true
        material.uniform_data.desaturation_enabled = desaturation_enabled;
        // material.uniform_data.screen_xs_xyratio = xs_xyr;
        // We don't have per-object velocity yet, so we'll keep viw as zero.
        // material.uniform_data.viw = Vec4::ZERO;
    }
    for (_, material) in skybox_mat.iter_mut() {
        // Copy the current game state into the skybox material's uniform block.
        material.uniforms.vpc = vpc;
        material.uniforms.player_offset = p_offset;
        material.uniforms.spd_of_light = state.speed_of_light;
        material.uniforms.wrld_time = state.world_time;
        material.uniforms.color_shift = color_shift; // 1 for true
        material.uniforms.desaturation_enabled = desaturation_enabled;
        // material.uniforms.screen_xs_xyratio = xs_xyr;
    }
}

fn swap_to_relativistic_material(
    trigger: On<SceneInstanceReady>,
    mut commands: Commands,
    q_children: Query<&Children>,
    q_std_mat: Query<(&MeshMaterial3d<StandardMaterial>,)>,
    q_to_rel: Query<Entity, With<NeedsRelativisticMaterial>>,
    mut rel_mats: ResMut<Assets<RelativisticMaterial>>,
    std_mats: Res<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut rel_mat_lookup: ResMut<RelativisticMatLookup>,
) {
    let ent = trigger.entity;
    // ensure NeedsRelativisticMaterial and get the entity commands.
    if !q_to_rel.contains(ent) { return; }

    let mut rel_unis = OnceCell::new();

    for child in q_children.iter_descendants(ent) {
        // find the child with a standard material
        let Ok((c_mat,)) = q_std_mat.get(child) else { continue };
        let Some(c_mat_h) = std_mats.get(c_mat.id()) else { continue };
        let Some(c_tex_img) = c_mat_h.base_color_texture.as_ref() else { continue };
        let Some(c_tex_path) = c_tex_img.path() else { continue };
        let Some(tex_file_stem) = c_tex_path.path().file_stem() else { continue };
        let tm_key = tex_file_stem.to_str().unwrap_or_default().to_owned();
        let rel_mat = rel_mat_lookup.entry(tm_key).or_insert_with(|| {
            // info!("RelMaterial Init for: {:?}", tex_file_stem);
            let uv_stem_path = lookup_rel_texture(tex_file_stem, RelativisticTextureType::UV);
            let ir_stem_path = lookup_rel_texture(tex_file_stem, RelativisticTextureType::IR);
            let uv_texture = asset_server.load(uv_stem_path + ".webp");
            let ir_texture = asset_server.load(ir_stem_path + ".webp");
            let rel_mat = rel_mats.add(RelativisticMaterial::new(
                c_tex_img.clone(),
                uv_texture,
                ir_texture,
            ));
            rel_mat
        });

        commands.entity(child)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((
                MeshMaterial3d::from(rel_mat.clone()),
                Aabb::from_min_max(Vec3::splat(-10000.0), Vec3::splat(10000.0)), // Set a large AABB to avoid culling issues
            ));
        let _ = rel_unis.set(RelativisticObject::from(rel_mat.clone()));
    }

    commands.entity(ent).remove::<NeedsRelativisticMaterial>()
        .insert((
            rel_unis.take().unwrap(),
        ));
    // info!("Swapped {} materials to RelativisticMaterial for entity {:?}", mats_created, ent);
}

fn lookup_rel_texture(tex_file_stem: &OsStr, tex_ty: RelativisticTextureType) -> String {
    let stem = tex_file_stem.to_str().unwrap_or_default();
    match (stem, tex_ty) {
        ("maptile2", RelativisticTextureType::UV) => "textures/maptile2IR".into(),
        ("maptile3", RelativisticTextureType::UV) => "textures/maptile3IR".into(),
        ("mushroom", RelativisticTextureType::IR) => "textures/mushroomUV".into(),
        ("mushrooms", RelativisticTextureType::UV) => "textures/mushroomsIR".into(),
        ("maptile4", RelativisticTextureType::UV) => "textures/maptile4IR".into(),
        ("maptile1", RelativisticTextureType::UV) => "textures/maptile1IR".into(),
        ("hutroof", _) => "textures/hutroof".into(),
        ("hutChimney", _) => "textures/hutChimney".into(),
        ("archmiddlewhite", _) => "textures/archmiddlewhite".into(),
        ("archpillarwhite", _) => "textures/archpillarwhite".into(),
        ("shrub_01_d", _) => format!("textures/Shrub_01{}", tex_ty.get_stem_suffix()),
        ("shrub_04_d", _) => format!("textures/Shrub_04{}", tex_ty.get_stem_suffix()),
        ("weeds_01_d", _) => format!("textures/weeds_01{}", tex_ty.get_stem_suffix()),
        ("fencepurple", _) => format!("textures/fence{}", tex_ty.get_stem_suffix()),
        ("fenceredreverse", _) => format!("textures/fencered{}Invert", tex_ty.get_stem_suffix()),
        _ => format!("textures/{}{}", stem, tex_ty.get_stem_suffix())
    }
}


#[derive(Clone, Copy)]
#[allow(dead_code)]
enum RelativisticTextureType {
    UV,
    IR,
    Base,
}


impl RelativisticTextureType {
    fn get_stem_suffix(&self) -> &'static str {
        match self {
            RelativisticTextureType::UV => "UV",
            RelativisticTextureType::IR => "IR",
            RelativisticTextureType::Base => "",
        }
    }
}


#[derive(Component)]
#[allow(dead_code)]
pub struct RelativisticObject {
    /// Velocity in world space.
    pub viw: Vec3,
    pub start_time: f32,
    pub material_handle: Handle<RelativisticMaterial>,
}

impl RelativisticObject {
    #[allow(dead_code)]
    pub fn new(viw: Vec3, start_time: f32, material_handle: Handle<RelativisticMaterial>) -> Self {
        Self {
            viw,
            start_time,
            material_handle,
        }
    }
}

impl From<Handle<RelativisticMaterial>> for RelativisticObject {
    fn from(material_handle: Handle<RelativisticMaterial>) -> Self {
        Self {
            viw: Vec3::ZERO,
            start_time: 0.0,
            material_handle,
        }
    }
}


#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct RelativisticMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub base_texture: Handle<Image>,

    #[texture(2)]
    pub uv_texture: Handle<Image>,

    #[texture(3)]
    pub ir_texture: Handle<Image>,

    // Uniforms that we will update from our systems.
    #[uniform(4)]
    pub uniform_data: RelativisticUniforms,
}

impl RelativisticMaterial {
    fn new(base_texture: Handle<Image>, uv_texture: Handle<Image>, ir_texture: Handle<Image>) -> Self {
        Self {
            base_texture,
            uv_texture,
            ir_texture,
            uniform_data: default(),
        }
    }
}

#[derive(AsBindGroup, Clone, Copy, Default, ShaderType)]
pub struct RelativisticUniforms {
    /// velocity of player
    pub vpc: Vec4,
    /// velocity in world
    pub viw: Vec4,
    pub player_offset: Vec4,
    pub spd_of_light: f32,
    pub wrld_time: f32,
    pub strt_time: f32,
    pub color_shift: u32, // Use u32 for bools in shaders
    pub desaturation_enabled: u32,
    // pub world_matrix: Mat4,
    // pub screen_xs_xyratio: Vec2,
}

impl Material for RelativisticMaterial {
    fn fragment_shader() -> ShaderRef { "shaders/rel_shader.wgsl".into() }
    fn vertex_shader() -> ShaderRef { "shaders/rel_shader.wgsl".into() }
    // fn alpha_mode(&self) -> AlphaMode { AlphaMode::Opaque }
    fn alpha_mode(&self) -> AlphaMode { AlphaMode::Mask(0.5) }

    // mesh attributes (UVs)
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// A temporary system to spawn one cube with our custom material for testing.
#[allow(dead_code)]
pub fn setup_test_cube(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RelativisticMaterial>>, // Use our custom material type
    asset_server: Res<AssetServer>,
) {
    // TODO: Load the textures needed by the material.
    let base_texture = asset_server.load("textures/orb.webp");
    let uv_texture = asset_server.load("textures/orbUV.webp");
    let ir_texture = asset_server.load("textures/orbIR.webp");

    let mesh_handle = meshes.add(Cylinder::new(3.0, 35.0).mesh().segments(100).resolution(32));
    // symetrical around ground produces no bending since vertices only on top and bottom
    // let mesh_handle = meshes.add(Cuboid::new(3.0, 25.0, 3.0));
    let material_handle = materials.add(RelativisticMaterial {
        base_texture,
        uv_texture,
        ir_texture,
        uniform_data: RelativisticUniforms::default(),
        // uniform_buffer: None, // This will be set by the render pipeline
    });

    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle.clone()),
        // Transform::from_xyz(20.0, 8.0, 0.0),
        Transform::from_xyz(20.0, 12.5, 0.0),
        // Transform::from_xyz(20.0, 0.0, 0.0),
        Visibility::default(),
        RelativisticObject {
            viw: Vec3::ZERO,
            start_time: 0.0,
            material_handle
        },
        Name::new("TestCube"),
    ));
}

// MARK: Skybox

#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct SkyboxMaterial {
    #[uniform(0)]
    pub sky_color: Vec4,
    #[uniform(1)]
    pub uniforms: RelativisticUniforms,
}

impl Material for SkyboxMaterial {
    fn fragment_shader() -> ShaderRef { "shaders/rel_skybox.wgsl".into() }
    fn vertex_shader() -> ShaderRef { "shaders/rel_skybox.wgsl".into() }
    fn alpha_mode(&self) -> AlphaMode { AlphaMode::Opaque }

    // fn specialize(
    //     _pipeline: &bevy::pbr::MaterialPipeline<Self>,
    //     descriptor: &mut RenderPipelineDescriptor,
    //     layout: &MeshVertexBufferLayoutRef,
    //     _key: bevy::pbr::MaterialPipelineKey<Self>,
    // ) -> Result<(), SpecializedMeshPipelineError> {
    //     let vertex_layout = layout.0.get_layout(&[
    //         Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
    //         Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
    //     ])?;
    //     descriptor.vertex.buffers = vec![vertex_layout];
    //     Ok(())
    // }
}

impl Default for SkyboxMaterial {
    fn default() -> Self {
        Self {
            sky_color: CLEAR_COLOR.to_linear().to_vec4(),
            uniforms: RelativisticUniforms::default(),
        }
    }
}

#[derive(Component)]
pub struct Skybox;

fn setup_skybox(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyboxMaterial>>,
) {
    let sb_mat = materials.add(SkyboxMaterial::default());
    // Create a large cube to act as the skybox.
    // The cube is centered at the origin and has a size of 5_000 units.
    let mesh2 = meshes.add(
        Cuboid::from_length(10_000.0).mesh().build()
            .scaled_by(Vec3::splat(-1.0)) // Invert the scale to face the inside
    );
    commands.spawn((
        Skybox,
        Mesh3d(mesh2),
        MeshMaterial3d(sb_mat.clone()),
        Transform::from_translation(Vec3::ZERO),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        // RenderLayers::layer(0),
        Name::new("Skybox"),
    ));
    // info!("Skybox spawned with material: {:?}", mat);
}
