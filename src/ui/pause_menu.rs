use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    picking::hover::HoverMap,
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
    player::{MovementSettings, Player, set_grab_mode},
    ui::{in_game::UiFlashEvent, toasts::ToastEvent},
};

const PANEL_WIDTH: f32 = 1080.0;
const PANEL_HEIGHT: f32 = 720.0;
const KEYBIND_ROW_HEIGHT: f32 = 46.0;
const KEYBIND_ROW_GAP: f32 = 6.0;
const MOUSE_SENS_MIN: f32 = 0.00002;
const MOUSE_SENS_MAX: f32 = 0.0005;
const FREE_CAM_SPEED_MIN: f32 = 5.0;
const FREE_CAM_SPEED_MAX: f32 = 120.0;

pub struct PauseMenuUiPlugin;

impl Plugin for PauseMenuUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenuState>()
            .add_systems(Startup, setup_pause_menu_ui)
            .add_systems(Update, toggle_pause_menu)
            .add_systems(Update, handle_pause_menu_keyboard)
            .add_systems(Update, capture_rebind_input)
            .add_systems(Update, update_hover_focus)
            .add_systems(Update, activate_pressed_buttons)
            .add_systems(Update, apply_mouse_wheel_input)
            .add_systems(Update, keep_selected_keybind_visible)
            .add_systems(Update, sync_pause_menu_visibility)
            .add_systems(Update, sync_pause_menu_hint)
            .add_systems(Update, sync_pause_menu_styles)
            .add_systems(Update, sync_pause_menu_values)
            .add_systems(Update, sync_pause_menu_modal);
    }
}

#[derive(Resource, Debug, Clone)]
pub struct PauseMenuState {
    pub open: bool,
    pub focus: FocusTarget,
    pub modal: PauseMenuModal,
    pub resume_on_close: bool,
    pub capture_armed: bool,
}

impl Default for PauseMenuState {
    fn default() -> Self {
        Self {
            open: false,
            focus: FocusTarget::Setting(SettingItem::MouseSensitivity),
            modal: PauseMenuModal::None,
            resume_on_close: false,
            capture_armed: false,
        }
    }
}

pub fn is_pause_menu_open(state: Option<&PauseMenuState>) -> bool {
    state.map(|state| state.open).unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    Setting(SettingItem),
    Keybind(KeyAction),
    Footer(FooterAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingItem {
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

impl SettingItem {
    const ALL: [Self; 9] = [
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

    fn label(self) -> &'static str {
        match self {
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

    fn hint(self) -> &'static str {
        match self {
            Self::MouseSensitivity => "Affects first-person and free-camera look speed.",
            Self::FreeCamSpeed => "Changes the speed of the detached debug camera.",
            Self::MasterVolume => "Global loudness for all music and effects.",
            Self::MusicVolume => "Background music volume mix.",
            Self::SfxVolume => "Effects and movement sounds.",
            Self::Fullscreen => "Applies immediately when toggled.",
            Self::Vsync => "Synchronizes presentation with the display refresh.",
            Self::PerfHud => "Shows the performance and physics HUD overlay.",
            Self::PhysicsGizmos => "Shows Rapier physics debug shapes only.",
        }
    }

    fn is_slider(self) -> bool {
        matches!(
            self,
            Self::MouseSensitivity
                | Self::FreeCamSpeed
                | Self::MasterVolume
                | Self::MusicVolume
                | Self::SfxVolume
        )
    }

    fn value_label(
        self,
        movement: &MovementSettings,
        audio: &AudioSettings,
        graphics: &GraphicsSettings,
    ) -> String {
        match self {
            Self::MouseSensitivity => format!("{:.5}", movement.mouse_sens),
            Self::FreeCamSpeed => format!("{:.0}", movement.free_cam_speed),
            Self::MasterVolume => format!("{:.0}%", audio.master_v * 100.0),
            Self::MusicVolume => format!("{:.0}%", audio.music_v * 100.0),
            Self::SfxVolume => format!("{:.0}%", audio.sfx_v * 100.0),
            Self::Fullscreen => bool_label(graphics.fullscreen),
            Self::Vsync => bool_label(graphics.vsync_enabled),
            Self::PerfHud => bool_label(graphics.show_perf_hud),
            Self::PhysicsGizmos => bool_label(graphics.show_physics_gizmos),
        }
    }

    fn slider_fraction(self, movement: &MovementSettings, audio: &AudioSettings) -> f32 {
        match self {
            Self::MouseSensitivity => normalize(movement.mouse_sens, MOUSE_SENS_MIN, MOUSE_SENS_MAX),
            Self::FreeCamSpeed => {
                normalize(movement.free_cam_speed, FREE_CAM_SPEED_MIN, FREE_CAM_SPEED_MAX)
            }
            Self::MasterVolume => audio.master_v,
            Self::MusicVolume => audio.music_v,
            Self::SfxVolume => audio.sfx_v,
            _ => 0.0,
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
            Self::MouseSensitivity => {
                movement.mouse_sens =
                    (movement.mouse_sens + direction as f32 * 0.00002).clamp(MOUSE_SENS_MIN, MOUSE_SENS_MAX)
            }
            Self::FreeCamSpeed => {
                movement.free_cam_speed = (movement.free_cam_speed + direction as f32 * 5.0)
                    .clamp(FREE_CAM_SPEED_MIN, FREE_CAM_SPEED_MAX)
            }
            Self::MasterVolume => audio.master_v = (audio.master_v + direction as f32 * 0.05).clamp(0.0, 1.0),
            Self::MusicVolume => audio.music_v = (audio.music_v + direction as f32 * 0.05).clamp(0.0, 1.0),
            Self::SfxVolume => audio.sfx_v = (audio.sfx_v + direction as f32 * 0.05).clamp(0.0, 1.0),
            Self::Fullscreen => graphics.fullscreen = !graphics.fullscreen,
            Self::Vsync => graphics.vsync_enabled = !graphics.vsync_enabled,
            Self::PerfHud => graphics.show_perf_hud = !graphics.show_perf_hud,
            Self::PhysicsGizmos => graphics.show_physics_gizmos = !graphics.show_physics_gizmos,
        }
    }

    fn reset(self, movement: &mut MovementSettings, audio: &mut AudioSettings, graphics: &mut GraphicsSettings) {
        let movement_default = MovementSettings::default();
        let audio_default = AudioSettings::default();
        let graphics_default = GraphicsSettings::default();
        match self {
            Self::MouseSensitivity => movement.mouse_sens = movement_default.mouse_sens,
            Self::FreeCamSpeed => movement.free_cam_speed = movement_default.free_cam_speed,
            Self::MasterVolume => audio.master_v = audio_default.master_v,
            Self::MusicVolume => audio.music_v = audio_default.music_v,
            Self::SfxVolume => audio.sfx_v = audio_default.sfx_v,
            Self::Fullscreen => graphics.fullscreen = graphics_default.fullscreen,
            Self::Vsync => graphics.vsync_enabled = graphics_default.vsync_enabled,
            Self::PerfHud => graphics.show_perf_hud = graphics_default.show_perf_hud,
            Self::PhysicsGizmos => graphics.show_physics_gizmos = graphics_default.show_physics_gizmos,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FooterAction {
    Resume,
    ResetAllBindings,
}

impl FooterAction {
    const ALL: [Self; 2] = [Self::Resume, Self::ResetAllBindings];

    fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::ResetAllBindings => "Reset All Bindings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseMenuModal {
    None,
    CaptureKey(KeyAction),
    ConfirmResetAll { selected: ConfirmChoice },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Cancel,
    Confirm,
}

impl ConfirmChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Cancel => "Cancel",
            Self::Confirm => "Reset all",
        }
    }

    fn flip(self) -> Self {
        match self {
            Self::Cancel => Self::Confirm,
            Self::Confirm => Self::Cancel,
        }
    }
}

#[derive(Component)]
struct PauseMenuRoot;
#[derive(Component)]
struct SelectedHintTitle;
#[derive(Component)]
struct SelectedHintBody;
#[derive(Component)]
struct LeftColumnScrollViewport;
#[derive(Component)]
struct KeybindScrollViewport;
#[derive(Component)]
struct KeybindRow(KeyAction);
#[derive(Component)]
struct SettingRow(SettingItem);
#[derive(Component)]
struct FooterButtonMarker(FooterAction);
#[derive(Component)]
struct ModalBackdrop;
#[derive(Component)]
struct ModalTitleText;
#[derive(Component)]
struct ModalBodyText;
#[derive(Component)]
struct ModalButtonMarker(ConfirmChoice);
#[derive(Component)]
struct SliderFill(SettingItem);

#[derive(Component)]
struct ValuePillText(ValueTarget);

#[derive(Debug, Clone, Copy)]
enum ValueTarget {
    Setting(SettingItem),
    Keybind(KeyAction),
}

#[derive(Component, Debug, Clone, Copy)]
struct Focusable {
    target: FocusTarget,
    action: ButtonAction,
}

#[derive(Debug, Clone, Copy)]
enum ButtonAction {
    FocusOnly,
    ToggleSetting,
    OpenRebind(KeyAction),
    Footer(FooterAction),
    ModalChoice(ConfirmChoice),
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
            BackgroundColor(Color::srgba(0.01, 0.02, 0.04, 0.76)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(PANEL_WIDTH),
                    height: Val::Px(PANEL_HEIGHT),
                    max_width: Val::Percent(92.0),
                    max_height: Val::Percent(88.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(18.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.055, 0.065, 0.085, 0.98)),
                BorderColor::all(Color::srgba(0.7, 0.75, 0.86, 0.16)),
                BorderRadius::all(Val::Px(18.0)),
            ))
            .with_children(|panel| {
                spawn_header(panel, &font);
                panel
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_grow: 1.0,
                        min_height: Val::Px(0.0),
                        column_gap: Val::Px(18.0),
                        ..row_node()
                    })
                    .with_children(|body| {
                        spawn_left_column(body, &font);
                        spawn_right_column(body, &font);
                    });
                spawn_footer(panel, &font);
            });

            spawn_modal(root, &font);
        });
}

fn spawn_header(parent: &mut ChildSpawnerCommands, font: &Handle<Font>) {
    parent.spawn(Node {
        width: Val::Percent(100.0),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::End,
        ..row_node()
    })
    .with_children(|header| {
        header.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|text_col| {
            text_col.spawn((Text::new("Pause"), heading_text(font.clone(), 34.0), TextColor(Color::WHITE)));
            text_col.spawn((
                Text::new("Refined controls, settings, and bindings."),
                body_text(font.clone(), 15.0),
                TextColor(Color::srgba(0.78, 0.8, 0.86, 0.95)),
            ));
        });

        header.spawn((
            Text::new("Esc / ? close   WASD or arrows move   Enter select   R reset focused"),
            body_text(font.clone(), 26.0),
            TextColor(Color::srgba(0.56, 0.62, 0.72, 0.92)),
        ));
    });
}

fn spawn_left_column(parent: &mut ChildSpawnerCommands, font: &Handle<Font>) {
    parent.spawn(Node {
        width: Val::Px(340.0),
        height: Val::Percent(100.0),
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        ..default()
    })
    .with_children(|left| {
        left.spawn((
            LeftColumnScrollViewport,
            Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                padding: UiRect::right(Val::Px(4.0)),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition(Vec2::ZERO),
        ))
        .with_children(|viewport| {
            viewport
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(14.0),
                    ..default()
                })
                .with_children(|content| {
                    content.spawn(card_node())
                        .insert(BackgroundColor(Color::srgba(0.07, 0.08, 0.11, 0.94)))
                        .insert(BorderColor::all(Color::srgba(0.72, 0.75, 0.82, 0.12)))
                        .insert(BorderRadius::all(Val::Px(16.0)))
                        .with_children(|card| {
                            card.spawn((
                                Text::new("Current Selection"),
                                body_text(font.clone(), 12.0),
                                TextColor(Color::srgba(0.54, 0.63, 0.78, 0.92)),
                            ));
                            card.spawn((
                                SelectedHintTitle,
                                Text::new("Mouse sensitivity"),
                                heading_text(font.clone(), 22.0),
                                TextColor(Color::WHITE),
                            ));
                            card.spawn((
                                SelectedHintBody,
                                Text::new("Affects first-person and free-camera look speed."),
                                body_text(font.clone(), 15.0),
                                TextColor(Color::srgba(0.8, 0.82, 0.88, 0.96)),
                            ));
                        });

                    spawn_settings_card(content, font, "Mouse And Camera", &[SettingItem::MouseSensitivity, SettingItem::FreeCamSpeed]);
                    spawn_settings_card(content, font, "Audio", &[SettingItem::MasterVolume, SettingItem::MusicVolume, SettingItem::SfxVolume]);
                    spawn_settings_card(
                        content,
                        font,
                        "Graphics And Debug",
                        &[SettingItem::Fullscreen, SettingItem::Vsync, SettingItem::PerfHud, SettingItem::PhysicsGizmos],
                    );
                });
            });
    });
}

fn spawn_settings_card(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, title: &str, items: &[SettingItem]) {
    parent.spawn(card_node())
        .insert(BackgroundColor(Color::srgba(0.085, 0.095, 0.12, 0.96)))
        .insert(BorderColor::all(Color::srgba(0.72, 0.75, 0.82, 0.1)))
        .insert(BorderRadius::all(Val::Px(16.0)))
        .with_children(|card| {
            card.spawn((Text::new(title), body_text(font.clone(), 13.0), TextColor(Color::srgba(0.76, 0.8, 0.9, 0.95))));
            for item in items {
                card.spawn((
                    Button,
                    row_surface_node(56.0),
                    BackgroundColor(Color::srgba(0.11, 0.12, 0.16, 0.84)),
                    BorderColor::all(Color::srgba(0.78, 0.8, 0.88, 0.08)),
                    BorderRadius::all(Val::Px(14.0)),
                    Interaction::None,
                    SettingRow(*item),
                    Focusable {
                        target: FocusTarget::Setting(*item),
                        action: if item.is_slider() { ButtonAction::FocusOnly } else { ButtonAction::ToggleSetting },
                    },
                ))
                .with_children(|row| {
                    row.spawn((Text::new(item.label()), body_text(font.clone(), 16.0), TextColor(Color::WHITE)));
                    row.spawn(Node {
                        width: Val::Px(150.0),
                        justify_content: JustifyContent::FlexEnd,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..row_node()
                    })
                    .with_children(|control| {
                        if item.is_slider() {
                            control.spawn((
                                Node {
                                    width: Val::Px(88.0),
                                    height: Val::Px(8.0),
                                    justify_content: JustifyContent::FlexStart,
                                    overflow: Overflow::clip(),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.14, 0.16, 0.2, 1.0)),
                                BorderRadius::all(Val::Px(99.0)),
                            ))
                            .with_children(|track| {
                                track.spawn((
                                    SliderFill(*item),
                                    Node {
                                        width: Val::Percent(0.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.63, 0.73, 0.96, 0.95)),
                                    BorderRadius::all(Val::Px(99.0)),
                                ));
                            });
                        }
                        control.spawn(value_pill_node())
                            .with_child((ValuePillText(ValueTarget::Setting(*item)), Text::new(""), body_text(font.clone(), 14.0), TextColor(Color::WHITE)));
                    });
                });
            }
        });
}

fn spawn_right_column(parent: &mut ChildSpawnerCommands, font: &Handle<Font>) {
    parent.spawn(card_node())
        .insert(Node {
            flex_grow: 1.0,
            height: Val::Percent(100.0),
            min_width: Val::Px(0.0),
            ..card_node()
        })
        .insert(BackgroundColor(Color::srgba(0.085, 0.095, 0.12, 0.96)))
        .insert(BorderColor::all(Color::srgba(0.72, 0.75, 0.82, 0.1)))
        .insert(BorderRadius::all(Val::Px(16.0)))
        .with_children(|right| {
            right.spawn((Text::new("Keybindings"), heading_text(font.clone(), 24.0), TextColor(Color::WHITE)));
            right.spawn((
                Text::new("Scroll with the mouse wheel or keyboard. Hover to highlight, click to rebind."),
                body_text(font.clone(), 14.0),
                TextColor(Color::srgba(0.78, 0.82, 0.9, 0.94)),
            ));
            right.spawn((
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..row_node()
                },
                BackgroundColor(Color::srgba(0.12, 0.14, 0.18, 0.94)),
                BorderColor::all(Color::srgba(0.76, 0.8, 0.88, 0.09)),
                BorderRadius::all(Val::Px(12.0)),
            ))
            .with_children(|header| {
                header.spawn((Text::new("Action"), body_text(font.clone(), 13.0), TextColor(Color::srgba(0.65, 0.72, 0.83, 0.96))));
                header.spawn((Text::new("Binding"), body_text(font.clone(), 13.0), TextColor(Color::srgba(0.65, 0.72, 0.83, 0.96))));
            });
            right.spawn((
                KeybindScrollViewport,
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    padding: UiRect::right(Val::Px(4.0)),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition(Vec2::ZERO),
            ))
            .with_children(|viewport| {
                viewport.spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(KEYBIND_ROW_GAP),
                    ..default()
                })
                .with_children(|list| {
                    for action in KeyAction::ALL {
                        list.spawn((
                            Button,
                            row_surface_node(KEYBIND_ROW_HEIGHT),
                            BackgroundColor(Color::srgba(0.1, 0.11, 0.15, 0.82)),
                            BorderColor::all(Color::srgba(0.82, 0.85, 0.92, 0.06)),
                            BorderRadius::all(Val::Px(14.0)),
                            Interaction::None,
                            KeybindRow(action),
                            Focusable {
                                target: FocusTarget::Keybind(action),
                                action: ButtonAction::OpenRebind(action),
                            },
                        ))
                        .with_children(|row| {
                            row.spawn((Text::new(action.label()), body_text(font.clone(), 16.0), TextColor(Color::WHITE)));
                            row.spawn(value_pill_node())
                                .with_child((ValuePillText(ValueTarget::Keybind(action)), Text::new(""), body_text(font.clone(), 14.0), TextColor(Color::WHITE)));
                        });
                    }
                });
            });
        });
}

fn spawn_footer(parent: &mut ChildSpawnerCommands, font: &Handle<Font>) {
    parent.spawn(Node {
        width: Val::Percent(100.0),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        ..row_node()
    })
    .with_children(|footer| {
        footer.spawn((
            Text::new("Bindings and settings persist automatically through the unified config."),
            body_text(font.clone(), 13.0),
            TextColor(Color::srgba(0.6, 0.67, 0.77, 0.96)),
        ));
        footer.spawn(Node {
            column_gap: Val::Px(12.0),
            ..row_node()
        })
        .with_children(|buttons| {
            for action in FooterAction::ALL {
                buttons.spawn((
                    Button,
                    Node {
                        min_width: Val::Px(if action == FooterAction::Resume { 120.0 } else { 200.0 }),
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.14, 0.16, 0.22, 0.95)),
                    BorderColor::all(Color::srgba(0.78, 0.82, 0.92, 0.14)),
                    BorderRadius::all(Val::Px(12.0)),
                    Interaction::None,
                    FooterButtonMarker(action),
                    Focusable {
                        target: FocusTarget::Footer(action),
                        action: ButtonAction::Footer(action),
                    },
                ))
                .with_child((Text::new(action.label()), body_text(font.clone(), 15.0), TextColor(Color::WHITE)));
            }
        });
    });
}

fn spawn_modal(parent: &mut ChildSpawnerCommands, font: &Handle<Font>) {
    parent.spawn((
        ModalBackdrop,
        Visibility::Hidden,
        GlobalZIndex(920),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.48)),
    ))
    .with_children(|overlay| {
        overlay.spawn(card_node())
            .insert(Node {
                width: Val::Px(520.0),
                max_width: Val::Percent(84.0),
                ..card_node()
            })
            .insert(BackgroundColor(Color::srgba(0.09, 0.08, 0.1, 0.98)))
            .insert(BorderColor::all(Color::srgba(1.0, 0.68, 0.38, 0.36)))
            .insert(BorderRadius::all(Val::Px(18.0)))
            .with_children(|modal| {
                modal.spawn((ModalTitleText, Text::new(""), heading_text(font.clone(), 24.0), TextColor(Color::WHITE)));
                modal.spawn((ModalBodyText, Text::new(""), body_text(font.clone(), 16.0), TextColor(Color::srgba(0.88, 0.89, 0.93, 0.98))));
                modal.spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::FlexEnd,
                    column_gap: Val::Px(12.0),
                    ..row_node()
                })
                .with_children(|buttons| {
                    for choice in [ConfirmChoice::Cancel, ConfirmChoice::Confirm] {
                        buttons.spawn((
                            Button,
                            Node {
                                min_width: Val::Px(120.0),
                                padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.13, 0.15, 0.2, 1.0)),
                            BorderColor::all(Color::srgba(0.82, 0.84, 0.9, 0.12)),
                            BorderRadius::all(Val::Px(12.0)),
                            Interaction::None,
                            ModalButtonMarker(choice),
                            Focusable {
                                target: FocusTarget::Footer(FooterAction::ResetAllBindings),
                                action: ButtonAction::ModalChoice(choice),
                            },
                        ))
                        .with_child((Text::new(choice.label()), body_text(font.clone(), 15.0), TextColor(Color::WHITE)));
                    }
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

    match pause_menu.modal {
        PauseMenuModal::CaptureKey(_) | PauseMenuModal::ConfirmResetAll { .. } => {
            pause_menu.modal = PauseMenuModal::None;
            pause_menu.capture_armed = false;
            return;
        }
        PauseMenuModal::None => {}
    }

    if pause_menu.open {
        close_pause_menu(&mut commands, &mut pause_menu, &mut q_player, &mut q_cursor, &mut game_state);
        return;
    }

    if active_camera.0 != CameraMode::FirstPerson {
        return;
    }

    pause_menu.open = true;
    pause_menu.focus = pause_menu.focus.clamp_valid();
    pause_menu.resume_on_close = !game_state.is_hard_paused;

    if pause_menu.resume_on_close {
        if let Ok((mut transform, mut velocity)) = q_player.single_mut() {
            set_hard_paused(&mut commands, &mut game_state, &mut transform, &mut velocity, true);
        }
    }

    if let Ok(mut cursor_options) = q_cursor.single_mut() {
        set_grab_mode(&mut cursor_options, CursorGrabMode::None);
    }
}

fn handle_pause_menu_keyboard(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut key_mapping: ResMut<KeyMapping>,
    mut movement: ResMut<MovementSettings>,
    mut audio: ResMut<AudioSettings>,
    mut graphics: ResMut<GraphicsSettings>,
    mut q_player: Query<(&mut Transform, &mut Velocity), With<Player>>,
    mut q_cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut game_state: ResMut<GameState>,
) {
    if !pause_menu.open {
        return;
    }

    if let PauseMenuModal::ConfirmResetAll { selected } = pause_menu.modal {
        if pressed_left(&input) || pressed_right(&input) || pressed_up(&input) || pressed_down(&input) {
            pause_menu.modal = PauseMenuModal::ConfirmResetAll {
                selected: selected.flip(),
            };
        }
        if pressed_enter(&input) {
            match selected {
                ConfirmChoice::Cancel => pause_menu.modal = PauseMenuModal::None,
                ConfirmChoice::Confirm => {
                    *key_mapping = KeyMapping::default();
                    pause_menu.modal = PauseMenuModal::None;
                    commands.trigger(ToastEvent::warning("Reset every keybinding to default."));
                    commands.trigger(UiFlashEvent::warning());
                }
            }
        }
        if input.just_pressed(KeyCode::KeyR) || input.just_pressed(KeyCode::Escape) {
            pause_menu.modal = PauseMenuModal::None;
        }
        return;
    }

    if !matches!(pause_menu.modal, PauseMenuModal::None) {
        return;
    }

    if pressed_up(&input) {
        pause_menu.focus = move_focus_vertical(pause_menu.focus, -1);
    }
    if pressed_down(&input) {
        pause_menu.focus = move_focus_vertical(pause_menu.focus, 1);
    }

    match pause_menu.focus {
        FocusTarget::Setting(setting) => {
            if pressed_left(&input) {
                setting.adjust(-1, &mut movement, &mut audio, &mut graphics);
            }
            if pressed_right(&input) || pressed_enter(&input) {
                setting.adjust(1, &mut movement, &mut audio, &mut graphics);
            }
            if input.just_pressed(KeyCode::KeyR) {
                setting.reset(&mut movement, &mut audio, &mut graphics);
            }
        }
        FocusTarget::Keybind(action) => {
            if pressed_enter(&input) {
                pause_menu.modal = PauseMenuModal::CaptureKey(action);
                pause_menu.capture_armed = true;
            }
            if input.just_pressed(KeyCode::KeyR) {
                key_mapping.reset_key(action);
            }
        }
        FocusTarget::Footer(action) => {
            if pressed_left(&input) {
                pause_menu.focus = FocusTarget::Footer(previous_footer(action));
            }
            if pressed_right(&input) {
                pause_menu.focus = FocusTarget::Footer(next_footer(action));
            }
            if pressed_enter(&input) {
                activate_footer(
                    &mut commands,
                    action,
                    &mut pause_menu,
                    &mut key_mapping,
                    &mut q_player,
                    &mut q_cursor,
                    &mut game_state,
                );
            }
        }
    }
}

fn capture_rebind_input(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut key_mapping: ResMut<KeyMapping>,
) {
    let PauseMenuModal::CaptureKey(action) = pause_menu.modal else {
        return;
    };

    if pause_menu.capture_armed {
        pause_menu.capture_armed = false;
        return;
    }

    if input.just_pressed(KeyCode::Escape) {
        pause_menu.modal = PauseMenuModal::None;
        return;
    }

    let Some(key) = input.get_just_pressed().next().copied() else {
        return;
    };
    if matches!(key, KeyCode::ShiftLeft | KeyCode::ShiftRight | KeyCode::ControlLeft | KeyCode::ControlRight) {
        return;
    }

    pause_menu.modal = PauseMenuModal::None;
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

fn update_hover_focus(
    mut pause_menu: ResMut<PauseMenuState>,
    q_interactions: Query<(&Interaction, &Focusable), (Changed<Interaction>, With<Button>)>,
) {
    if !pause_menu.open || !matches!(pause_menu.modal, PauseMenuModal::None) {
        return;
    }
    for (interaction, focusable) in &q_interactions {
        if *interaction == Interaction::Hovered {
            pause_menu.focus = focusable.target;
        }
    }
}

fn activate_pressed_buttons(
    mut commands: Commands,
    mut pause_menu: ResMut<PauseMenuState>,
    q_pressed: Query<(&Interaction, &Focusable), (Changed<Interaction>, With<Button>)>,
    mut key_mapping: ResMut<KeyMapping>,
    mut movement: ResMut<MovementSettings>,
    mut audio: ResMut<AudioSettings>,
    mut graphics: ResMut<GraphicsSettings>,
    mut q_player: Query<(&mut Transform, &mut Velocity), With<Player>>,
    mut q_cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut game_state: ResMut<GameState>,
) {
    if !pause_menu.open {
        return;
    }

    for (interaction, focusable) in &q_pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }

        pause_menu.focus = focusable.target;
        match (pause_menu.modal, focusable.action) {
            (PauseMenuModal::ConfirmResetAll { .. }, ButtonAction::ModalChoice(choice)) => {
                match choice {
                    ConfirmChoice::Cancel => pause_menu.modal = PauseMenuModal::None,
                    ConfirmChoice::Confirm => {
                        *key_mapping = KeyMapping::default();
                        pause_menu.modal = PauseMenuModal::None;
                        commands.trigger(ToastEvent::warning("Reset every keybinding to default."));
                        commands.trigger(UiFlashEvent::warning());
                    }
                }
            }
            (PauseMenuModal::None, ButtonAction::FocusOnly) => {}
            (PauseMenuModal::None, ButtonAction::ToggleSetting) => {
                if let FocusTarget::Setting(setting) = focusable.target {
                    setting.adjust(1, &mut movement, &mut audio, &mut graphics);
                }
            }
            (PauseMenuModal::None, ButtonAction::OpenRebind(action)) => {
                pause_menu.modal = PauseMenuModal::CaptureKey(action);
                pause_menu.capture_armed = true;
            }
            (PauseMenuModal::None, ButtonAction::Footer(action)) => {
                activate_footer(
                    &mut commands,
                    action,
                    &mut pause_menu,
                    &mut key_mapping,
                    &mut q_player,
                    &mut q_cursor,
                    &mut game_state,
                );
            }
            _ => {}
        }
    }
}

fn apply_mouse_wheel_input(
    mut wheel_events: MessageReader<MouseWheel>,
    hover_map: Res<HoverMap>,
    pause_menu: Res<PauseMenuState>,
    mut left_viewport_query: Query<
        (&mut ScrollPosition, &Node, &ComputedNode),
        (With<LeftColumnScrollViewport>, Without<KeybindScrollViewport>),
    >,
    mut keybind_viewport_query: Query<
        (&mut ScrollPosition, &Node, &ComputedNode),
        (With<KeybindScrollViewport>, Without<LeftColumnScrollViewport>),
    >,
    hovered_settings: Query<&SettingRow>,
    hovered_keybinds: Query<(), With<KeybindRow>>,
) {
    if !pause_menu.open || !matches!(pause_menu.modal, PauseMenuModal::None) {
        return;
    }

    let mut hovered_left_region = false;
    let mut hovered_keybind_region = false;
    for pointer_map in hover_map.values() {
        for entity in pointer_map.keys().copied() {
            if hovered_settings.get(entity).is_ok() || left_viewport_query.get(entity).is_ok() {
                hovered_left_region = true;
            }
            if hovered_keybinds.get(entity).is_ok() || keybind_viewport_query.get(entity).is_ok() {
                hovered_keybind_region = true;
            }
        }
    }

    for wheel in wheel_events.read() {
        let delta = if wheel.unit == MouseScrollUnit::Line {
            wheel.y * 28.0
        } else {
            wheel.y
        };

        if hovered_left_region {
            let Ok((mut scroll, node, computed)) = left_viewport_query.single_mut() else {
                continue;
            };
            let max_offset = (computed.content_size() - computed.size()) * computed.inverse_scale_factor();
            if node.overflow.y == OverflowAxis::Scroll {
                scroll.y = (scroll.y - delta).clamp(0.0, max_offset.y.max(0.0));
            }
            continue;
        }

        if hovered_keybind_region {
            let Ok((mut scroll, node, computed)) = keybind_viewport_query.single_mut() else {
                continue;
            };
            let max_offset = (computed.content_size() - computed.size()) * computed.inverse_scale_factor();
            if node.overflow.y == OverflowAxis::Scroll {
                scroll.y = (scroll.y - delta).clamp(0.0, max_offset.y.max(0.0));
            }
        }
    }
}

fn keep_selected_keybind_visible(
    pause_menu: Res<PauseMenuState>,
    mut viewport_query: Query<(&mut ScrollPosition, &ComputedNode), With<KeybindScrollViewport>>,
) {
    let FocusTarget::Keybind(action) = pause_menu.focus else {
        return;
    };
    if !pause_menu.is_changed() || !pause_menu.open || !matches!(pause_menu.modal, PauseMenuModal::None) {
        return;
    }
    let index = KeyAction::ALL
        .iter()
        .position(|candidate| *candidate == action)
        .unwrap_or(0) as f32;
    let Ok((mut scroll, computed)) = viewport_query.single_mut() else {
        return;
    };
    let row_span = KEYBIND_ROW_HEIGHT + KEYBIND_ROW_GAP;
    let top = index * row_span;
    let bottom = top + KEYBIND_ROW_HEIGHT;
    let view_height = computed.size().y;
    if top < scroll.y {
        scroll.y = top.max(0.0);
    } else if bottom > scroll.y + view_height {
        scroll.y = (bottom - view_height).max(0.0);
    }
}

fn sync_pause_menu_visibility(
    mut commands: Commands,
    pause_menu: Res<PauseMenuState>,
    q_root: Query<Entity, With<PauseMenuRoot>>,
    q_modal_backdrop: Query<Entity, With<ModalBackdrop>>,
) {
    if !pause_menu.is_changed() {
        return;
    }

    if let Ok(root) = q_root.single() {
        commands.entity(root).insert(if pause_menu.open { Visibility::Visible } else { Visibility::Hidden });
    }
    let modal_visible = !matches!(pause_menu.modal, PauseMenuModal::None);
    if let Ok(entity) = q_modal_backdrop.single() {
        commands.entity(entity).insert(if modal_visible { Visibility::Visible } else { Visibility::Hidden });
    }
}

fn sync_pause_menu_hint(
    mut commands: Commands,
    pause_menu: Res<PauseMenuState>,
    q_hint_title: Query<Entity, With<SelectedHintTitle>>,
    q_hint_body: Query<Entity, With<SelectedHintBody>>,
) {
    if !pause_menu.is_changed() {
        return;
    }
    let (hint_title, hint_body) = selected_hint_text(pause_menu.focus);
    if let Ok(entity) = q_hint_title.single() {
        commands.entity(entity).insert(Text::new(hint_title));
    }
    if let Ok(entity) = q_hint_body.single() {
        commands.entity(entity).insert(Text::new(hint_body));
    }
}

fn sync_pause_menu_styles(
    mut commands: Commands,
    pause_menu: Res<PauseMenuState>,
    q_setting_rows: Query<(Entity, &SettingRow, &Interaction)>,
    q_keybind_rows: Query<(Entity, &KeybindRow, &Interaction)>,
    q_footer_buttons: Query<(Entity, &FooterButtonMarker, &Interaction)>,
    q_modal_buttons: Query<(Entity, &ModalButtonMarker, &Interaction)>,
) {
    if !pause_menu.is_changed() {
        return;
    }

    for (entity, row, interaction) in &q_setting_rows {
        let selected = pause_menu.focus == FocusTarget::Setting(row.0);
        apply_surface_style(&mut commands, entity, selected, *interaction);
    }
    for (entity, row, interaction) in &q_keybind_rows {
        let selected = pause_menu.focus == FocusTarget::Keybind(row.0);
        apply_surface_style(&mut commands, entity, selected, *interaction);
    }
    for (entity, button, interaction) in &q_footer_buttons {
        let selected = pause_menu.focus == FocusTarget::Footer(button.0);
        apply_footer_style(&mut commands, entity, selected, *interaction);
    }
    for (entity, button, interaction) in &q_modal_buttons {
        let selected = matches!(pause_menu.modal, PauseMenuModal::ConfirmResetAll { selected } if selected == button.0);
        apply_footer_style(&mut commands, entity, selected, *interaction);
    }
}

fn sync_pause_menu_values(
    mut commands: Commands,
    key_mapping: Res<KeyMapping>,
    movement: Res<MovementSettings>,
    audio: Res<AudioSettings>,
    graphics: Res<GraphicsSettings>,
    q_slider_fill: Query<(Entity, &SliderFill)>,
    q_value_text: Query<(Entity, &ValuePillText)>,
) {
    if !key_mapping.is_changed() && !movement.is_changed() && !audio.is_changed() && !graphics.is_changed() {
        return;
    }

    for (entity, fill) in &q_slider_fill {
        let fraction = fill.0.slider_fraction(&movement, &audio) * 100.0;
        commands.entity(entity).insert(Node {
            width: Val::Percent(fraction),
            height: Val::Percent(100.0),
            ..default()
        });
    }

    for (entity, target) in &q_value_text {
        let text = match target.0 {
            ValueTarget::Setting(setting) => setting.value_label(&movement, &audio, &graphics),
            ValueTarget::Keybind(action) => key_mapping.binding_label(action),
        };
        commands.entity(entity).insert(Text::new(text));
    }
}

fn sync_pause_menu_modal(
    mut commands: Commands,
    pause_menu: Res<PauseMenuState>,
    q_modal_title: Query<Entity, With<ModalTitleText>>,
    q_modal_body: Query<Entity, With<ModalBodyText>>,
) {
    if !pause_menu.is_changed() {
        return;
    }

    if let Ok(entity) = q_modal_title.single() {
        commands.entity(entity).insert(Text::new(modal_title(pause_menu.modal)));
    }
    if let Ok(entity) = q_modal_body.single() {
        commands.entity(entity).insert(Text::new(modal_body(pause_menu.modal)));
    }
}

fn activate_footer(
    commands: &mut Commands,
    action: FooterAction,
    pause_menu: &mut PauseMenuState,
    _key_mapping: &mut KeyMapping,
    q_player: &mut Query<(&mut Transform, &mut Velocity), With<Player>>,
    q_cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
    game_state: &mut GameState,
) {
    match action {
        FooterAction::Resume => close_pause_menu(commands, pause_menu, q_player, q_cursor, game_state),
        FooterAction::ResetAllBindings => {
            pause_menu.modal = PauseMenuModal::ConfirmResetAll {
                selected: ConfirmChoice::Cancel,
            };
        }
    }
}

fn close_pause_menu(
    commands: &mut Commands,
    pause_menu: &mut PauseMenuState,
    q_player: &mut Query<(&mut Transform, &mut Velocity), With<Player>>,
    q_cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
    game_state: &mut GameState,
) {
    pause_menu.open = false;
    pause_menu.modal = PauseMenuModal::None;
    pause_menu.capture_armed = false;
    if pause_menu.resume_on_close {
        if let Ok((mut transform, mut velocity)) = q_player.single_mut() {
            set_hard_paused(commands, game_state, &mut transform, &mut velocity, false);
        }
    }
    pause_menu.resume_on_close = false;
    if let Ok(mut cursor_options) = q_cursor.single_mut() {
        set_grab_mode(&mut cursor_options, CursorGrabMode::Locked);
    }
}

fn move_focus_vertical(current: FocusTarget, direction: i32) -> FocusTarget {
    match current {
        FocusTarget::Setting(setting) => {
            let idx = SettingItem::ALL.iter().position(|item| *item == setting).unwrap_or(0) as i32;
            let next = idx + direction;
            if next < 0 {
                FocusTarget::Footer(FooterAction::ResetAllBindings)
            } else if next >= SettingItem::ALL.len() as i32 {
                FocusTarget::Keybind(KeyAction::ALL[0])
            } else {
                FocusTarget::Setting(SettingItem::ALL[next as usize])
            }
        }
        FocusTarget::Keybind(action) => {
            let idx = KeyAction::ALL.iter().position(|item| *item == action).unwrap_or(0) as i32;
            let next = idx + direction;
            if next < 0 {
                FocusTarget::Setting(*SettingItem::ALL.last().unwrap())
            } else if next >= KeyAction::ALL.len() as i32 {
                FocusTarget::Footer(FooterAction::Resume)
            } else {
                FocusTarget::Keybind(KeyAction::ALL[next as usize])
            }
        }
        FocusTarget::Footer(action) => {
            if direction < 0 {
                FocusTarget::Keybind(*KeyAction::ALL.last().unwrap())
            } else {
                FocusTarget::Setting(SettingItem::MouseSensitivity)
            }
        }
    }
}

impl FocusTarget {
    fn clamp_valid(self) -> Self {
        match self {
            Self::Setting(setting) if SettingItem::ALL.contains(&setting) => self,
            Self::Keybind(action) if KeyAction::ALL.contains(&action) => self,
            Self::Footer(action) if FooterAction::ALL.contains(&action) => self,
            _ => FocusTarget::Setting(SettingItem::MouseSensitivity),
        }
    }
}

fn selected_hint_text(focus: FocusTarget) -> (&'static str, &'static str) {
    match focus {
        FocusTarget::Setting(setting) => (setting.label(), setting.hint()),
        FocusTarget::Keybind(action) => (action.label(), "Click or press Enter to capture a new binding. R resets the focused binding."),
        FocusTarget::Footer(FooterAction::Resume) => ("Resume", "Closes the pause menu and returns to first-person play."),
        FocusTarget::Footer(FooterAction::ResetAllBindings) => ("Reset All Bindings", "Prompts for confirmation, then restores the default keyboard layout."),
    }
}

fn modal_title(modal: PauseMenuModal) -> String {
    match modal {
        PauseMenuModal::None => String::new(),
        PauseMenuModal::CaptureKey(action) => format!("Bind {}", action.label()),
        PauseMenuModal::ConfirmResetAll { .. } => "Reset all bindings?".to_string(),
    }
}

fn modal_body(modal: PauseMenuModal) -> String {
    match modal {
        PauseMenuModal::None => String::new(),
        PauseMenuModal::CaptureKey(action) => format!(
            "Press a new key to bind to {}.\nEsc cancels without changing the current binding.",
            action.label()
        ),
        PauseMenuModal::ConfirmResetAll { .. } => "This resets every keyboard binding to its default layout.".to_string(),
    }
}

fn apply_surface_style(commands: &mut Commands, entity: Entity, selected: bool, interaction: Interaction) {
    let hovered = interaction == Interaction::Hovered;
    let pressed = interaction == Interaction::Pressed;
    let bg = if pressed {
        Color::srgba(0.2, 0.24, 0.31, 0.98)
    } else if selected {
        Color::srgba(0.16, 0.2, 0.28, 0.94)
    } else if hovered {
        Color::srgba(0.135, 0.16, 0.22, 0.92)
    } else {
        Color::srgba(0.11, 0.12, 0.16, 0.84)
    };
    let border = if selected || hovered {
        Color::srgba(0.72, 0.8, 0.98, 0.44)
    } else {
        Color::srgba(0.82, 0.85, 0.92, 0.08)
    };
    commands.entity(entity)
        .insert(BackgroundColor(bg))
        .insert(BorderColor::all(border));
}

fn apply_footer_style(commands: &mut Commands, entity: Entity, selected: bool, interaction: Interaction) {
    let hovered = interaction == Interaction::Hovered;
    let pressed = interaction == Interaction::Pressed;
    let bg = if pressed {
        Color::srgba(0.26, 0.31, 0.42, 1.0)
    } else if selected {
        Color::srgba(0.19, 0.24, 0.34, 0.98)
    } else if hovered {
        Color::srgba(0.16, 0.2, 0.28, 0.98)
    } else {
        Color::srgba(0.14, 0.16, 0.22, 0.95)
    };
    let border = if selected || hovered {
        Color::srgba(0.78, 0.84, 0.98, 0.5)
    } else {
        Color::srgba(0.78, 0.82, 0.92, 0.14)
    };
    commands.entity(entity)
        .insert(BackgroundColor(bg))
        .insert(BorderColor::all(border));
}

fn previous_footer(current: FooterAction) -> FooterAction {
    match current {
        FooterAction::Resume => FooterAction::ResetAllBindings,
        FooterAction::ResetAllBindings => FooterAction::Resume,
    }
}

fn next_footer(current: FooterAction) -> FooterAction {
    previous_footer(current)
}

fn row_node() -> Node {
    Node {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        ..default()
    }
}

fn card_node() -> Node {
    Node {
        padding: UiRect::all(Val::Px(16.0)),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(8.0),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

fn row_surface_node(height: f32) -> Node {
    Node {
        width: Val::Percent(100.0),
        min_height: Val::Px(height),
        padding: UiRect::all(Val::Px(12.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        border: UiRect::all(Val::Px(1.0)),
        ..row_node()
    }
}

fn value_pill_node() -> impl Bundle {
    (
        Node {
            min_width: Val::Px(92.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.13, 0.15, 0.2, 1.0)),
        BorderColor::all(Color::srgba(0.84, 0.87, 0.94, 0.12)),
        BorderRadius::all(Val::Px(99.0)),
    )
}

fn heading_text(font: Handle<Font>, size: f32) -> TextFont {
    TextFont {
        font,
        font_size: size,
        ..default()
    }
}

fn body_text(font: Handle<Font>, size: f32) -> TextFont {
    TextFont {
        font,
        font_size: size,
        ..default()
    }
}

fn bool_label(value: bool) -> String {
    if value { "On" } else { "Off" }.to_string()
}

fn normalize(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn just_pressed_question(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::Slash)
        && (input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight))
}

fn pressed_up(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::ArrowUp) || input.just_pressed(KeyCode::KeyW)
}

fn pressed_down(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::ArrowDown) || input.just_pressed(KeyCode::KeyS)
}

fn pressed_left(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::ArrowLeft) || input.just_pressed(KeyCode::KeyA)
}

fn pressed_right(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::ArrowRight) || input.just_pressed(KeyCode::KeyD)
}

fn pressed_enter(input: &ButtonInput<KeyCode>) -> bool {
    input.just_pressed(KeyCode::Enter) || input.just_pressed(KeyCode::NumpadEnter)
}
