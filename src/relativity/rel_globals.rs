
use bevy::{
    core_pipeline::core_3d::graph::{Core3d, Node3d}, ecs::query::QueryItem, prelude::*, render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin}, render_graph::{Node, NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner}, render_phase::TrackedRenderPass, render_resource::{binding_types::uniform_buffer, AsBindGroup, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, DynamicBindGroupLayoutEntries, RenderPassColorAttachment, RenderPassDescriptor, ShaderStages, ShaderType, UniformBuffer}, renderer::{RenderContext, RenderDevice, RenderQueue}, view::ViewTarget, Extract, Render, RenderApp, RenderSet
    }, window::PrimaryWindow
};

use crate::{cam_extra::HasFov, game_state::GameState, player::{Player, PlayerCamera}};

// The plugin that sets up our global relativistic uniform buffer.
pub struct RelativisticGlobalsPlugin;

impl Plugin for RelativisticGlobalsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RelativisticGlobals>()
            .add_systems(Update, update_relativistic_globals)
            .add_plugins(ExtractResourcePlugin::<GpuRelativisticGlobals>::default());

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_render_graph_node::<ViewNodeRunner<RelativisticNode>>(Core3d, RelativisticNodeLabel);
            render_app.add_render_graph_edges(
                Core3d,
                (
                    // From our custom node
                    RelativisticNodeLabel,
                    // To the main PBR pass
                    Node3d::StartMainPass,
                ),
            );

            // render_app
                // .init_resource::<RelativisticGlobalsBuffer>()
                // .init_resource::<RelativisticGlobalsBindGroup>()
                // .add_systems(ExtractSchedule, extract_relativistic_globals)
                // .add_systems(
                //     Render,
                //     prepare_relativistic_globals.in_set(RenderSet::PrepareResources),
                // );

            render_app.init_resource::<RelativisticUniforms>()
                .add_systems(
                    Render,
                    (
                        prepare_relativistic_uniforms.in_set(RenderSet::Prepare),
                        queue_relativistic_bind_group.in_set(RenderSet::Queue),
                    ),
                );
        }
    }
}



// Main World Data
#[derive(Resource, Default, Clone, ShaderType)]
pub struct RelativisticGlobals {
    pub vpc: Vec4, // velocity of player
    pub player_offset: Vec4,
    pub spd_of_light: f32,
    pub wrld_time: f32,
    pub color_shift: u32, // 1 = enabled
    pub xyr: f32, // x/y ratio for the camera
    pub xs: f32, // x scale (tangent of half FOV)
}

fn update_relativistic_globals(
    mut globals: ResMut<RelativisticGlobals>,
    state: Res<GameState>,
    player: Query<&Transform, With<Player>>,
    // We still need camera/window info for xyr and xs
    q_camera: Query<(&Camera, &Projection), With<PlayerCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(camera) = q_camera.single() else { return };
    let Ok(window) = windows.single() else { return };
    let Ok(player) = player.single() else { return };
    globals.vpc = (state.player_velocity_vector / state.speed_of_light).extend(0.0);
    globals.player_offset = player.translation.extend(0.0);
    globals.spd_of_light = state.speed_of_light;
    globals.wrld_time = state.world_time;
    globals.color_shift = 1; // Enable color shift
    globals.xyr = window.width() / window.height();
    globals.xs = (camera.1.get_fov() / 2.0).tan();
}


// Render World Data and Systems
#[derive(Resource, Default, Clone, ShaderType)]
pub struct GpuRelativisticGlobals {
    pub vpc: Vec4, // velocity of player
    pub player_offset: Vec4,
    pub spd_of_light: f32,
    pub wrld_time: f32,
    pub color_shift: u32, // 1 = enabled
    pub xyr: f32, // x/y ratio for the camera
    pub xs: f32, // x scale (tangent of half FOV)
}

impl ExtractResource for GpuRelativisticGlobals {
    type Source = RelativisticGlobals;
    fn extract_resource(source: &Self::Source) -> Self {
        source.clone().into()
    }
}
impl From<RelativisticGlobals> for GpuRelativisticGlobals {
    fn from(value: RelativisticGlobals) -> Self {
        Self {
            vpc: value.vpc,
            player_offset: value.player_offset,
            spd_of_light: value.spd_of_light,
            wrld_time: value.wrld_time,
            color_shift: value.color_shift,
            xyr: value.xyr,
            xs: value.xs,
        }
    }
}

#[derive(Resource, Default)]
struct RelativisticUniforms(UniformBuffer<GpuRelativisticGlobals>);

fn prepare_relativistic_uniforms(
    globals: Res<GpuRelativisticGlobals>,
    mut uniform_buffer: ResMut<RelativisticUniforms>,
    render_device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    uniform_buffer.0.set(globals.clone());
    uniform_buffer.0.write_buffer(&render_device, &queue);
}

#[derive(Resource)]
struct RelativisticBindGroup(BindGroup);

fn queue_relativistic_bind_group(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    uniforms: Res<RelativisticUniforms>, // Assuming this is your resource with the uniform data
) {
    let Some(bind_group) = uniforms.0.binding() else {
        warn!("Relativistic uniform buffer is not ready yet");
        return;
    };
    info!("Got relativistic uniform buffer");
    let entries = DynamicBindGroupLayoutEntries::new_with_indices(
        ShaderStages::VERTEX_FRAGMENT,
        ((99, uniform_buffer::<GpuRelativisticGlobals>(false).visibility(ShaderStages::VERTEX_FRAGMENT)),)
    );
    let layout = render_device.create_bind_group_layout(
        "relativistic_globals_layout",
        &entries
    );
    let bind_group = render_device.create_bind_group(
        "relativistic_globals_bind_group",
        &layout,
        &[BindGroupEntry {
            binding: 99,
            resource: bind_group,
        }]
    );

    commands.insert_resource(RelativisticBindGroup(bind_group));
}



#[derive(RenderLabel, Debug, Clone, Hash, PartialEq, Eq)]
pub struct RelativisticNodeLabel;


#[derive(Component, Default, Clone)]
pub struct ExtractedRelGlobals(pub GpuRelativisticGlobals);



#[derive(Default)]
pub struct RelativisticNode;

impl ViewNode for RelativisticNode {
    type ViewQuery = (
        &'static ViewTarget,
        // &'static ExtractedRelGlobals,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, ): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(bind_group_resource) = world.get_resource::<RelativisticBindGroup>() else {
            warn!("Relativistic bind group is not ready yet");
            return Ok(());
        };

        let mut tracked_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("RelativisticPass"),
                color_attachments: &[
                    Some(view_target.get_color_attachment())
                ],
                ..default()
            },
        );

        // Here's the magic!
        tracked_pass.set_bind_group(2, &bind_group_resource.0, &[]);

        Ok(())
    }
}
















// // This struct holds the data we'll send to the GPU.
// // It contains all the fields that are the same for every object.
// #[derive(Default, Clone, ShaderType, AsBindGroup)]
// pub struct RelativisticGlobalsUniform {
//     /// velocity of player
//     #[uniform(0)]
//     pub vpc: Vec4,
//     #[uniform(1)]
//     pub player_offset: Vec4,
//     #[uniform(2)]
//     pub spd_of_light: f32,
//     #[uniform(3)]
//     pub wrld_time: f32,
//     #[uniform(4)]
//     pub color_shift: u32,
//     #[uniform(5)]
//     pub xyr: f32, // xy ratio
//     #[uniform(6)]
//     pub xs: f32,  // x scale (tangent of half FOV)
// }

// // This resource holds the GPU buffer for our uniforms.
// #[derive(Resource, Default)]
// pub struct RelativisticGlobalsBuffer {
//     pub buffer: UniformBuffer<RelativisticGlobalsUniform>,
// }

// // This resource will hold the bind group we create from the buffer.
// #[derive(Resource, Default, Clone)]
// pub struct RelativisticGlobalsBindGroup {
//     pub bind_group: Option<BindGroup>,
// }

// // A resource to help us extract the player's transform.
// #[derive(Resource)]
// pub struct ExtractedPlayer(Transform);

// // The extract system runs every frame to copy data from the main world to the render world.
// fn extract_relativistic_globals(
//     mut commands: Commands,
//     state: Extract<Res<GameState>>,
//     q_player: Extract<Query<&Transform, With<Player>>>,
// ) {
//     if let Ok(transform) = q_player.single() {
//         // We insert the GameState and player transform as resources into the render world.
//         commands.insert_resource(state.clone());
//         commands.insert_resource(ExtractedPlayer(*transform));
//     }
// }

// // The prepare system runs in the render world to update the GPU buffer.
// fn prepare_relativistic_globals(
//     render_device: Res<RenderDevice>,
//     render_queue: Res<RenderQueue>,
//     mut globals_buffer: ResMut<RelativisticGlobalsBuffer>,
//     mut bind_group: ResMut<RelativisticGlobalsBindGroup>,
//     state: Res<GameState>,
//     player: Res<ExtractedPlayer>,
//     // We still need camera/window info for xyr and xs
//     q_camera: Query<(&Camera, &Projection), With<PlayerCamera>>,
//     windows: Query<&Window, With<PrimaryWindow>>,
// ) {
//     let Ok(camera) = q_camera.single() else { return };
//     let Ok(window) = windows.single() else { return };

//     // Fill the uniform struct with the latest data.
//     let uniform = globals_buffer.buffer.get_mut();
//     uniform.vpc = (state.player_velocity_vector / state.speed_of_light).extend(0.0);
//     uniform.player_offset = player.0.translation.extend(0.0);
//     uniform.spd_of_light = state.speed_of_light;
//     uniform.wrld_time = state.orb_speed_boost_timer; // Or another time value
//     uniform.color_shift = 1;
//     uniform.xyr = window.width() / window.height();
//     uniform.xs = (camera.1.get_fov() / 2.0).tan();

//     // Write the data to the GPU buffer.
//     globals_buffer.buffer.write_buffer(&render_device, &render_queue);

//     // Create the bind group from the buffer.
//     let entries = DynamicBindGroupLayoutEntries::new_with_indices(
//         ShaderStages::VERTEX_FRAGMENT,
//         ((0, uniform_buffer::<RelativisticGlobalsUniform>(false).visibility(ShaderStages::VERTEX_FRAGMENT)),)
//     );
//     let layouts = entries.to_vec();
//     let layout = render_device.create_bind_group_layout("relativistic_globals_layout", &layouts);

//     bind_group.bind_group = Some(render_device.create_bind_group(
//         "relativistic_globals_bind_group",
//         &layout,
//         &[BindGroupEntry {
//             binding: 0,
//             resource: globals_buffer.buffer.binding().unwrap(),
//         }],
//     ));
// }
