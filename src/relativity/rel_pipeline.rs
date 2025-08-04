// use bevy::{
//     pbr::{MaterialPipeline, MaterialPipelineKey, MeshPipeline, MeshPipelineKey},
//     prelude::*,
//     render::{render_resource::{
//         BindGroupLayout, BindGroupLayoutEntry, BindingType, BufferBindingType, RenderPipelineDescriptor, ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedMeshPipeline
//     }, renderer::RenderDevice},
// };

// use super::rel_globals::RelativisticGlobalsUniform;
// use super::rel_material::RelativisticMaterial;

// // This resource will be created at startup and hold our layouts.
// #[derive(Resource)]
// pub struct RelativisticPipeline {
//     shader: Handle<Shader>,
//     pub mesh_pipeline: MaterialPipeline<RelativisticMaterial>,
//     pub globals_layout: BindGroupLayout,
//     // We might add a material-specific layout here later if needed.
// }

// impl FromWorld for RelativisticPipeline {
//     fn from_world(world: &mut World) -> Self {
//         let render_device = world.resource::<RenderDevice>();

//         // world.resource::<

//         // Create the layout for our global uniforms once.
//         let globals_layout = render_device.create_bind_group_layout(
//             Some("relativistic_globals_layout"),
//             &[BindGroupLayoutEntry {
//                 binding: 0,
//                 visibility: ShaderStages::VERTEX_FRAGMENT,
//                 ty: BindingType::Buffer {
//                     ty: BufferBindingType::Uniform,
//                     has_dynamic_offset: false,
//                     min_binding_size: Some(RelativisticGlobalsUniform::min_size()),
//                 },
//                 count: None,
//             }],
//         );

//         RelativisticPipeline {
//             pipeline: world.resource::<MaterialPipeline<RelativisticMaterial>>().clone(),
//             globals_layout,
//         }
//     }
// }

// // This tells Bevy how to create a render pipeline for our material.
// impl SpecializedRenderPipeline for RelativisticPipeline {
//     type Key = MaterialPipelineKey<RelativisticMaterial>;

//     fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
//         // Start with a standard mesh pipeline descriptor.
//         let mut descriptor = ;

//         // Add our global layout to the pipeline's layout at group 2.
//         descriptor.layout.insert(2, self.globals_layout.clone());

//         descriptor
//     }
// }
