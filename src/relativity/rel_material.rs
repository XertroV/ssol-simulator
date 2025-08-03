use bevy::{prelude::*, render::render_resource::AsBindGroup};



#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct RelativisticMaterial {
    #[uniform(0)]
    pub vpc: Vec4,
    #[uniform(1)]
    pub player_offset: Vec3,
    #[uniform(2)]
    pub world_time: f32,
}

impl Material for RelativisticMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/relativistic_shader.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/relativistic_shader.wgsl".into()
    }
}
