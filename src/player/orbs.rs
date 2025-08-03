use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::{game_state::{self, Orb, OrbParent}, player::Player};


pub fn detect_orb_collisions(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    mut q_player: Query<(Entity, &mut Velocity), With<Player>>,
    q_orbs: Query<(Entity, &ChildOf), (With<ChildOf>, With<Orb>)>,
    mut q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
    time: Res<Time>,
) {
    let Ok(player) = q_player.single_mut() else {
        return;
    };
    for event in collision_events.read() {
        if let CollisionEvent::Started(ent1, ent2, _) = event {
            // info!("Collision detected: {:?} with {:?}", ent1, ent2);
            let (collided_obj, _) = match (*ent1 == player.0, *ent2 == player.0) {
                (true, false) => (ent2, ent1),
                (false, true) => (ent1, ent2),
                _ => continue, // Not a collision with the player
            };

            // did we hit an orb?
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
    }
}
