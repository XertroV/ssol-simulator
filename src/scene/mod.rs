use bevy::{prelude::*, scene::SceneInstanceReady};

pub use calculated_data::*;

use crate::game_state::OrbParent;

mod calculated_data;

pub struct SceneCalcDataPlugin;

#[derive(Event)]
pub struct DoRecalcSceneData;

impl Plugin for SceneCalcDataPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CalculatedData>()
            .add_event::<DoRecalcSceneData>()
            .add_observer(calc_scene_data_on_ready)
            .add_observer(on_do_recalc_scene_data);
    }
}

fn calc_scene_data_on_ready(
    ready: Trigger<SceneInstanceReady>,
    mut calc_data: ResMut<CalculatedData>,
    q_orbs: Query<(&Transform,), With<OrbParent>>,
) {
    // only run for orb parents
    let Ok(orb_p) = q_orbs.get(ready.target()) else { return };
    calc_data.merge_orb(orb_p.0.translation);
}

fn on_do_recalc_scene_data(
    _t: Trigger<DoRecalcSceneData>,
    mut calc_data: ResMut<CalculatedData>,
    q_orbs: Query<(&Transform,), With<OrbParent>>,
) {
    calc_data.reset();
    for orb_p in q_orbs.iter() {
        calc_data.merge_orb(orb_p.0.translation);
    }
}
