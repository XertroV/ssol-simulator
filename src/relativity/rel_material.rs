use bevy::{prelude::*, render::{mesh::MeshVertexBufferLayoutRef, render_resource::{AsBindGroup, Buffer, RenderPipelineDescriptor, ShaderRef, ShaderType, SpecializedMeshPipelineError}}};

use crate::{game_state::GameState, player::Player};

pub struct RelativisticMaterialPlugin;

impl Plugin for RelativisticMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<RelativisticMaterial>::default())
            // .add_systems(ExtractSchedule, extract_relativistic_material_data)
            // .add_systems(Startup, add_relativistic_object_component)
            .add_systems(Update, (update_relativistic_materials,))
            .add_systems(Startup, setup_test_cube)
            ;
    }
}



pub fn update_relativistic_materials(
    mut materials: ResMut<Assets<RelativisticMaterial>>,
    state: Res<GameState>,
    q_player: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = q_player.single() else { return };

    // Iterate over all loaded materials of our custom type.
    for (_, material) in materials.iter_mut() {
        // Copy the current game state into the material's uniform block.
        material.uniform_data.vpc = (state.player_velocity_vector / state.speed_of_light).extend(0.0) * -1.0;
        material.uniform_data.player_offset = player_transform.translation.extend(0.0);
        material.uniform_data.spd_of_light = state.speed_of_light;
        material.uniform_data.wrld_time = 0.0; // todo
        material.uniform_data.color_shift = 1; // 1 for true
        // We don't have per-object velocity yet, so we'll keep viw as zero.
        material.uniform_data.viw = Vec4::ZERO;
    }
}


#[derive(Component)]
pub struct RelativisticObject {
    /// Velocity in world space.
    pub viw: Vec3,
    pub start_time: f32,
    pub material_handle: Handle<RelativisticMaterial>,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct RelativisticMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub base_texture: Handle<Image>,

    #[texture(2)]
    #[sampler(3)]
    pub uv_texture: Handle<Image>,

    #[texture(4)]
    #[sampler(5)]
    pub ir_texture: Handle<Image>,

    // Uniforms that we will update from our systems.
    #[uniform(6)]
    pub uniform_data: RelativisticUniforms,
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
    // pub world_matrix: Mat4,
}

impl Material for RelativisticMaterial {
    fn fragment_shader() -> ShaderRef { "shaders/rel_shader.wgsl".into() }
    fn vertex_shader() -> ShaderRef { "shaders/rel_shader.wgsl".into() }
    fn alpha_mode(&self) -> AlphaMode { AlphaMode::Opaque }
    // mesh attributes (UVs)
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
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
