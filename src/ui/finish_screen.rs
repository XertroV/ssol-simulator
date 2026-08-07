use bevy::{camera::visibility::{InheritedVisibility, ViewVisibility}, prelude::*};

use crate::{
    audio::PlayWhiteArchPassSound,
    game_state::{FinishReached, GameState, GameWon, OrbSplit},
    player::PlayerRespawnRequest,
    ui::{
        PauseMenuState, is_pause_menu_open,
        theme::{self, text_font},
    },
};

pub struct FinishScreenUiPlugin;

impl Plugin for FinishScreenUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FinishFlowState>()
            .add_systems(Startup, setup_finish_ui)
            .add_systems(Update, (trigger_finish_on_click, animate_win_flash, sync_finish_ui))
            .add_observer(on_game_won)
            .add_observer(on_finish_reached)
            .add_observer(on_player_respawn_request);
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FinishFlowState {
    pub phase: FinishPhase,
    pub flash_alpha: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FinishPhase {
    #[default]
    NotWon,
    WonRoaming,
    EndOverlayOpen,
}

#[derive(Component)]
struct WinHudRoot;

#[derive(Component)]
struct WinStatusText;

#[derive(Component)]
struct WinHintText;

#[derive(Component)]
struct WinLocalValueText;

#[derive(Component)]
struct WinWorldValueText;

#[derive(Component)]
struct EndOverlayRoot;

#[derive(Component)]
struct EndLocalValueText;

#[derive(Component)]
struct EndWorldValueText;

#[derive(Component)]
struct EndTableText;

#[derive(Component)]
struct EndHintText;

#[derive(Component)]
struct WhiteFlashOverlay;

fn setup_finish_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");
    let caption = text_font(font.clone(), 13.0);
    let label_font = text_font(font.clone(), 16.0);
    let medium_font = text_font(font.clone(), 22.0);
    let big_font = text_font(font.clone(), 36.0);
    let huge_font = text_font(font.clone(), 48.0);
    let table_font = TextFont {
        // Match Bevy's default monospace font used by Perf UI.
        font: Handle::default().into(),
        font_size: FontSize::Px(13.0),
        ..default()
    };

    // Compact win chip — top-left, does not steal FOV
    commands
        .spawn((
            WinHudRoot,
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            GlobalZIndex(920),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                left: Val::Px(14.0),
                max_width: Val::Px(340.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(theme::RADIUS_MD)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(theme::PANEL),
            BorderColor::all(theme::BORDER_STRONG),
        ))
        .with_children(|root| {
            root.spawn((
                WinStatusText,
                Text::new("All Orbs Collected"),
                caption.clone(),
                TextColor(theme::SUCCESS),
            ));
            root.spawn((
                Text::new("local"),
                caption.clone(),
                TextColor(theme::TEXT_MUTED),
            ));
            root.spawn((
                WinLocalValueText,
                Text::new("00:00:00.000"),
                big_font.clone(),
                TextColor(theme::TEXT),
            ));
            root.spawn((
                Text::new("world"),
                caption.clone(),
                TextColor(theme::TEXT_MUTED),
            ));
            root.spawn((
                WinWorldValueText,
                Text::new("00:00:00.000"),
                medium_font.clone(),
                TextColor(theme::TEXT_DIM),
            ));
            root.spawn((
                WinHintText,
                Text::new("Click or pass through the white arch"),
                caption.clone(),
                TextColor(theme::TEXT_MUTED),
            ));
        });

    commands
        .spawn((
            EndOverlayRoot,
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            GlobalZIndex(930),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(20.0)),
                ..default()
            },
            BackgroundColor(theme::SCRIM),
        ))
        .with_children(|root| {
            root.spawn((
                InheritedVisibility::default(),
                ViewVisibility::default(),
                Node {
                    width: Val::Px(720.0),
                    max_width: Val::Percent(92.0),
                    max_height: Val::Percent(90.0),
                    padding: UiRect::axes(Val::Px(22.0), Val::Px(18.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(theme::RADIUS_LG)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::BORDER_STRONG),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("RUN COMPLETE"),
                    caption.clone(),
                    TextColor(theme::ACCENT),
                ));
                panel.spawn((
                    EndLocalValueText,
                    Text::new("00:00:00.000"),
                    huge_font,
                    TextColor(theme::TEXT),
                ));
                panel.spawn((
                    Text::new("local time"),
                    caption.clone(),
                    TextColor(theme::TEXT_MUTED),
                ));
                panel.spawn((
                    EndWorldValueText,
                    Text::new("00:00:00.000"),
                    big_font,
                    TextColor(theme::TEXT_DIM),
                ));
                panel.spawn((
                    Text::new("world time"),
                    caption.clone(),
                    TextColor(theme::TEXT_MUTED),
                ));
                panel.spawn((
                    InheritedVisibility::default(),
                    ViewVisibility::default(),
                    Node {
                        width: Val::Percent(100.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        margin: UiRect::top(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(theme::RADIUS_SM)),
                        ..default()
                    },
                    BackgroundColor(theme::WELL),
                    BorderColor::all(theme::BORDER),
                ))
                .with_children(|table_wrap| {
                    table_wrap.spawn((
                        EndTableText,
                        Text::new("Waiting for orb history..."),
                        table_font,
                        TextColor(theme::TEXT_DIM),
                    ));
                });
                panel.spawn((
                    EndHintText,
                    Text::new("Press Backspace to reset"),
                    label_font,
                    TextColor(theme::TEXT_MUTED),
                ));
            });
        });

    commands.spawn((
        WhiteFlashOverlay,
        Visibility::Hidden,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        GlobalZIndex(925),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
    ));
}

fn on_game_won(_trigger: On<GameWon>, mut finish_state: ResMut<FinishFlowState>) {
    finish_state.phase = FinishPhase::WonRoaming;
    finish_state.flash_alpha = u8::MAX;
}

fn on_finish_reached(
    _trigger: On<FinishReached>,
    mut commands: Commands,
    game_state: Res<GameState>,
    mut finish_state: ResMut<FinishFlowState>,
) {
    if !game_state.game_win || finish_state.phase == FinishPhase::EndOverlayOpen {
        return;
    }
    finish_state.phase = FinishPhase::EndOverlayOpen;
    commands.trigger(PlayWhiteArchPassSound);
}

fn on_player_respawn_request(
    _trigger: On<PlayerRespawnRequest>,
    mut finish_state: ResMut<FinishFlowState>,
) {
    finish_state.phase = FinishPhase::NotWon;
    finish_state.flash_alpha = 0;
}

fn trigger_finish_on_click(
    mut commands: Commands,
    mouse_input: Res<ButtonInput<MouseButton>>,
    pause_menu: Option<Res<PauseMenuState>>,
    game_state: Res<GameState>,
    finish_state: Res<FinishFlowState>,
) {
    if is_pause_menu_open(pause_menu.as_deref()) {
        return;
    }
    if !game_state.game_win || finish_state.phase != FinishPhase::WonRoaming {
        return;
    }
    if mouse_input.just_pressed(MouseButton::Left) || mouse_input.just_pressed(MouseButton::Right) {
        commands.trigger(FinishReached);
    }
}

fn animate_win_flash(mut finish_state: ResMut<FinishFlowState>, time: Res<Time>) {
    if finish_state.flash_alpha == 0 {
        return;
    }

    let next = finish_state.flash_alpha as f32 - (255.0 * time.delta_secs() / 1.6);
    finish_state.flash_alpha = next.max(0.0).round() as u8;
}

fn sync_finish_ui(
    game_state: Res<GameState>,
    finish_state: Res<FinishFlowState>,
    mut visibility_set: ParamSet<(
        Query<&mut Visibility, (With<WinHudRoot>, Without<EndOverlayRoot>)>,
        Query<&mut Visibility, (With<EndOverlayRoot>, Without<WinHudRoot>)>,
        Query<(&mut Visibility, &mut BackgroundColor), With<WhiteFlashOverlay>>,
    )>,
    mut text_set: ParamSet<(
        Query<&mut Text, With<WinStatusText>>,
        Query<&mut Text, With<WinHintText>>,
        Query<&mut Text, (With<WinLocalValueText>, Without<WinWorldValueText>)>,
        Query<&mut Text, (With<WinWorldValueText>, Without<WinLocalValueText>)>,
        Query<&mut Text, (With<EndLocalValueText>, Without<EndWorldValueText>)>,
        Query<&mut Text, (With<EndWorldValueText>, Without<EndLocalValueText>)>,
        Query<&mut Text, With<EndTableText>>,
        Query<&mut Text, With<EndHintText>>,
    )>,
    mut color_set: ParamSet<(
        Query<&mut TextColor, With<WinWorldValueText>>,
        Query<&mut TextColor, With<EndWorldValueText>>,
    )>,
) {
    let win_visible = finish_state.phase == FinishPhase::WonRoaming;
    let end_visible = finish_state.phase == FinishPhase::EndOverlayOpen;

    if let Ok(mut visibility) = visibility_set.p0().single_mut() {
        *visibility = if win_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = visibility_set.p1().single_mut() {
        *visibility = if end_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok((mut visibility, mut color)) = visibility_set.p2().single_mut() {
        let alpha = finish_state.flash_alpha as f32 / 255.0;
        *visibility = if alpha > 0.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        color.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
    }

    let local_time = format_full_time(game_state.player_time);
    let world_time = format_full_time(game_state.world_time);
    let cheated_world_color = if game_state.game_win && game_state.used_cheat_99_orbs {
        Color::srgba(0.98, 0.34, 0.34, 0.98)
    } else {
        Color::srgba(0.88, 0.91, 0.96, 0.96)
    };
    let cheated_world_hud_color = if game_state.game_win && game_state.used_cheat_99_orbs {
        Color::srgba(0.98, 0.34, 0.34, 0.94)
    } else {
        Color::srgba(0.88, 0.91, 0.96, 0.92)
    };

    if let Ok(mut text) = text_set.p0().single_mut() {
        **text = match finish_state.phase {
            FinishPhase::WonRoaming => "All Orbs Collected".to_string(),
            FinishPhase::EndOverlayOpen => "Run Complete".to_string(),
            FinishPhase::NotWon => String::new(),
        };
    }
    if let Ok(mut text) = text_set.p1().single_mut() {
        **text = match finish_state.phase {
            FinishPhase::WonRoaming => "Click or pass through the white arch".to_string(),
            FinishPhase::EndOverlayOpen => "Press Backspace to reset".to_string(),
            FinishPhase::NotWon => String::new(),
        };
    }
    if let Ok(mut text) = text_set.p2().single_mut() {
        **text = local_time.clone();
    }
    if let Ok(mut text) = text_set.p3().single_mut() {
        **text = world_time.clone();
    }
    if let Ok(mut color) = color_set.p0().single_mut() {
        *color = TextColor(cheated_world_hud_color);
    }
    if let Ok(mut text) = text_set.p4().single_mut() {
        **text = local_time.clone();
    }
    if let Ok(mut text) = text_set.p5().single_mut() {
        **text = world_time.clone();
    }
    if let Ok(mut color) = color_set.p1().single_mut() {
        *color = TextColor(cheated_world_color);
    }
    if let Ok(mut text) = text_set.p6().single_mut() {
        **text = format_orb_table(&game_state.orb_splits);
    }
    if let Ok(mut text) = text_set.p7().single_mut() {
        **text = "Press Backspace to reset".to_string();
    }
}

fn format_full_time(time: f32) -> String {
    let total_ms = (time.max(0.0) * 1000.0).round() as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let seconds = (total_ms / 1_000) % 60;
    let millis = total_ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn format_compact_time(time: f32) -> String {
    let total_ms = (time.max(0.0) * 1000.0).round() as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let seconds = (total_ms / 1_000) % 60;
    let millis = total_ms % 1_000;
    if hours > 0 {
        format!("{hours:01}:{minutes:02}:{seconds:02}.{millis:03}")
    } else {
        format!("{minutes:02}:{seconds:02}.{millis:03}")
    }
}

fn format_orb_table(splits: &[OrbSplit]) -> String {
    if splits.is_empty() {
        return "No orb history yet.".to_string();
    }

    // Compact single-column table: dual columns wrapped poorly in a narrow panel.
    let mut lines = vec!["#  Orb   Local      +Split     World".to_string()];
    for split in splits {
        lines.push(format!(
            "{:>3} {:>4} {:>10} {:>10} {:>10}",
            split.sequence_index,
            split.orb_id.0 + 1,
            format_compact_time(split.player_time),
            format_compact_time(split.player_split_delta),
            format_compact_time(split.world_time),
        ));
    }
    lines.join("\n")
}
