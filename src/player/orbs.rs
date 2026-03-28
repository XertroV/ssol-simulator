use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::{
    game_state::{self, FinishReached, Orb, OrbParent},
    player::Player,
    scene_loader::{WhiteFinishArch, WhiteFinishArchSensor},
};

/// Helper function to set the visibility of a single orb entity (should be set on OrbParent entity)
#[allow(dead_code)]
pub fn set_orb_visibility(commands: &mut Commands, entity: Entity, visible: bool) {
    if visible {
        commands.entity(entity).insert(Visibility::Visible);
    } else {
        commands.entity(entity).insert(Visibility::Hidden);
    }
}


pub fn detect_orb_collisions(
    mut commands: Commands,
    mut collision_events: MessageReader<CollisionEvent>,
    mut q_player: Query<(Entity, &mut Velocity), With<Player>>,
    q_orbs: Query<(Entity, &ChildOf), (With<ChildOf>, With<Orb>)>,
    q_white_arch_sensor: Query<(Entity, &ChildOf), (With<ChildOf>, With<WhiteFinishArchSensor>, Without<Orb>)>,
    mut q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
    q_white_arch: Query<&Visibility, (With<WhiteFinishArch>, Without<OrbParent>)>,
    ghost_replay: Option<Res<crate::ghost::GhostReplayInput>>,
) {
    // During ghost verification replay, orb collection is handled by ghost_verify_sync_orbs
    // at the exact recorded frame indices. Skip collision-based orb detection to prevent
    // timing differences from causing game state divergence.
    let skip_orbs = ghost_replay.is_some();

    let Ok(player) = q_player.single_mut() else {
        return;
    };
    for event in collision_events.read() {
        if let CollisionEvent::Started(ent1, ent2, _) = event {
            let (collided_obj, _) = match (*ent1 == player.0, *ent2 == player.0) {
                (true, false) => (ent2, ent1),
                (false, true) => (ent1, ent2),
                _ => continue, // Not a collision with the player
            };

            // did we hit an orb?
            if !skip_orbs {
                if let Ok(orb_ent) = q_orbs.get(*collided_obj) {
                    let orb_p = orb_ent.1.parent();
                    // get the parent's visibility
                    let Ok(mut orb_p_vis) = q_orb_p_vis.get_mut(orb_p) else { return };
                    if *orb_p_vis == Visibility::Hidden {
                        continue; // Already picked up
                    }
                    // hide the orb parent and trigger orb pickup.
                    *orb_p_vis = Visibility::Hidden;
                    commands.trigger(game_state::OrbPickedUp(orb_p));
                    continue;
                }
            }
            if let Ok(_wa_ent) = q_white_arch_sensor.get(*collided_obj) {
                let Ok(white_arch_vis) = q_white_arch.single() else { return };
                // did we hit the white arch?
                if *white_arch_vis == Visibility::Visible {
                    commands.trigger(FinishReached);
                    info!("Player hit the white arch.");
                }
            }
        }
    }
}
