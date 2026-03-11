use std::{
    fs,
    path::{Path, PathBuf},
};

use bevy::{
    prelude::*,
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};
use bevy_rapier3d::render::DebugRenderContext;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{
    audio::AudioSettings,
    key_mapping::{KeyAction, KeyMapping},
    player::MovementSettings,
    ui::{PauseMenuState, is_pause_menu_open},
};

const CONFIG_FILE_NAME: &str = "settings.json";

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                process_global_setting_shortcuts,
                apply_window_settings,
                apply_physics_gizmo_settings,
                sync_config_from_runtime,
            ),
        );
    }
}

#[derive(Resource, Debug, Clone)]
pub struct AppConfigStore {
    pub path: PathBuf,
    pub config: AppConfig,
}

impl AppConfigStore {
    pub fn load() -> Self {
        let path = default_config_path();
        match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<AppConfig>(&contents) {
                Ok(config) => Self { path, config },
                Err(error) => {
                    warn!(
                        "Failed to parse config at {}: {}. Using defaults.",
                        path.display(),
                        error
                    );
                    let store = Self {
                        path,
                        config: AppConfig::default(),
                    };
                    store.save().ok();
                    store
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let store = Self {
                    path,
                    config: AppConfig::default(),
                };
                store.save().ok();
                store
            }
            Err(error) => {
                warn!(
                    "Failed to read config at {}: {}. Using defaults.",
                    path.display(),
                    error
                );
                Self {
                    path,
                    config: AppConfig::default(),
                }
            }
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(&self.config)
            .map_err(std::io::Error::other)?;
        fs::write(&self.path, contents)
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    pub fullscreen: bool,
    pub vsync_enabled: bool,
    pub show_perf_hud: bool,
    pub show_physics_gizmos: bool,
    pub desaturation_enabled: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync_enabled: false,
            show_perf_hud: true,
            show_physics_gizmos: false,
            desaturation_enabled: false,
        }
    }
}

impl GraphicsSettings {
    pub fn window_mode(&self) -> WindowMode {
        match self.fullscreen {
            true => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            false => WindowMode::Windowed,
        }
    }

    pub fn present_mode(&self) -> PresentMode {
        match self.vsync_enabled {
            true => PresentMode::AutoVsync,
            false => PresentMode::AutoNoVsync,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub key_mapping: KeyMapping,
    pub movement: MovementSettings,
    pub audio: AudioSettings,
    pub graphics: GraphicsSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            key_mapping: KeyMapping::default(),
            movement: MovementSettings::default(),
            audio: AudioSettings::default(),
            graphics: GraphicsSettings::default(),
        }
    }
}

fn sync_config_from_runtime(
    mut store: ResMut<AppConfigStore>,
    key_mapping: Res<KeyMapping>,
    movement: Res<MovementSettings>,
    audio: Res<AudioSettings>,
    graphics: Res<GraphicsSettings>,
) {
    let mut changed = false;

    if key_mapping.is_changed() && store.config.key_mapping != *key_mapping {
        store.config.key_mapping = key_mapping.clone();
        changed = true;
    }

    if movement.is_changed() && store.config.movement != *movement {
        store.config.movement = movement.clone();
        changed = true;
    }

    if audio.is_changed() && store.config.audio != *audio {
        store.config.audio = audio.clone();
        changed = true;
    }

    if graphics.is_changed() && store.config.graphics != *graphics {
        store.config.graphics = graphics.clone();
        changed = true;
    }

    if changed {
        if let Err(error) = store.save() {
            warn!("Failed to save config to {}: {}", store.path.display(), error);
        }
    }
}

fn process_global_setting_shortcuts(
    input: Res<ButtonInput<KeyCode>>,
    key_mapping: Res<KeyMapping>,
    pause_menu: Option<Res<PauseMenuState>>,
    mut graphics: ResMut<GraphicsSettings>,
) {
    if is_pause_menu_open(pause_menu.as_deref()) {
        return;
    }

    if key_mapping.just_pressed(&input, KeyAction::FullscreenToggle) {
        graphics.fullscreen = !graphics.fullscreen;
    }
    if key_mapping.just_pressed(&input, KeyAction::VsyncToggle) {
        graphics.vsync_enabled = !graphics.vsync_enabled;
    }
    if key_mapping.just_pressed(&input, KeyAction::FpsStats) {
        graphics.show_perf_hud = !graphics.show_perf_hud;
    }
    if key_mapping.just_pressed(&input, KeyAction::PhysicsGizmos) {
        graphics.show_physics_gizmos = !graphics.show_physics_gizmos;
    }
}

fn apply_window_settings(
    graphics: Res<GraphicsSettings>,
    mut q_window: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !graphics.is_changed() {
        return;
    }

    let Ok(mut window) = q_window.single_mut() else {
        return;
    };
    window.mode = graphics.window_mode();
    window.present_mode = graphics.present_mode();
}

fn apply_physics_gizmo_settings(
    graphics: Res<GraphicsSettings>,
    debug_context: Option<ResMut<DebugRenderContext>>,
) {
    if !graphics.is_changed() {
        return;
    }

    let Some(mut debug_context) = debug_context else {
        return;
    };
    debug_context.enabled = graphics.show_physics_gizmos;
}

fn default_config_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("io", "xertrov", "ssol-simulator") {
        return project_dirs.config_dir().join(CONFIG_FILE_NAME);
    }

    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        return Path::new(&config_home)
            .join("ssol-simulator")
            .join(CONFIG_FILE_NAME);
    }

    PathBuf::from(CONFIG_FILE_NAME)
}
