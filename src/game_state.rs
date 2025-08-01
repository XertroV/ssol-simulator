use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

#[derive(Component)]
pub struct Orb;

#[derive(Event)]
pub struct OrbPickedUp(pub Entity);


pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameState>()
            .add_event::<OrbPickedUp>()
            .add_observer(orb_picked_up);
    }
}

#[derive(Resource)]
pub struct GameState {
    pub use_player_cam: bool,
    pub score: u32,
    pub orb_speed_boost_timer: f32,
    /// The percentage of max speed the player can currently use.
    /// Corresponds to `pctOfSpdUsing` in the C# code.
    pub speed_multiplier: f32,
    pub movement_frozen: bool,

    pub player_velocity_vector: Vec3,
    pub speed_of_light: f32,
    pub max_player_speed: f32,
    // technically this is the inverse of the Lorentz factor,
    pub inv_lorentz_factor: f32,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            use_player_cam: true,
            score: 0,
            orb_speed_boost_timer: 0.0,
            speed_multiplier: NORM_PERCENT_SPEED,
            movement_frozen: false,
            player_velocity_vector: Vec3::ZERO,
            speed_of_light: 200.0, // Default value from GameState.cs
            max_player_speed: 32.0, // Default value from GameState.cs
            inv_lorentz_factor: 1.0,
        }
    }
}

// Constants ported from GameState.cs
const ORB_SPEED_INC: f32 = 0.05;
const ORB_DECEL_RATE: f32 = 0.6;
const ORB_SPEED_DUR: f32 = 2.0;
const FINAL_MAX_SPEED: f32 = 0.99;
const NORM_PERCENT_SPEED: f32 = 0.625;

pub fn speed_boost_decay_system(mut state: ResMut<GameState>, time: Res<Time>) {
    if state.orb_speed_boost_timer > 0.0 {
        state.orb_speed_boost_timer -= time.delta_secs();
    } else {
        // If the timer is done, decay the speed multiplier.
        if state.speed_multiplier > NORM_PERCENT_SPEED {
            state.speed_multiplier -= ORB_DECEL_RATE * time.delta_secs();
            state.speed_multiplier = state.speed_multiplier.max(NORM_PERCENT_SPEED);
        }
    }
}

pub fn orb_picked_up(trigger: Trigger<OrbPickedUp>, mut state: ResMut<GameState>) {
    state.score += 1;
    info!("Score: {}", state.score);
    // Reset the timer and increase the speed multiplier.
    state.orb_speed_boost_timer = ORB_SPEED_DUR;
    state.speed_multiplier += ORB_SPEED_INC;
    state.speed_multiplier = state.speed_multiplier.min(FINAL_MAX_SPEED);
    info!("Speed multiplier: {}", state.speed_multiplier);
}
