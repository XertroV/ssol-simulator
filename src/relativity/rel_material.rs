#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

use std::ffi::OsStr;

use bevy::{
    asset::{load_internal_asset, weak_handle, Handle}, core_pipeline::core_3d::{
        AlphaMask3d, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey, Transparent3d,
    }, ecs::{
        component::Tick,
        query::{QueryItem, ROQueryItem},
        system::{
            lifetimeless::{Read, SRes}, StaticSystemParam, SystemParamItem
        },
    }, pbr::{
        DrawMesh, MeshPipeline, MeshPipelineKey, MeshPipelineViewLayoutKey, RenderMeshInstances,
        SetMeshBindGroup, SetMeshViewBindGroup,
    }, platform::collections::HashMap, prelude::*, render::{
        batching::{
            gpu_preprocessing::{
                get_or_create_work_item_buffer, init_work_item_buffers, PhaseBatchedInstanceBuffers,
                PhaseIndirectParametersBuffers, PreprocessWorkItem,
                UntypedPhaseBatchedInstanceBuffers,
            },
            GetBatchData, GetFullBatchData,
        }, extract_component::{ExtractComponent, ExtractComponentPlugin}, mesh::{MeshVertexBufferLayoutRef, RenderMesh}, primitives::Aabb, render_asset::{RenderAssetUsages, RenderAssets}, render_phase::*, render_resource::{binding_types::*, *}, renderer::{RenderDevice, RenderQueue}, texture::{FallbackImage, GpuImage}, view::{
            self, ExtractedView, NoIndirectDrawing, RenderVisibleEntities, ViewTarget,
            VisibilityClass,
        }, Extract, ExtractSchedule, Render, RenderApp, RenderSet
    }, scene::SceneInstanceReady
};
use bytemuck::NoUninit;

// Shader handle
pub const CUSTOM_MATERIAL_SHADER_HANDLE: Handle<Shader> = weak_handle!("49fcc61a-7c0b-4b92-853f-7f66be59c3ab");

// Main plugin
#[derive(Default)]
pub struct CustomRenderPlugin;

impl Plugin for CustomRenderPlugin {
    fn build(&self, app: &mut App) {
        // Load the shader (you'll need to provide the actual shader file)
        load_internal_asset!(
            app,
            CUSTOM_MATERIAL_SHADER_HANDLE,
            "../../assets/shaders/rel_shader.wgsl",
            Shader::from_wgsl
        );

        app.init_resource::<GlobalCustomUniforms>()
            .init_resource::<RelativisticMatLookup>()
            .add_plugins(ExtractComponentPlugin::<CustomMaterialInstance>::default())
            .add_observer(swap_to_relativistic_material)
            ;

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<GlobalCustomUniformBuffer>()
                .init_resource::<CustomMaterialPipeline>()
                .init_resource::<SpecializedMeshPipelines<CustomMaterialPipeline>>()
                .add_systems(ExtractSchedule, extract_global_uniforms)
                .add_systems(
                    Render,
                    (
                        prepare_global_uniforms.in_set(RenderSet::PrepareResources),
                        prepare_global_bind_group.in_set(RenderSet::PrepareBindGroups),
                        prepare_instance_uniforms.in_set(RenderSet::PrepareResources),
                        queue_custom_material_meshes.in_set(RenderSet::Queue),
                    ),
                ).add_systems(
                    Render,
                    prepare_instance_bind_groups.in_set(RenderSet::PrepareBindGroups),
                );
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_render_command::<Opaque3d, DrawCustomMaterial>()
                .add_render_command::<Transparent3d, DrawCustomMaterial>()
                .add_render_command::<AlphaMask3d, DrawCustomMaterial>();
        }
    }
}


// impl Plugin for CustomRenderPlugin {
//     fn build(&self, app: &mut App) {
//         // Add extract component plugin
//         app.add_plugins(ExtractComponentPlugin::<CustomMaterialInstance>::default());

//         if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
//             render_app
//                 .init_resource::<SpecializedMeshPipelines<CustomMaterialPipeline>>()
//                 .add_render_command::<Opaque3d, DrawCustomMaterial>()
//                 .add_systems(
//                     Render,
//                     queue_custom_material_meshes.in_set(RenderSet::Queue),
//                 );
//         }
//     }

//     fn finish(&self, app: &mut App) {
//         if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
//             render_app.init_resource::<CustomMaterialPipeline>();
//         }
//     }
// }


// Marker for GLTF entities that need the relativistic material
#[derive(Component)]
pub struct NeedsRelativisticMaterial;


/// A map of entity IDs to their relativistic material handles.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct RelativisticMatLookup(pub HashMap<String, Handle<CustomMaterialInstance>>);




fn swap_to_relativistic_material(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    q_children: Query<&Children>,
    q_std_mat: Query<(&MeshMaterial3d<StandardMaterial>,)>,
    q_to_rel: Query<Entity, With<NeedsRelativisticMaterial>>,
    mut rel_mats: ResMut<Assets<CustomMaterialInstance>>,
    std_mats: Res<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut rel_mat_lookup: ResMut<RelativisticMatLookup>,
) {
    let ent = trigger.target();
    // ensure NeedsRelativisticMaterial and get the entity commands.
    if !q_to_rel.contains(ent) { return; }

    let mut mats_created = 0;
    // let mut rel_unis = OnceCell::new();

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
            let rel_mat = rel_mats.add(CustomMaterialInstance::new(
                c_tex_img.clone(),
                uv_texture,
                ir_texture,
            ));
            rel_mat
        });

        commands.entity(child)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((
                Aabb::from_min_max(Vec3::splat(-10000.0), Vec3::splat(10000.0)), // Set a large AABB to avoid culling issues
                // UseRelativisticPipeline,
            ));
        mats_created += 1;
        // let _ = rel_unis.set(RelativisticObject::from(rel_mat.clone()));
    }

    commands.entity(ent).remove::<NeedsRelativisticMaterial>()
        // .insert((rel_unis.take().unwrap(),))
        ;
    // info!("Swapped {} materials to RelativisticMaterial for entity {:?}", mats_created, ent);
}


#[derive(Clone, Copy)]
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

// Global uniforms - shared across all instances
#[derive(Resource, Clone, Default, ShaderType)]
pub struct GlobalCustomUniforms {
    pub time_scale: f32,
    pub global_intensity: f32,
    pub wind_direction: Vec3,
    pub _padding: f32, // Ensure 16-byte alignment
}

// Per-instance data component
#[derive(Asset, TypePath, Component, Clone, Default)]
#[require(VisibilityClass)]
#[component(on_add = view::add_visibility_class::<CustomMaterialInstance>)]
pub struct CustomMaterialInstance {
    pub velocity: Vec3,
    pub start_time: f32,
    pub base_texture: Handle<Image>,
    pub texture2: Handle<Image>,
    pub texture3: Handle<Image>,
}

impl CustomMaterialInstance {
    fn new(base_texture: Handle<Image>, uv_texture: Handle<Image>, ir_texture: Handle<Image>) -> Self {
        Self {
            velocity: Vec3::ZERO,
            start_time: 0.0,
            base_texture,
            texture2: uv_texture,
            texture3: ir_texture,
        }
    }
}

impl ExtractComponent for CustomMaterialInstance {
    type QueryData = &'static CustomMaterialInstance;
    type QueryFilter = ();
    type Out = CustomMaterialInstance;

    fn extract_component(item: QueryItem<'_, Self::QueryData>) -> Option<Self::Out> {
        Some(item.clone())
    }
}

// Instance uniform data for GPU
#[repr(C)]
#[derive(Clone, Copy, ShaderType, Component, NoUninit)]
pub struct CustomInstanceUniform {
    pub velocity: Vec3,
    pub start_time: f32,
}

// Buffer wrapper for global uniforms
#[derive(Resource, Default)]
pub struct GlobalCustomUniformBuffer {
    pub buffer: UniformBuffer<GlobalCustomUniforms>,
}

// Global bind group resource
#[derive(Resource)]
pub struct GlobalCustomBindGroup {
    pub value: BindGroup,
}

// Instance texture bind group component
#[derive(Component)]
pub struct CustomInstanceTextureBindGroup {
    pub value: BindGroup,
}

// Pipeline resource
#[derive(Resource)]
pub struct CustomMaterialPipeline {
    pub mesh_pipeline: MeshPipeline,
    pub global_layout: BindGroupLayout,
    pub instance_layout: BindGroupLayout,
    pub texture_layout: BindGroupLayout,
    pub shader_handle: Handle<Shader>,
}

impl FromWorld for CustomMaterialPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let mesh_pipeline = world.resource::<MeshPipeline>().clone();

        let global_layout = render_device.create_bind_group_layout(
            "custom_global_layout",
            &BindGroupLayoutEntries::single(
                ShaderStages::VERTEX_FRAGMENT,
                uniform_buffer::<GlobalCustomUniforms>(false),
            ),
        );

        let instance_layout = render_device.create_bind_group_layout(
            "custom_instance_layout",
            &BindGroupLayoutEntries::single(
                ShaderStages::VERTEX_FRAGMENT,
                uniform_buffer::<CustomInstanceUniform>(true),
            ),
        );

        let texture_layout = render_device.create_bind_group_layout(
            "custom_texture_layout",
            &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        );

        Self {
            mesh_pipeline,
            global_layout,
            instance_layout,
            texture_layout,
            shader_handle: CUSTOM_MATERIAL_SHADER_HANDLE,
        }
    }
}

impl SpecializedMeshPipeline for CustomMaterialPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;

        // Override with our shader and layouts
        descriptor.label = Some("custom_material_pipeline".into());
        descriptor.vertex.shader = self.shader_handle.clone();
        descriptor.fragment = Some(FragmentState {
            shader: self.shader_handle.clone(),
            targets: vec![Some(ColorTargetState {
                format: if key.contains(MeshPipelineKey::HDR) {
                    ViewTarget::TEXTURE_FORMAT_HDR
                } else {
                    TextureFormat::bevy_default()
                },
                blend: if key.contains(MeshPipelineKey::BLEND_ALPHA) {
                    Some(BlendState::ALPHA_BLENDING)
                } else {
                    None
                },
                write_mask: ColorWrites::ALL,
            })],
            entry_point: "fragment".into(),
            shader_defs: vec![],
        });

        // Set our custom bind group layouts
        let view_layout = self
            .mesh_pipeline
            .get_view_layout(MeshPipelineViewLayoutKey::from(key));

        descriptor.layout = vec![
            self.mesh_pipeline.get_view_layout(key.into()).clone(), // Group 0: View
            // view_layout.main_layout.clone(),         // Group 0: View
            self.global_layout.clone(),              // Group 1: Global uniforms
            self.mesh_pipeline.mesh_layouts.model_only.clone(), // Group 2: Mesh transform
            self.instance_layout.clone(),            // Group 3: Instance uniforms
            self.texture_layout.clone(),             // Group 4: Instance textures
        ];

        Ok(descriptor)
    }
}

// Render command sequence
type DrawCustomMaterial = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetGlobalCustomBindGroup<1>,
    SetMeshBindGroup<2>,
    SetInstanceCustomBindGroup<3>,
    SetInstanceTextureBindGroup<4>,
    DrawMesh,
);

// Custom render commands
pub struct SetGlobalCustomBindGroup<const I: usize>;
impl<const I: usize, P: PhaseItem> RenderCommand<P> for SetGlobalCustomBindGroup<I> {
    type ViewQuery = ();
    type ItemQuery = ();
    type Param = SRes<GlobalCustomBindGroup>;

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        bind_group: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(I, &bind_group.into_inner().value, &[]);
        RenderCommandResult::Success
    }
}

// Add this component to store per-instance uniform bind groups
#[derive(Component)]
pub struct CustomInstanceBindGroup {
    pub bind_group: BindGroup,
}

// Fixed render command
pub struct SetInstanceCustomBindGroup<const I: usize>;
impl<const I: usize, P: PhaseItem> RenderCommand<P> for SetInstanceCustomBindGroup<I> {
    type ViewQuery = ();
    type ItemQuery = Read<CustomInstanceBindGroup>;
    type Param = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        bind_group_query: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(instance_bind_group) = bind_group_query else {
            return RenderCommandResult::Skip;
        };

        pass.set_bind_group(I, &instance_bind_group.bind_group, &[]);
        RenderCommandResult::Success
    }
}

// Create bind groups during preparation
fn prepare_instance_bind_groups(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<CustomMaterialPipeline>,
    instances: Query<(Entity, &CustomMaterialInstance)>,
) {
    for (entity, instance) in instances.iter() {
        let uniform = CustomInstanceUniform {
            velocity: instance.velocity,
            start_time: instance.start_time,
        };

        let uniform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("custom_instance_uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = render_device.create_bind_group(
            "custom_instance_bind_group",
            &pipeline.instance_layout,
            &BindGroupEntries::single(uniform_buffer.as_entire_binding()),
        );

        commands.entity(entity).insert(CustomInstanceBindGroup { bind_group });
    }
}


pub struct SetInstanceTextureBindGroup<const I: usize>;
impl<const I: usize, P: PhaseItem> RenderCommand<P> for SetInstanceTextureBindGroup<I> {
    type ViewQuery = ();
    type ItemQuery = Read<CustomInstanceTextureBindGroup>;
    type Param = ();

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, Self::ViewQuery>,
        texture_bind_group: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(texture_bind_group) = texture_bind_group else {
            return RenderCommandResult::Failure("No texture bind group");
        };

        pass.set_bind_group(I, &texture_bind_group.value, &[]);
        RenderCommandResult::Success
    }
}



// System implementations
fn extract_global_uniforms(
    mut commands: Commands,
    globals: Extract<Res<GlobalCustomUniforms>>,
) {
    commands.insert_resource(globals.clone());
}

fn prepare_global_uniforms(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut global_buffer: ResMut<GlobalCustomUniformBuffer>,
    globals: Res<GlobalCustomUniforms>,
) {
    *global_buffer.buffer.get_mut() = globals.clone();
    global_buffer
        .buffer
        .write_buffer(&render_device, &render_queue);
}

fn prepare_global_bind_group(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<CustomMaterialPipeline>,
    global_buffer: Res<GlobalCustomUniformBuffer>,
) {
    if let Some(binding) = global_buffer.buffer.binding() {
        let bind_group = render_device.create_bind_group(
            "global_custom_bind_group",
            &pipeline.global_layout,
            &BindGroupEntries::single(binding),
        );
        commands.insert_resource(GlobalCustomBindGroup { value: bind_group });
    }
}

fn prepare_instance_uniforms(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline: Res<CustomMaterialPipeline>,
    fallback_image: Res<FallbackImage>,
    images: Res<RenderAssets<GpuImage>>,
    instances: Query<(Entity, &CustomMaterialInstance)>,
) {
    for (entity, instance) in instances.iter() {
        // Create texture bind group for this instance
        let texture1_view = images
            .get(&instance.base_texture)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view);

        let texture2_view = images
            .get(&instance.texture2)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view);

        let texture3_view = images
            .get(&instance.texture3)
            .map(|img| &img.texture_view)
            .unwrap_or(&fallback_image.d2.texture_view);

        let texture_bind_group = render_device.create_bind_group(
            "custom_instance_texture_bind_group",
            &pipeline.texture_layout,
            &[
                bge_tex(0, &texture1_view),
                bge_tex(1, &texture2_view),
                bge_tex(2, &texture3_view),

                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&fallback_image.d2.sampler),
                },
            ],
        );

        commands
            .entity(entity)
            .insert(CustomInstanceTextureBindGroup {
                value: texture_bind_group,
            });
    }
}


fn bge_tex(slot: u32, texture: &TextureView) -> BindGroupEntry {
    BindGroupEntry {
        binding: slot,
        resource: BindingResource::TextureView(texture),
    }
}



fn queue_custom_material_meshes(
    pipeline_cache: Res<PipelineCache>,
    custom_pipeline: Res<CustomMaterialPipeline>,
    mut opaque_render_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    opaque_draw_functions: Res<DrawFunctions<Opaque3d>>,
    mut specialized_mesh_pipelines: ResMut<SpecializedMeshPipelines<CustomMaterialPipeline>>,
    views: Query<(&ExtractedView, &RenderVisibleEntities, &Msaa)>,
    (render_meshes, render_mesh_instances): (Res<RenderAssets<RenderMesh>>, Res<RenderMeshInstances>),
    param: StaticSystemParam<<MeshPipeline as GetBatchData>::Param>,
    // Add GPU preprocessing support
    mut phase_batched_instance_buffers: ResMut<PhaseBatchedInstanceBuffers<Opaque3d, <MeshPipeline as GetBatchData>::BufferData>>,
    mut phase_indirect_parameters_buffers: ResMut<PhaseIndirectParametersBuffers<Opaque3d>>,
) {
    let draw_function_id = opaque_draw_functions.read().id::<DrawCustomMaterial>();

    for (view, visible_entities, msaa) in &views {
        let Some(opaque_phase) = opaque_render_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };

        let view_key = MeshPipelineKey::from_msaa_samples(msaa.samples())
            | MeshPipelineKey::from_hdr(view.hdr);

        for &(render_entity, visible_entity) in visible_entities.get::<CustomMaterialInstance>().iter() {
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(visible_entity) else {
                continue;
            };
            let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
                continue;
            };

            let mut mesh_key = view_key;
            mesh_key |= MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());

            let pipeline_id = specialized_mesh_pipelines.specialize(
                &pipeline_cache,
                &custom_pipeline,
                mesh_key,
                &mesh.layout,
            );

            let pipeline_id = match pipeline_id {
                Ok(id) => id,
                Err(_) => continue,
            };

            opaque_phase.add(
                Opaque3dBatchSetKey {
                    draw_function: draw_function_id,
                    pipeline: pipeline_id,
                    material_bind_group_index: None,
                    vertex_slab: Default::default(),
                    index_slab: None,
                    lightmap_slab: None,
                },
                Opaque3dBinKey {
                    asset_id: mesh_instance.mesh_asset_id.into(),
                },
                (render_entity, visible_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::BatchableMesh,
                Default::default(),
            );
        }
    }
}


// #[allow(clippy::too_many_arguments)]
// fn queue_custom_material_meshes(
//     pipeline_cache: Res<PipelineCache>,
//     custom_pipeline: Res<CustomMaterialPipeline>,
//     mut opaque_render_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
//     mut alpha_mask_render_phases: ResMut<ViewBinnedRenderPhases<AlphaMask3d>>,
//     mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
//     mut specialized_pipelines: ResMut<SpecializedMeshPipelines<CustomMaterialPipeline>>,
//     (
//         opaque_draw_functions,
//         alpha_mask_draw_functions,
//         transparent_draw_functions,
//         render_meshes,
//         render_mesh_instances,
//     ): (
//         Res<DrawFunctions<Opaque3d>>,
//         Res<DrawFunctions<AlphaMask3d>>,
//         Res<DrawFunctions<Transparent3d>>,
//         Res<RenderAssets<RenderMesh>>,
//         Res<RenderMeshInstances>,
//     ),
//     views: Query<(
//         &ExtractedView,
//         &RenderVisibleEntities,
//         &Msaa,
//         Has<NoIndirectDrawing>,
//     )>,
//     param: StaticSystemParam<<MeshPipeline as GetBatchData>::Param>,
//     mut phase_batched_instance_buffers: ResMut<
//         PhaseBatchedInstanceBuffers<Opaque3d, <MeshPipeline as GetBatchData>::BufferData>,
//     >,
//     mut phase_indirect_parameters_buffers: ResMut<PhaseIndirectParametersBuffers<Opaque3d>>,
//     mut change_tick: Local<Tick>,
// ) {
//     let system_param_item = param.into_inner();

//     let UntypedPhaseBatchedInstanceBuffers {
//         ref mut data_buffer,
//         ref mut work_item_buffers,
//         ref mut late_indexed_indirect_parameters_buffer,
//         ref mut late_non_indexed_indirect_parameters_buffer,
//         ..
//     } = phase_batched_instance_buffers.buffers;

//     let opaque_draw_function = opaque_draw_functions.read().id::<DrawCustomMaterial>();
//     let alpha_mask_draw_function = alpha_mask_draw_functions.read().id::<DrawCustomMaterial>();
//     let transparent_draw_function = transparent_draw_functions.read().id::<DrawCustomMaterial>();

//     for (view, visible_entities, msaa, no_indirect_drawing) in views.iter() {
//         let Some(opaque_phase) = opaque_render_phases.get_mut(&view.retained_view_entity) else {
//             continue;
//         };
//         let Some(alpha_mask_phase) = alpha_mask_render_phases.get_mut(&view.retained_view_entity)
//         else {
//             continue;
//         };
//         let Some(transparent_phase) = transparent_render_phases.get_mut(&view.retained_view_entity)
//         else {
//             continue;
//         };

//         let work_item_buffer = get_or_create_work_item_buffer::<Opaque3d>(
//             work_item_buffers,
//             view.retained_view_entity,
//             no_indirect_drawing,
//             false, // No occlusion culling for simplicity
//         );

//         init_work_item_buffers(
//             work_item_buffer,
//             late_indexed_indirect_parameters_buffer,
//             late_non_indexed_indirect_parameters_buffer,
//         );

//         let mut view_key = MeshPipelineKey::from_msaa_samples(msaa.samples())
//             | MeshPipelineKey::from_hdr(view.hdr);

//         let mut mesh_batch_set_info = None;

//         // Find visible entities with CustomMaterialInstance
//         for &(render_entity, visible_entity) in
//             visible_entities.get::<CustomMaterialInstance>().iter()
//         {
//             let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(visible_entity)
//             else {
//                 continue;
//             };

//             let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
//                 continue;
//             };

//             let mut mesh_key = view_key;
//             mesh_key |= MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());

//             if mesh_batch_set_info.is_none() {
//                 mesh_batch_set_info = Some(MeshBatchSetInfo {
//                     indirect_parameters_index: phase_indirect_parameters_buffers
//                         .buffers
//                         .allocate(mesh.indexed(), 1),
//                     is_indexed: mesh.indexed(),
//                 });
//             }
//             let mesh_info = mesh_batch_set_info.unwrap();

//             let Some(input_index) =
//                 MeshPipeline::get_binned_index(&system_param_item, visible_entity)
//             else {
//                 continue;
//             };
//             let output_index = data_buffer.add() as u32;

//             let pipeline_id = specialized_pipelines.specialize(
//                 &pipeline_cache,
//                 &custom_pipeline,
//                 mesh_key,
//                 &mesh.layout,
//             );

//             let pipeline_id = match pipeline_id {
//                 Ok(id) => id,
//                 Err(_) => continue,
//             };

//             let next_change_tick = change_tick.get() + 1;
//             change_tick.set(next_change_tick);

//             // Add to opaque phase (you can add logic for transparency here)
//             opaque_phase.add(
//                 Opaque3dBatchSetKey {
//                     draw_function: opaque_draw_function,
//                     pipeline: pipeline_id,
//                     material_bind_group_index: None,
//                     vertex_slab: Default::default(),
//                     index_slab: None,
//                     lightmap_slab: None,
//                 },
//                 Opaque3dBinKey {
//                     asset_id: AssetId::<bevy::render::mesh::Mesh>::invalid().untyped(),
//                 },
//                 (render_entity, visible_entity),
//                 mesh_instance.current_uniform_index,
//                 BinnedRenderPhaseType::BatchableMesh,
//                 *change_tick,
//             );

//             work_item_buffer.push(
//                 mesh.indexed(),
//                 PreprocessWorkItem {
//                     input_index: input_index.into(),
//                     output_or_indirect_parameters_index: if no_indirect_drawing {
//                         output_index
//                     } else {
//                         mesh_info.indirect_parameters_index
//                     },
//                 },
//             );
//         }

//         if let Some(mesh_info) = mesh_batch_set_info {
//             phase_indirect_parameters_buffers
//                 .buffers
//                 .add_batch_set(mesh_info.is_indexed, mesh_info.indirect_parameters_index);
//         }
//     }
// }

#[derive(Clone, Copy)]
struct MeshBatchSetInfo {
    indirect_parameters_index: u32,
    is_indexed: bool,
}

// Bundle for easy spawning
#[derive(Bundle, Default)]
pub struct CustomMaterialBundle {
    pub material_instance: CustomMaterialInstance,
    pub mesh: Mesh3d,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

// Convenience functions
impl CustomMaterialBundle {
    pub fn new(
        mesh: Handle<bevy::render::mesh::Mesh>,
        velocity: Vec3,
        start_time: f32,
        texture1: Handle<Image>,
        texture2: Handle<Image>,
        texture3: Handle<Image>,
    ) -> Self {
        Self {
            material_instance: CustomMaterialInstance {
                velocity,
                start_time,
                base_texture: texture1,
                texture2,
                texture3,
            },
            mesh: Mesh3d(mesh),
            ..Default::default()
        }
    }
}
