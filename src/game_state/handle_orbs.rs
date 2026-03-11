use bevy::prelude::*;

use crate::{
    audio::PlayOrbPickupSound,
    game_state::{GameState, GameWon, OrbPickedUp, OrbSplit, OrbParent, ShowWhiteArch},
    orb_curriculum::OrbId,
    scene_loader::WhiteFinishArch,
    ui::in_game::OrbUiUpdateEvent,
};


// Constants ported from GameState.cs
pub const ORB_SPEED_INC: f32 = 0.05;
pub const ORB_DECEL_RATE: f32 = 0.6;
pub const ORB_SPEED_DUR: f32 = 2.0;
pub const FINAL_MAX_SPEED: f32 = 0.99;
pub const NORM_PERCENT_SPEED: f32 = 0.625;

pub fn orb_picked_up(
    trigger: On<OrbPickedUp>,
    mut commands: Commands,
    mut state: ResMut<GameState>,
    q_orb_ids: Query<&OrbId, With<OrbParent>>,
) {
    let orb_entity = trigger.event().0;
    state.score += 1;
    if let Ok(orb_id) = q_orb_ids.get(orb_entity) {
        let (prev_player_time, prev_world_time) = state
            .orb_splits
            .last()
            .map(|split| (split.player_time, split.world_time))
            .unwrap_or((0.0, 0.0));
        let sequence_index = state.score;
        let player_time = state.player_time;
        let world_time = state.world_time;
        state.orb_splits.push(OrbSplit {
            sequence_index,
            orb_id: *orb_id,
            player_time,
            world_time,
            player_split_delta: player_time - prev_player_time,
            world_split_delta: world_time - prev_world_time,
        });
    }
    // info!("Score: {}", state.score);
    // Reset the timer and increase the speed multiplier.
    state.orb_speed_boost_timer = ORB_SPEED_DUR;
    state.speed_multiplier += ORB_SPEED_INC;
    state.speed_multiplier = state.speed_multiplier.min(FINAL_MAX_SPEED);
    // info!("Speed multiplier: {}", state.speed_multiplier);
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
        commands.trigger(GameWon);
        info!("All orbs ({}/{}) collected! You win!", state.score, state.nb_orbs);
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
