use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_rapier3d::prelude::Velocity;

use crate::{
    audio::AudioSettings,
    camera_switcher::CameraMode,
    config::GraphicsSettings,
    game_state::{GameState, set_hard_paused},
    key_mapping::{KeyAction, KeyMapping},
    player::{Player, set_grab_mode, MovementSettings},
    ui::{
        in_game::UiFlashEvent,
        toasts::ToastEvent,
    },
};

pub struct PauseMenuUiPlugin;

impl Plugin for PauseMenuUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenuState>()
            .add_systems(Startup, setup_pause_menu_ui)
            .add_systems(
                Update,
                (
                    toggle_pause_menu,
                    navigate_pause_menu,
                    capture_rebind_input,
                    sync_pause_menu_ui,
                )
                    .chain(),
            );
    }
}

#[derive(Resource, Debug, Default)]
pub struct PauseMenuState {
    pub open: bool,
    pub selected_index: usize,
    pub capture_action: Option<KeyAction>,
    pub capture_armed: bool,
    pub resume_on_close: bool,
}

pub fn is_pause_menu_open(state: Option<&PauseMenuState>) -> bool {
    state.map(|state| state.open).unwrap_or(false)
}

#[derive(Component)]
struct PauseMenuRoot;

#[derive(Component)]
struct PauseMenuBody;

#[derive(Component)]
struct PauseMenuModal;

#[derive(Component)]
struct PauseMenuModalText;

#[derive(Debug, Clone, Copy)]
enum MenuItem {
    Binding(KeyAction),
    MouseSensitivity,
    FreeCamSpeed,
    MasterVolume,
    MusicVolume,
    SfxVolume,
    Fullscreen,
    Vsync,
    PerfHud,
    PhysicsGizmos,
}

impl MenuItem {
    const ALL: [Self; 24] = [
        Self::Binding(KeyAction::Forward),
        Self::Binding(KeyAction::Backward),
        Self::Binding(KeyAction::Left),
        Self::Binding(KeyAction::Right),
        Self::Binding(KeyAction::FreeCam),
        Self::Binding(KeyAction::FpsStats),
        Self::Binding(KeyAction::VsyncToggle),
        Self::Binding(KeyAction::FullscreenToggle),
        Self::Binding(KeyAction::FreeCamUp),
        Self::Binding(KeyAction::FreeCamDown),
        Self::Binding(KeyAction::ResetGame),
        Self::Binding(KeyAction::PauseGame),
        Self::Binding(KeyAction::ToggleWhiteArch),
        Self::Binding(KeyAction::Cheat99Orbs),
        Self::Binding(KeyAction::PhysicsGizmos),
        Self::MouseSensitivity,
        Self::FreeCamSpeed,
        Self::MasterVolume,
        Self::MusicVolume,
        Self::SfxVolume,
        Self::Fullscreen,
        Self::Vsync,
        Self::PerfHud,
        Self::PhysicsGizmos,
    ];

    fn section(self) -> &'static str {
        match self {
            Self::Binding(_) => "Keyboard Shortcuts",
            Self::MouseSensitivity | Self::FreeCamSpeed => "Mouse And Camera",
            Self::MasterVolume | Self::MusicVolume | Self::SfxVolume => "Audio",
            Self::Fullscreen | Self::Vsync | Self::PerfHud | Self::PhysicsGizmos => {
                "Graphics And Debug"
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Binding(action) => action.label(),
            Self::MouseSensitivity => "Mouse sensitivity",
            Self::FreeCamSpeed => "Free camera speed",
            Self::MasterVolume => "Master volume",
            Self::MusicVolume => "Music volume",
            Self::SfxVolume => "SFX volume",
            Self::Fullscreen => "Fullscreen",
            Self::Vsync => "VSync",
            Self::PerfHud => "Perf HUD",
            Self::PhysicsGizmos => "Physics gizmos",
        }
    }

    fn value_label(
        self,
        key_mapping: &KeyMapping,
        movement: &MovementSettings,
        audio: &AudioSettings,
        graphics: &GraphicsSettings,
    ) -> String {
        match self {
            Self::Binding(action) => key_mapping.binding_label(action),
            Self::MouseSensitivity => {
                slider_label(movement.mouse_sens, 0.00002, 0.0005, format!("{:.5}", movement.mouse_sens))
            }
            Self::FreeCamSpeed => {
                slider_label(movement.free_cam_speed, 5.0, 120.0, format!("{:.0}", movement.free_cam_speed))
            }
            Self::MasterVolume => slider_label(audio.master_v, 0.0, 1.0, format!("{:.0}%", audio.master_v * 100.0)),
            Self::MusicVolume => slider_label(audio.music_v, 0.0, 1.0, format!("{:.0}%", audio.music_v * 100.0)),
            Self::SfxVolume => slider_label(audio.sfx_v, 0.0, 1.0, format!("{:.0}%", audio.sfx_v * 100.0)),
            Self::Fullscreen => on_off_label(graphics.fullscreen),
            Self::Vsync => on_off_label(graphics.vsync_enabled),
            Self::PerfHud => on_off_label(graphics.show_perf_hud),
            Self::PhysicsGizmos => on_off_label(graphics.show_physics_gizmos),
        }
    }

    fn adjust(
        self,
        direction: i32,
        movement: &mut MovementSettings,
        audio: &mut AudioSettings,
        graphics: &mut GraphicsSettings,
    ) {
        if direction == 0 {
            return;
        }

        match self {
            Self::Binding(_) => {}
            Self::MouseSensitivity => {
                movement.mouse_sens =
                    (movement.mouse_sens + direction as f32 * 0.00002).clamp(0.00002, 0.0005);
            }
            Self::FreeCamSpeed => {
                movement.free_cam_speed =
                    (movement.free_cam_speed + direction as f32 * 5.0).clamp(5.0, 120.0);
            }
            Self::MasterVolume => {
                audio.master_v = (audio.master_v + direction as f32 * 0.05).clamp(0.0, 1.0);
            }
            Self::MusicVolume => {
                audio.music_v = (audio.music_v + direction as f32 * 0.05).clamp(0.0, 1.0);
            }
            Self::SfxVolume => {
                audio.sfx_v = (audio.sfx_v + direction as f32 * 0.05).clamp(0.0, 1.0);
            }
            Self::Fullscreen => graphics.fullscreen = !graphics.fullscreen,
            Self::Vsync => graphics.vsync_enabled = !graphics.vsync_enabled,
            Self::PerfHud => graphics.show_perf_hud = !graphics.show_perf_hud,
            Self::PhysicsGizmos => graphics.show_physics_gizmos = !graphics.show_physics_gizmos,
        }
    }

    fn reset(
        self,
        key_mapping: &mut KeyMapping,
        movement: &mut MovementSettings,
        audio: &mut AudioSettings,
        graphics: &mut GraphicsSettings,
    ) {
        match self {
            Self::Binding(action) => key_mapping.reset_key(action),
            Self::MouseSensitivity => movement.mouse_sens = MovementSettings::default().mouse_sens,
            Self::FreeCamSpeed => movement.free_cam_speed = MovementSettings::default().free_cam_speed,
            Self::MasterVolume => audio.master_v = AudioSettings::default().master_v,
            Self::MusicVolume => audio.music_v = AudioSettings::default().music_v,
            Self::SfxVolume => audio.sfx_v = AudioSettings::default().sfx_v,
            Self::Fullscreen => graphics.fullscreen = GraphicsSettings::default().fullscreen,
            Self::Vsync => graphics.vsync_enabled = GraphicsSettings::default().vsync_enabled,
            Self::PerfHud => graphics.show_perf_hud = GraphicsSettings::default().show_perf_hud,
            Self::PhysicsGizmos => {
                graphics.show_physics_gizmos = GraphicsSettings::default().show_physics_gizmos
            }
        }
    }
}

fn setup_pause_menu_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");

    commands
        .spawn((
            PauseMenuRoot,
            Visibility::Hidden,
            GlobalZIndex(900),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(980.0),
                    max_width: Val::Percent(90.0),
                    max_height: Val::Percent(85.0),
                    border: UiRect::all(Val::Px(2.0)),
                    padding: UiRect::all(Val::Px(22.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(14.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.07, 0.1, 0.97)),
                BorderColor::all(Color::srgba(0.85, 0.85, 0.95, 0.25)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Paused"),
                    TextFont {
                        font: font.clone(),
                        font_size: 44.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
                panel.spawn((
                    Text::new(
                        "Menu controls: Up/Down or W/S select, Left/Right or A/D adjust, Enter activate, R reset, Esc or ? close",
                    ),
                    TextFont {
                        font: font.clone(),
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.82, 0.84, 0.9, 0.95)),
                ));
                panel.spawn((
                    PauseMenuBody,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 25.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });

            root.spawn((
                PauseMenuModal,
                Visibility::Hidden,
                GlobalZIndex(910),
                Node {
                    position_type: PositionType::Absolute,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ))
            .with_children(|modal| {
                modal.spawn((
                    Node {
                        width: Val::Px(520.0),
                        max_width: Val::Percent(80.0),
                        border: UiRect::all(Val::Px(2.0)),
                        padding: UiRect::all(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.12, 0.1, 0.05, 0.96)),
                    BorderColor::all(Color::srgba(1.0, 0.55, 0.0, 0.95)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        PauseMenuModalText,
                        Text::new(""),
                        TextFont {
                            font,
                            font_size: 30.0,
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 0.96, 0.88, 1.0)),
                    ));
                });
            });
        });
}

fn toggle_pause_menu(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    active_camera: Res<crate::camera_switcher::ActiveCamera>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut q_player: Query<(&mut Transform, &mut Velocity), With<Player>>,
    mut q_cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut game_state: ResMut<GameState>,
) {
    let toggle_pressed = input.just_pressed(KeyCode::Escape) || just_pressed_question(&input);
    if !toggle_pressed {
        return;
    }

    if pause_menu.capture_action.take().is_some() {
        pause_menu.capture_armed = false;
        return;
    }

    if pause_menu.open {
        pause_menu.open = false;
        if pause_menu.resume_on_close {
            if let Ok((mut transform, mut velocity)) = q_player.single_mut() {
                set_hard_paused(
                    &mut commands,
                    &mut game_state,
                    &mut transform,
                    &mut velocity,
                    false,
                );
            }
        }
        pause_menu.resume_on_close = false;
        pause_menu.capture_armed = false;
        if let Ok(mut cursor_options) = q_cursor.single_mut() {
            set_grab_mode(&mut cursor_options, CursorGrabMode::Locked);
        }
        return;
    }

    if active_camera.0 != CameraMode::FirstPerson {
        return;
    }

    pause_menu.open = true;
    pause_menu.selected_index = pause_menu.selected_index.min(MenuItem::ALL.len() - 1);
    pause_menu.resume_on_close = !game_state.is_hard_paused;

    if pause_menu.resume_on_close {
        if let Ok((mut transform, mut velocity)) = q_player.single_mut() {
            set_hard_paused(
                &mut commands,
                &mut game_state,
                &mut transform,
                &mut velocity,
                true,
            );
        }
    }

    if let Ok(mut cursor_options) = q_cursor.single_mut() {
        set_grab_mode(&mut cursor_options, CursorGrabMode::None);
    }
}

fn navigate_pause_menu(
    input: Res<ButtonInput<KeyCode>>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut key_mapping: ResMut<KeyMapping>,
    mut movement: ResMut<MovementSettings>,
    mut audio: ResMut<AudioSettings>,
    mut graphics: ResMut<GraphicsSettings>,
) {
    if !pause_menu.open || pause_menu.capture_action.is_some() {
        return;
    }

    let vertical = match (
        input.just_pressed(KeyCode::ArrowUp) || input.just_pressed(KeyCode::KeyW),
        input.just_pressed(KeyCode::ArrowDown) || input.just_pressed(KeyCode::KeyS),
    ) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };

    if vertical != 0 {
        let len = MenuItem::ALL.len() as isize;
        let next = (pause_menu.selected_index as isize + vertical as isize).rem_euclid(len);
        pause_menu.selected_index = next as usize;
    }

    let horizontal = match (
        input.just_pressed(KeyCode::ArrowLeft) || input.just_pressed(KeyCode::KeyA),
        input.just_pressed(KeyCode::ArrowRight) || input.just_pressed(KeyCode::KeyD),
    ) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };

    let current = MenuItem::ALL[pause_menu.selected_index];
    if horizontal != 0 {
        current.adjust(horizontal, &mut movement, &mut audio, &mut graphics);
    }

    if input.just_pressed(KeyCode::KeyR) {
        current.reset(&mut key_mapping, &mut movement, &mut audio, &mut graphics);
    }

    if input.just_pressed(KeyCode::Enter) || input.just_pressed(KeyCode::NumpadEnter) {
        match current {
            MenuItem::Binding(action) => {
                pause_menu.capture_action = Some(action);
                pause_menu.capture_armed = true;
            }
            _ => current.adjust(1, &mut movement, &mut audio, &mut graphics),
        }
    }
}

fn capture_rebind_input(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut key_mapping: ResMut<KeyMapping>,
) {
    let Some(action) = pause_menu.capture_action else {
        return;
    };

    if pause_menu.capture_armed {
        pause_menu.capture_armed = false;
        return;
    }

    if input.just_pressed(KeyCode::Escape) {
        pause_menu.capture_action = None;
        pause_menu.capture_armed = false;
        return;
    }

    let Some(key) = input.get_just_pressed().next().copied() else {
        return;
    };
    if matches!(
        key,
        KeyCode::ShiftLeft | KeyCode::ShiftRight | KeyCode::ControlLeft | KeyCode::ControlRight
    ) {
        return;
    }

    pause_menu.capture_action = None;
    pause_menu.capture_armed = false;
    if key_mapping.key_for(action) == Some(key) {
        return;
    }

    if let Some(old_action) = key_mapping.action_for_key(key) {
        if old_action != action {
            key_mapping.set_key(old_action, None);
            commands.trigger(UiFlashEvent::warning());
            commands.trigger(ToastEvent::warning(format!(
                "Unbound {} and added it to {}.",
                old_action.label(),
                action.label()
            )));
        }
    }

    key_mapping.set_key(action, Some(key));
}

fn sync_pause_menu_ui(
    mut commands: Commands,
    pause_menu: Res<PauseMenuState>,
    key_mapping: Res<KeyMapping>,
    movement: Res<MovementSettings>,
    audio: Res<AudioSettings>,
    graphics: Res<GraphicsSettings>,
    q_root: Query<Entity, With<PauseMenuRoot>>,
    q_body: Query<Entity, With<PauseMenuBody>>,
    q_modal: Query<Entity, With<PauseMenuModal>>,
    q_modal_text: Query<Entity, With<PauseMenuModalText>>,
) {
    if !pause_menu.is_changed()
        && !key_mapping.is_changed()
        && !movement.is_changed()
        && !audio.is_changed()
        && !graphics.is_changed()
    {
        return;
    }

    if let Ok(root) = q_root.single() {
        commands.entity(root).insert(match pause_menu.open {
            true => Visibility::Visible,
            false => Visibility::Hidden,
        });
    }

    if let Ok(body) = q_body.single() {
        commands
            .entity(body)
            .insert(Text::new(render_menu_body(
                pause_menu.selected_index,
                &key_mapping,
                &movement,
                &audio,
                &graphics,
            )));
    }

    let modal_visibility = match pause_menu.capture_action {
        Some(_) => Visibility::Visible,
        None => Visibility::Hidden,
    };
    if let Ok(modal) = q_modal.single() {
        commands.entity(modal).insert(modal_visibility);
    }
    if let Ok(modal_text) = q_modal_text.single() {
        let text = match pause_menu.capture_action {
            Some(action) => format!(
                "Press a key to bind to {}.\nEsc cancels.",
                action.label()
            ),
            None => String::new(),
        };
        commands.entity(modal_text).insert(Text::new(text));
    }
}

fn render_menu_body(
    selected_index: usize,
    key_mapping: &KeyMapping,
    movement: &MovementSettings,
    audio: &AudioSettings,
    graphics: &GraphicsSettings,
) -> String {
    let mut lines = Vec::new();
    let mut current_section = "";

    for (index, item) in MenuItem::ALL.into_iter().enumerate() {
        if item.section() != current_section {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            current_section = item.section();
            lines.push(current_section.to_string());
        }

        let prefix = if index == selected_index { "> " } else { "  " };
        lines.push(format!(
            "{}{:<24} {}",
            prefix,
            item.label(),
            item.value_label(key_mapping, movement, audio, graphics)
        ));
    }

    lines.push(String::new());
    lines.push("Pause menu: Esc or ? opens/closes in first person.".to_string());
    lines.join("\n")
}

fn just_pressed_question(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::Slash)
        && (input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight))
}

fn slider_label(value: f32, min: f32, max: f32, value_label: String) -> String {
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    let filled = (t * 12.0).round() as usize;
    let empty = 12usize.saturating_sub(filled);
    format!("[{}{}] {}", "=".repeat(filled), "-".repeat(empty), value_label)
}

fn on_off_label(value: bool) -> String {
    match value {
        true => "On".to_string(),
        false => "Off".to_string(),
    }
}
