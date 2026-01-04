//! ZMQ Bridge for Python AI Training
//!
//! Provides a ZMQ REP socket server that communicates with Python training scripts
//! using MessagePack serialization. Runs on a dedicated thread and communicates
//! with Bevy via channels.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use bevy::prelude::*;

use super::{AiActionInput, AiConfig, AiEpisodeControl, AiObservations, AiRewardSignal, CurriculumConfig};

// ============================================================================
// Message Types for ZMQ Communication
// ============================================================================

/// Messages sent from Python client to Rust server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Request to reset the episode and get initial observation
    Reset {
        /// Optional reason for the reset (for logging)
        #[serde(default)]
        reason: Option<String>,
    },
    /// Apply an action and step the simulation
    Step { action: ActionData },
    /// Get current observation without stepping
    GetObservation,
    /// Update curriculum settings
    SetCurriculum { orb_spawn_radius: Option<f32>, max_orbs: Option<u32> },
    /// Graceful shutdown request
    Close,
}

/// Action data sent from Python
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionData {
    /// [pitch_delta, yaw_delta] in radians
    pub look: [f32; 2],
    /// [forward/back, left/right] in [-1, 1]
    pub move_dir: [f32; 2],
}

/// Observation data sent to Python
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationData {
    pub orb_checklist: Vec<f32>,
    pub player_position: [f32; 3],
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub player_velocity_local: [f32; 3],
    pub player_velocity_world: [f32; 3],
    pub speed_of_light_ratio: f32,
    pub combo_timer: f32,
    pub speed_multiplier: f32,
    pub wall_rays: Vec<f32>,
    /// [[dir_x, dir_y, dir_z, distance, orb_id]; 10]
    pub orb_targets: Vec<[f32; 5]>,
}

impl From<&AiObservations> for ObservationData {
    fn from(obs: &AiObservations) -> Self {
        Self {
            orb_checklist: obs.orb_checklist.to_vec(),
            player_position: obs.player_position.to_array(),
            camera_yaw: obs.camera_yaw,
            camera_pitch: obs.camera_pitch,
            player_velocity_local: obs.player_velocity_local.to_array(),
            player_velocity_world: obs.player_velocity_world.to_array(),
            speed_of_light_ratio: obs.speed_of_light_ratio,
            combo_timer: obs.combo_timer,
            speed_multiplier: obs.speed_multiplier,
            wall_rays: obs.wall_rays.to_vec(),
            orb_targets: obs
                .orb_targets
                .iter()
                .map(|(dir, dist, id)| [dir.x, dir.y, dir.z, *dist, *id])
                .collect(),
        }
    }
}

/// Response to Reset message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetResponse {
    pub observation: ObservationData,
    pub info: HashMap<String, f32>,
}

/// Response to Step message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResponse {
    pub observation: ObservationData,
    pub reward: f32,
    pub terminated: bool,
    pub truncated: bool,
    pub info: HashMap<String, f32>,
}

/// Response to GetObservation message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationResponse {
    pub observation: ObservationData,
}

/// Response sent back to Python (tagged enum for MessagePack)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerResponse {
    Reset(ResetResponse),
    Step(StepResponse),
    Observation(ObservationResponse),
    CurriculumUpdated,
    Closed,
    Error { message: String },
}

// ============================================================================
// Channel Messages (Bevy <-> ZMQ Thread)
// ============================================================================

/// Commands sent from ZMQ thread to Bevy
#[derive(Debug)]
pub enum BridgeCommand {
    Reset {
        reason: Option<String>,
    },
    Step(ActionData),
    GetObservation,
    SetCurriculum { orb_spawn_radius: Option<f32>, max_orbs: Option<u32> },
    Close,
}

/// Responses sent from Bevy to ZMQ thread
#[derive(Debug)]
pub enum BridgeResponse {
    Reset {
        observation: ObservationData,
        info: HashMap<String, f32>,
    },
    Step {
        observation: ObservationData,
        reward: f32,
        terminated: bool,
        truncated: bool,
        info: HashMap<String, f32>,
    },
    Observation {
        observation: ObservationData,
    },
    CurriculumUpdated,
    Closed,
    Error {
        message: String,
    },
}

// ============================================================================
// Bridge Resources
// ============================================================================

/// Resource holding the communication channels for the bridge
#[derive(Resource)]
pub struct BridgeChannels {
    /// Receives commands from ZMQ thread (wrapped in Mutex for Sync)
    pub command_rx: Mutex<Receiver<BridgeCommand>>,
    /// Sends responses to ZMQ thread
    pub response_tx: Mutex<Sender<BridgeResponse>>,
    /// Handle to ZMQ thread for cleanup
    pub thread_handle: Mutex<Option<JoinHandle<()>>>,
}

/// Resource tracking pending bridge state
#[derive(Resource, Default)]
pub struct BridgePendingState {
    /// Action from a Step command waiting to be applied
    pub pending_action: Option<ActionData>,
    /// True when we need to wait for action_repeat ticks before responding
    pub awaiting_step_completion: bool,
    /// Accumulated reward during action_repeat period
    pub accumulated_reward: f32,
    /// Ticks remaining in current step
    pub step_ticks_remaining: u32,
}

// ============================================================================
// ZMQ Server Thread
// ============================================================================

/// Spawns the ZMQ server thread and returns the channel handles
pub fn spawn_bridge_thread(port: u16) -> BridgeChannels {
    let (cmd_tx, cmd_rx) = mpsc::channel::<BridgeCommand>();
    let (resp_tx, resp_rx) = mpsc::channel::<BridgeResponse>();

    let handle = thread::spawn(move || {
        run_zmq_server(port, cmd_tx, resp_rx);
    });

    BridgeChannels {
        command_rx: Mutex::new(cmd_rx),
        response_tx: Mutex::new(resp_tx),
        thread_handle: Mutex::new(Some(handle)),
    }
}

/// The main ZMQ server loop running on a dedicated thread
fn run_zmq_server(port: u16, cmd_tx: Sender<BridgeCommand>, resp_rx: Receiver<BridgeResponse>) {
    let ctx = zmq::Context::new();
    let socket = ctx.socket(zmq::REP).expect("Failed to create ZMQ REP socket");

    let address = format!("tcp://127.0.0.1:{}", port);
    socket.bind(&address).expect("Failed to bind ZMQ socket");

    info!("ZMQ Bridge listening on {}", address);

    loop {
        // Receive message from Python
        let msg = match socket.recv_bytes(0) {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("ZMQ recv error: {}", e);
                continue;
            }
        };

        // Deserialize MessagePack
        let client_msg: ClientMessage = match rmp_serde::from_slice(&msg) {
            Ok(m) => m,
            Err(e) => {
                error!("MessagePack decode error: {}", e);
                let error_response = ServerResponse::Error {
                    message: format!("Decode error: {}", e),
                };
                let response_bytes = rmp_serde::to_vec(&error_response).unwrap();
                let _ = socket.send(response_bytes, 0);
                continue;
            }
        };

        // Convert to bridge command
        let bridge_cmd = match &client_msg {
            ClientMessage::Reset { reason } => BridgeCommand::Reset { reason: reason.clone() },
            ClientMessage::Step { action } => BridgeCommand::Step(action.clone()),
            ClientMessage::GetObservation => BridgeCommand::GetObservation,
            ClientMessage::SetCurriculum { orb_spawn_radius, max_orbs } => BridgeCommand::SetCurriculum {
                orb_spawn_radius: *orb_spawn_radius,
                max_orbs: *max_orbs,
            },
            ClientMessage::Close => BridgeCommand::Close,
        };

        // Send to Bevy
        if cmd_tx.send(bridge_cmd).is_err() {
            error!("Failed to send command to Bevy - channel closed");
            break;
        }

        // Wait for response from Bevy
        let bridge_resp = match resp_rx.recv() {
            Ok(r) => r,
            Err(_) => {
                error!("Failed to receive response from Bevy - channel closed");
                break;
            }
        };

        // Convert to server response
        let server_resp = match bridge_resp {
            BridgeResponse::Reset { observation, info } => {
                ServerResponse::Reset(ResetResponse { observation, info })
            }
            BridgeResponse::Step {
                observation,
                reward,
                terminated,
                truncated,
                info,
            } => ServerResponse::Step(StepResponse {
                observation,
                reward,
                terminated,
                truncated,
                info,
            }),
            BridgeResponse::Observation { observation } => {
                ServerResponse::Observation(ObservationResponse { observation })
            }
            BridgeResponse::CurriculumUpdated => ServerResponse::CurriculumUpdated,
            BridgeResponse::Closed => ServerResponse::Closed,
            BridgeResponse::Error { message } => ServerResponse::Error { message },
        };

        // Serialize and send response
        let response_bytes = rmp_serde::to_vec(&server_resp).unwrap();
        if socket.send(response_bytes, 0).is_err() {
            error!("Failed to send ZMQ response");
            break;
        }

        // Exit loop on Close
        if matches!(client_msg, ClientMessage::Close) {
            info!("ZMQ Bridge received Close - shutting down");
            break;
        }
    }
}

// ============================================================================
// Bevy Plugin and Systems
// ============================================================================

/// Plugin that sets up the ZMQ bridge
pub struct BridgePlugin;

impl Plugin for BridgePlugin {
    fn build(&self, app: &mut App) {
        // Bridge is conditionally started based on SimConfig in the startup system
        app.init_resource::<BridgePendingState>()
            .add_systems(Startup, maybe_start_bridge.after(super::configure_ai_from_simconfig))
            .add_systems(
                FixedUpdate,
                (
                    process_bridge_commands,
                    complete_pending_step,
                )
                    .chain()
                    .after(super::increment_episode_tick),
            );
    }
}

/// Start the bridge if zmq_port is configured
fn maybe_start_bridge(mut commands: Commands, sim_config: Res<crate::SimConfig>) {
    if let Some(port) = sim_config.zmq_port {
        let instance_str = sim_config.instance_name.as_deref().unwrap_or("default");
        info!("[{}] Starting ZMQ bridge on port {}", instance_str, port);
        let channels = spawn_bridge_thread(port);
        commands.insert_resource(channels);
    }
}

/// Process incoming commands from the ZMQ bridge
fn process_bridge_commands(
    channels: Option<Res<BridgeChannels>>,
    mut pending_state: ResMut<BridgePendingState>,
    mut episode_control: ResMut<AiEpisodeControl>,
    mut ai_action: ResMut<AiActionInput>,
    mut curriculum: ResMut<CurriculumConfig>,
    ai_config: Res<AiConfig>,
    observations: Res<AiObservations>,
    _reward_signal: Res<AiRewardSignal>,
) {
    let Some(channels) = channels else {
        return;
    };

    // Don't process new commands while awaiting step completion
    if pending_state.awaiting_step_completion {
        return;
    }

    // Lock the command receiver
    let command_rx = channels.command_rx.lock().unwrap();

    // Try to receive a command (non-blocking)
    let cmd = match command_rx.try_recv() {
        Ok(c) => c,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => {
            error!("Bridge command channel disconnected");
            return;
        }
    };

    // Drop the lock before processing
    drop(command_rx);

    // Lock the response sender
    let response_tx = channels.response_tx.lock().unwrap();

    match cmd {
        BridgeCommand::Reset { reason } => {
            // Log the respawn reason if provided
            match &reason {
                Some(r) => info!("Reset requested: {}", r),
                None => info!("Reset requested (no reason provided)"),
            }

            // Request episode reset
            episode_control.request_reset();

            // Build response with current observation (will be updated next frame)
            let obs_data = ObservationData::from(observations.as_ref());
            let mut info = HashMap::new();
            info.insert("episode".to_string(), episode_control.episode_count as f32);

            let _ = response_tx.send(BridgeResponse::Reset {
                observation: obs_data,
                info,
            });
        }

        BridgeCommand::Step(action) => {
            // Apply action
            ai_action.look = Vec2::new(action.look[0], action.look[1]);
            ai_action.move_dir = Vec2::new(action.move_dir[0], action.move_dir[1]);

            // Set up pending step - don't send response yet, will be sent after action_repeat
            pending_state.pending_action = Some(action);
            pending_state.awaiting_step_completion = true;
            pending_state.accumulated_reward = 0.0;
            pending_state.step_ticks_remaining = ai_config.action_repeat;
        }

        BridgeCommand::GetObservation => {
            let obs_data = ObservationData::from(observations.as_ref());
            let _ = response_tx.send(BridgeResponse::Observation {
                observation: obs_data,
            });
        }

        BridgeCommand::SetCurriculum { orb_spawn_radius, max_orbs } => {
            if let Some(radius) = orb_spawn_radius {
                curriculum.orb_spawn_radius = Some(radius);
                info!("Curriculum updated: orb_spawn_radius = {}", radius);
            }
            if let Some(count) = max_orbs {
                curriculum.max_orbs = Some(count);
                info!("Curriculum updated: max_orbs = {}", count);
            }
            let _ = response_tx.send(BridgeResponse::CurriculumUpdated);
        }

        BridgeCommand::Close => {
            let _ = response_tx.send(BridgeResponse::Closed);
        }
    }
}

/// Complete pending step after action_repeat ticks
fn complete_pending_step(
    channels: Option<Res<BridgeChannels>>,
    mut pending_state: ResMut<BridgePendingState>,
    observations: Res<AiObservations>,
    reward_signal: Res<AiRewardSignal>,
    episode_control: Res<AiEpisodeControl>,
) {
    let Some(channels) = channels else {
        return;
    };

    if !pending_state.awaiting_step_completion {
        return;
    }

    // Accumulate reward
    pending_state.accumulated_reward += reward_signal.step_reward;

    // Decrement tick counter
    if pending_state.step_ticks_remaining > 0 {
        pending_state.step_ticks_remaining -= 1;
    }

    // Check if step is complete or episode ended early
    let step_complete = pending_state.step_ticks_remaining == 0;
    let episode_ended = reward_signal.terminated || reward_signal.truncated;

    if step_complete || episode_ended {
        let obs_data = ObservationData::from(observations.as_ref());
        let mut info = HashMap::new();
        info.insert("episode_ticks".to_string(), episode_control.episode_ticks as f32);

        // Lock response sender and send
        let response_tx = channels.response_tx.lock().unwrap();
        let _ = response_tx.send(BridgeResponse::Step {
            observation: obs_data,
            reward: pending_state.accumulated_reward,
            terminated: reward_signal.terminated,
            truncated: reward_signal.truncated,
            info,
        });

        // Clear pending state
        pending_state.pending_action = None;
        pending_state.awaiting_step_completion = false;
        pending_state.accumulated_reward = 0.0;
    }
}
