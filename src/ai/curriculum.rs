use bevy::prelude::*;

pub struct CurriculumPlugin;

impl Plugin for CurriculumPlugin {
    fn build(&self, _app: &mut App) {
        // CurriculumConfig is initialized in main.rs (needed even without AI mode)
    }
}
