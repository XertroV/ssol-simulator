// use bevy::{prelude::*, render::{render_resource::*, renderer::{RenderDevice, RenderQueue}, Extract, Render, RenderApp, RenderSet, extract_resource::ExtractResource}};
// use crate::{cam_extra::HasFov, game_state::GameState, player::Player, relativity::rel_pipeline::{RelativisticGlobalsBindGroup, RelativisticPipeline}};

// pub struct RelativisticGlobalsPlugin;

// impl Plugin for RelativisticGlobalsPlugin {
//     fn build(&self, app: &mut App) {
//         if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
//             render_app
//                 .init_resource::<RelativisticGlobalsBuffer>()
//                 .add_systems(ExtractSchedule, extract_relativistic_globals)
//                 .add_systems(Render, prepare_relativistic_globals.in_set(RenderSet::PrepareResources));
//         }
//     }
// }

// // This is the data that will be available to our shaders
// #[derive(ShaderType, Default, Clone)]
// pub struct RelativisticGlobalsUniform {
//     pub vpc: Vec4,
//     pub player_offset: Vec4,
//     pub spd_of_light: f32,
//     pub wrld_time: f32,
//     pub xyr: f32, // Aspect Ratio
//     pub xs: f32,  // FOV Scale
// }

// // This resource is extracted from the main world to the render world.
// #[derive(Resource, Default, Clone, ExtractResource)]
// pub struct RelativisticGlobals {
//     uniform: RelativisticGlobalsUniform,
// }

// // This is the GPU buffer that will hold the uniform data.
// #[derive(Resource, Default)]
// pub struct RelativisticGlobalsBuffer {
//     pub buffer: UniformBuffer<RelativisticGlobalsUniform>,
// }

// // System to extract the data from the main world.
// fn extract_relativistic_globals(
//     mut commands: Commands,
//     state: Extract<Res<GameState>>,
//     q_player: Extract<Query<&Transform, With<Player>>>,
//     q_camera: Extract<Query<(&Camera, &Projection), With<crate::player::PlayerCamera>>>,
//     windows: Extract<Query<&Window, With<bevy::window::PrimaryWindow>>>,
// ) {
//     let Ok(player_transform) = q_player.single() else { return };
//     let Ok(camera) = q_camera.single() else { return };
//     let Ok(window) = windows.single() else { return };

//     commands.insert_resource(RelativisticGlobals {
//         uniform: RelativisticGlobalsUniform {
//             vpc: (state.player_velocity_vector / state.speed_of_light).extend(0.0),
//             player_offset: player_transform.translation.extend(0.0),
//             spd_of_light: state.speed_of_light,
//             wrld_time: state.orb_speed_boost_timer,
//             xyr: window.width() / window.height(),
//             xs: (camera.1.get_fov() / 2.0).tan(),
//         },
//     });
// }

// // System to write the extracted data to the GPU buffer.
// fn prepare_relativistic_globals(
//     render_device: Res<RenderDevice>,
//     render_queue: Res<RenderQueue>,
//     mut buffer: ResMut<RelativisticGlobalsBuffer>,
//     pipeline: Res<RelativisticPipeline>,
//     mut commands: Commands,
//     globals: Res<RelativisticGlobals>,
// ) {
//     buffer.buffer.set(globals.uniform.clone());
//     buffer.buffer.write_buffer(&render_device, &render_queue);

//     let bind_group = render_device.create_bind_group(
//         Some("relativistic_globals_bind_group"),
//         &pipeline.relativistic_globals_layout,
//         &[BindGroupEntry {
//             binding: 0,
//             resource: buffer.buffer.binding().unwrap(),
//         }],
//     );
//     commands.insert_resource(RelativisticGlobalsBindGroup { bind_group });
// }
