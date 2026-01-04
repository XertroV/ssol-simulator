use bevy::{ecs::entity_disabling::Disabled, prelude::*};
use bevy_rapier3d::prelude::*;

use crate::{audio::PlayOrbPickupSound, game_state::{GameState, OrbPickedUp, ShowWhiteArch}, scene_loader::WhiteFinishArch, ui::in_game::OrbUiUpdateEvent};


// Constants ported from GameState.cs
pub const ORB_SPEED_INC: f32 = 0.05;
pub const ORB_DECEL_RATE: f32 = 0.6;
pub const ORB_SPEED_DUR: f32 = 2.0;
pub const FINAL_MAX_SPEED: f32 = 0.99;
pub const NORM_PERCENT_SPEED: f32 = 0.625;

pub fn orb_picked_up(_trigger: On<OrbPickedUp>, mut commands: Commands, mut state: ResMut<GameState>) {
    state.score += 1;
    info!("Score: {}", state.score);
    // Reset the timer and increase the speed multiplier.
    state.orb_speed_boost_timer = ORB_SPEED_DUR;
    state.speed_multiplier += ORB_SPEED_INC;
    state.speed_multiplier = state.speed_multiplier.min(FINAL_MAX_SPEED);
    info!("Speed multiplier: {}", state.speed_multiplier);
    commands.trigger(PlayOrbPickupSound::from(state.as_ref()));
    commands.trigger(OrbUiUpdateEvent::Orbs(state.as_ref().into()));

    return_growth(&mut state);

    //
    state.game_win = state.score >= state.nb_orbs;
    if state.game_win {
        // set arch active
        // set cursor visible and locked
        state.speed_multiplier = FINAL_MAX_SPEED;
        commands.trigger(ShowWhiteArch);
    }
}

/// Updates speed of light target/step and t_step based on the number of orbs in the map.
/// Called when an orb is collected and once on game (re)start.
/// The name comes from the MovementScripts.cs
pub fn return_growth(state: &mut GameState) {
    let power: f32 = 0.0 - state.t_step as f32 * 6.0 / state.nb_orbs as f32;
    state.sol_target = state.start_speed_of_light - 1.0 / (1.0 + (power).exp()) * (state.start_speed_of_light - state.max_player_speed);
    if state.t_step >= state.nb_orbs {
        state.sol_target = state.max_player_speed;
    }
    state.t_step += 1;
    state.sol_step = (state.speed_of_light - state.sol_target).abs() / 20.0;
}


pub fn on_show_white_arch(
    _t: On<ShowWhiteArch>,
    mut commands: Commands,
    q_white_arch: Query<Entity, With<WhiteFinishArch>>,
) {
    if let Ok(arch_entity) = q_white_arch.single() {
        commands.entity(arch_entity).insert(Visibility::Visible);
        info!("Showing white finish arch");
    } else {
        warn!("No white finish arch found to show");
    }
}


pub fn hide_white_arch(
    commands: &mut Commands,
    q_white_arch: Query<Entity, With<WhiteFinishArch>>,
) {
    if let Ok(arch_entity) = q_white_arch.single() {
        commands.entity(arch_entity).insert(Visibility::Hidden);
        // info!("Hiding white finish arch");
    } else {
        // warn!("No white finish arch found to hide");
    }
}
