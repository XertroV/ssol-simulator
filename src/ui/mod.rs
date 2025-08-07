use bevy::prelude::*;
use iyes_perf_ui::prelude::PerfUiDefaultEntries;

use crate::camera_switcher::FreeCamPerfUI;


pub struct InGameUiPlugin;

impl Plugin for InGameUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ui);
    }
}


fn setup_ui(mut commands: Commands) {
    commands.spawn((
        FreeCamPerfUI,
        PerfUiDefaultEntries::default(),
    ));
}
