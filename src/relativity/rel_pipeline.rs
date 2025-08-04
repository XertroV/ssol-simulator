use bevy::{
    core_pipeline::core_3d::{Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey},
    ecs::{component::Tick, system::{lifetimeless::SRes, SystemParamItem}},
    pbr::*,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        mesh::RenderMesh,
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, PhaseItem, RenderCommand, SetItemPipeline, ViewBinnedRenderPhases
        },
        render_resource::*,
        renderer::RenderDevice,
        view::*,
        Render, RenderApp, RenderSet,
    },
};

use crate::{
    relativity::rel_globals::RelativisticGlobalsUniform,
    relativity::rel_material::RelativisticMaterial,
};

// This plugin will set up everything in the Render App.
pub struct RelativisticPipelinePlugin;

impl Plugin for RelativisticPipelinePlugin {
    fn build(&self, app: &mut App) {
        // This marker component will be extracted to the Render World.
        app.add_plugins(ExtractComponentPlugin::<UseRelativisticPipeline>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<SpecializedMeshPipelines<RelativisticPipeline>>()
            // Add our custom render command to the Opaque3d render phase.
            .add_render_command::<Opaque3d, DrawRelativisticMesh>()
            .add_systems(Render, queue_relativistic_meshes.in_set(RenderSet::Queue));
    }

    // This function runs once on startup to initialize our pipeline.
    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<RelativisticPipeline>();
    }
}

// 1. The Marker Component
// Attach this to any entity in the main world that should be rendered with our effect.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
pub struct UseRelativisticPipeline;

// The custom pipeline definition.
#[derive(Resource)]
pub struct RelativisticPipeline {
    pub mesh_pipeline: MeshPipeline,
    pub shader: Handle<Shader>,
    pub relativistic_globals_layout: BindGroupLayout,
}

impl FromWorld for RelativisticPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        // This layout defines the binding for our global uniforms.
        // It will be @group(3) in the shader.
        let relativistic_globals_layout =
            render_device.create_bind_group_layout(Some("relativistic_globals_layout"), &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(RelativisticGlobalsUniform::min_size()),
                    },
                    count: None,
                },
            ]);

        let asset_server = world.resource::<AssetServer>();
        RelativisticPipeline {
            mesh_pipeline: world.resource::<MeshPipeline>().clone(),
            shader: asset_server.load("shaders/rel_shader.wgsl"),
            relativistic_globals_layout,
        }
    }
}

// The specialization logic for our pipeline.
impl SpecializedMeshPipeline for RelativisticPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;
        descriptor.vertex.shader = self.shader.clone();
        descriptor.fragment.as_mut().unwrap().shader = self.shader.clone();
        // Insert our custom global bind group layout at index 3.
        // This corresponds to @group(3) in the shader.
        descriptor.layout.insert(3, self.relativistic_globals_layout.clone());
        Ok(descriptor)
    }
}

// 2. The Custom RenderCommand
// This is a tuple of render commands that tells the GPU what to do.
type DrawRelativisticMesh = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMaterialBindGroup<RelativisticMaterial, 2>,
    SetMeshBindGroup<1>,
    SetRelativisticGlobalsBindGroup<3>,
    DrawMesh,
);

#[derive(Resource)]
pub struct RelativisticGlobalsBindGroup {
    pub bind_group: BindGroup,
}

// This system implements the `SetRelativisticGlobalsBindGroup` command.
pub struct SetRelativisticGlobalsBindGroup<const I: usize>;
impl<const I: usize, P: bevy::render::render_phase::PhaseItem> RenderCommand<P> for SetRelativisticGlobalsBindGroup<I> {
    type Param = SRes<RelativisticGlobalsBindGroup>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        globals_bind_group: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut bevy::render::render_phase::TrackedRenderPass<'w>,
    ) -> bevy::render::render_phase::RenderCommandResult {
        let bind_group = &globals_bind_group.into_inner().bind_group;
        pass.set_bind_group(I, bind_group, &[]);
        bevy::render::render_phase::RenderCommandResult::Success
    }
}

// 3. The Queueing System
// This system finds all entities with our marker component and adds them to the Opaque3d render phase.
#[allow(clippy::too_many_arguments)]
pub fn queue_relativistic_meshes(
    opaque_3d_draw_functions: Res<DrawFunctions<Opaque3d>>,
    relativistic_pipeline: Res<RelativisticPipeline>,
    mut specialized_pipelines: ResMut<SpecializedMeshPipelines<RelativisticPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    material_bind_groups: Res<RenderMaterialBindings>,
    mut opaque_render_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut change_tick: Local<Tick>,
    views: Query<(
        &ExtractedView,
        &RenderVisibleEntities,
        &Msaa,
    )>,
) {
    let draw_function = opaque_3d_draw_functions.read().id::<DrawRelativisticMesh>();

    for (view, visible_entities, msaa) in &views {
        let Some(opaque_phase) = opaque_render_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };

        let view_key =
            MeshPipelineKey::from_msaa_samples(msaa.samples()) | MeshPipelineKey::from_hdr(view.hdr);

        let next_change_tick = change_tick.get() + 1;
        change_tick.set(next_change_tick);

        for (entity, visible_entity) in visible_entities.get::<UseRelativisticPipeline>().iter() {
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(*visible_entity) else {
                continue;
            };
            let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
                continue;
            };
            let material_binding_id = mesh_instance.material_bindings_index;

            let mut mesh_key = view_key;
            mesh_key |= MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());

            let pipeline = specialized_pipelines
                .specialize(&pipeline_cache, &relativistic_pipeline, mesh_key, &mesh.layout)
                .unwrap();

            opaque_phase.add(
                Opaque3dBatchSetKey {
                    draw_function,
                    pipeline,
                    material_bind_group_index: Some(material_binding_id.group.0),
                    lightmap_slab: None,
                    vertex_slab: Default::default(),
                    index_slab: None,
                },
                Opaque3dBinKey {
                    asset_id: mesh_instance.mesh_asset_id.untyped(),
                },
                (*entity, *visible_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::UnbatchableMesh,
                *change_tick,
            );
        }
        change_tick.set(next_change_tick);
    }
}
