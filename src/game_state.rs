use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::{camera_switcher::{self, is_free_cam_mode}, key_mapping::KeyMapping, player::{self}, scene_loader, ui::in_game::{OrbUiData}};
pub use handle_orbs::*;

mod handle_orbs;

#[derive(Component)]
pub struct Orb;

#[derive(Component)]
pub struct OrbParent;

#[derive(Event)]
pub struct OrbPickedUp(pub Entity);

#[derive(Event)]
pub struct ShowWhiteArch;

#[derive(Event)]
pub enum GameStatePaused {
    /// The game state was paused by the camera.
    CameraPaused,
    /// The game state was paused by the player.
    PlayerPaused,
    /// The game state was unpaused.
    Unpaused,
}
impl GameStatePaused {
    pub fn is_paused(&self) -> bool {
        matches!(self, GameStatePaused::CameraPaused | GameStatePaused::PlayerPaused)
    }
}

pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameState>()
            .add_observer(orb_picked_up)
            .add_observer(on_show_white_arch)
            .add_systems(Startup, set_orb_count.after(scene_loader::setup_scene))
            .add_systems(Update, (
                // process_game_state_input.before(player::player_update_start),
                process_game_state_input.after(player::player_update_done),
            ));
    }
}

#[derive(Clone, Debug)]
pub struct PlayerPhysState {
    pub velocity: Vec3,
    pub position: Vec3,
    // pub rotation: Quat,
    // pub transform: Transform,
}

impl From<(&Velocity, &Transform)> for PlayerPhysState {
    fn from((velocity, transform): (&Velocity, &Transform)) -> Self {
        Self {
            velocity: velocity.linvel,
            position: transform.translation,
            // rotation: transform.rotation,
        }
    }
}



enum SavedState {
    None,
    HardPaused(GameState, PlayerPhysState),
    InFreeCam(GameState, PlayerPhysState),
}



#[derive(Resource, Clone, Debug)]
pub struct GameState {
    // todo: change this to be a custom enum that tracks the source of the movement freeze.
    /// For storing the state of player movement when pausing, in free-cam, etc.
    /// We just keep a complete copy of the GameState for simplicity.
    pub movement_frozen: Option<Box<(GameState, PlayerPhysState)>>,
    // pub movement_frozen: Box<SavedState>,
    pub is_hard_paused: bool,
    /// How many orbs the player has collected.
    pub score: u32,
    /// Total number of orbs in the map.
    pub nb_orbs: u32,
    /// The timer for the speed boost from collecting an orb.
    pub orb_speed_boost_timer: f32,
    /// The percentage of max speed the player can currently use.
    /// Corresponds to `pctOfSpdUsing` in the C# code.
    pub speed_multiplier: f32,

    pub player_velocity_vector: Vec3,
    pub player_speed: f32,
    pub speed_of_light: f32,
    /// The maximum speed the player can reach. Multiplied by speed_multiplier.
    pub max_player_speed: f32,
    /// Lorentz factor: (1.0 - v_sq / c_sq).sqrt(),
    pub lorentz_factor: f32,

    pub player_time: f32,
    pub world_time: f32,

    pub game_win: bool,

    // From MovementScripts returnGrowth stuff
    /// Default 1600
    pub start_speed_of_light: f32,
    /// Number of times we've collected an Orb
    pub t_step: u32,
    /// The target speed of light (increased when orbs are collected)
    pub sol_target: f32,
    pub sol_step: f32,
}

impl GameState {
    /// Returns true if the player movement should be unpaused by the player when control is resumed.
    pub(crate) fn should_unpause_movement(&self) -> bool {
        self.movement_frozen.is_some() && !self.is_hard_paused
    }

    pub(crate) fn has_cam_paused_player_movement(&self) -> bool {
        return self.movement_frozen.is_some() && !self.is_hard_paused;
        // match self.movement_frozen.as_ref() {
        //     Some((s, _)) => !s.is_hard_paused,
        //     None => false,
        // }
    }

    pub(crate) fn color_shift(&self) -> u32 {
        match self.game_win {
            true => 0,
            false => 1,
        }
    }

    pub fn speed_as_pct_of_light(&self) -> f32 {
        self.player_speed / self.speed_of_light
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            movement_frozen: None,
            is_hard_paused: false,
            score: 0,
            orb_speed_boost_timer: 0.0,
            speed_multiplier: NORM_PERCENT_SPEED,
            player_velocity_vector: Vec3::ZERO,
            player_speed: 0.0,
            speed_of_light: 200.0, // Default value from GameState.cs
            // speed_of_light: 40.0, // testing
            // max_player_speed: 40.0, // testing
            max_player_speed: 32.0, // Default value from GameState.cs
            lorentz_factor: 1.0,
            nb_orbs: 100,
            player_time: 0.0,
            world_time: 0.0,
            game_win: false,
            start_speed_of_light: 1600.0, // Default 1600
            t_step: 0, // returnGrowth called once on init -> increments this
            sol_target: 820.0,
            sol_step: 1.0,
        }
    }
}

impl Into<OrbUiData> for &GameState {
    fn into(self) -> OrbUiData {
        OrbUiData {
            orbs_collected: self.score,
            orbs_total: self.nb_orbs,
        }
    }
}

pub fn speed_boost_decay_system(mut state: ResMut<GameState>, time: Res<Time>) {
    if state.orb_speed_boost_timer > 0.0 {
        state.orb_speed_boost_timer -= time.delta_secs();
    } else {
        // If the timer is done, decay the speed multiplier.
        if !state.game_win && state.speed_multiplier > NORM_PERCENT_SPEED {
            state.speed_multiplier -= ORB_DECEL_RATE * time.delta_secs();
            state.speed_multiplier = state.speed_multiplier.max(NORM_PERCENT_SPEED);
        }
    }
}

pub fn reset_game_state(commands: &mut Commands, state: &mut GameState, q_orbs: &Query<(), With<OrbParent>>) {
    *state = GameState::default();
    state.nb_orbs = q_orbs.iter().count() as u32;
    return_growth(state);
    info!("Game state reset");
}

pub fn set_orb_count(
    mut state: ResMut<GameState>,
    q_orbs: Query<(), With<OrbParent>>,
) {
    state.nb_orbs = q_orbs.iter().count() as u32;
    info!("Set orb count to {}", state.nb_orbs);
}

fn process_game_state_input(
    mut commands: Commands,
    mut state: ResMut<GameState>,
    // player_ctrl: Res<player::PlayerCtrl>,
    input: Res<ButtonInput<KeyCode>>,
    keys: Res<KeyMapping>,
    _q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
    active_cam: Res<camera_switcher::ActiveCamera>,
    mut q_player: Query<(&mut Transform, &mut Velocity), With<player::Player>>,
) {
    if is_free_cam_mode(active_cam) {
        // If we are in free camera mode, we don't process game state input.
        return;
    }

    let Ok((mut p_transform, mut p_vel)) = q_player.single_mut() else { return };

    let soft_reset = input.just_pressed(keys.reset_game);
    let hard_pause_toggle = input.just_pressed(keys.pause_game)
        || (soft_reset && state.is_hard_paused);

    if hard_pause_toggle {
        let mut restore = None;
        if state.is_hard_paused {
            restore = state.movement_frozen.take();
        }
        match restore.map(|frozen| *frozen) {
            Some((saved_state, saved_phys_state)) => {
                // Unfreeze the game state and restore player movement.
                state.clone_from(&saved_state);
                state.is_hard_paused = false;
                p_vel.linvel = saved_phys_state.velocity;
                p_transform.translation = saved_phys_state.position;
                commands.trigger(GameStatePaused::Unpaused);
                info!("Game hard unpaused");
            }
            _ => {
                // Freeze the game state and player movement.
                let phys_state = PlayerPhysState::from((&*p_vel, &*p_transform));
                state.movement_frozen = Some(Box::new((state.clone(), phys_state)));
                state.is_hard_paused = true;
                p_vel.linvel = Vec3::ZERO;
                commands.trigger(GameStatePaused::PlayerPaused);
                info!("Game hard paused");
            },
        }
    }

    if soft_reset {
        commands.trigger(player::PlayerRespawnRequest);
        info!("Game soft reset.\nState: {:?}\nPlayer: {:?}", state, (p_transform, p_vel));
    }
}

pub fn is_not_hard_paused(state: Res<GameState>) -> bool {
    !state.is_hard_paused
}

pub fn is_hard_paused(state: Res<GameState>) -> bool {
    state.is_hard_paused
}
