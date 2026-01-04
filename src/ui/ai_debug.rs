//! AI Debug UI - Shows reward breakdown when AI mode is enabled

use bevy::prelude::*;

use crate::ai::AiRewardSignal;
use crate::SimConfig;

pub struct AiDebugUiPlugin;

impl Plugin for AiDebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ai_debug_ui.run_if(is_ai_mode_enabled))
            .add_systems(Update, update_ai_debug_ui.run_if(is_ai_mode_enabled));
    }
}

/// Run condition that checks if AI mode is enabled
fn is_ai_mode_enabled(config: Res<SimConfig>) -> bool {
    config.ai_mode
}

/// Marker component for the AI debug panel root
#[derive(Component)]
struct AiDebugPanel;

/// Marker component for the total reward text
#[derive(Component)]
struct AiRewardTotalText;

/// Marker component for the time penalty text
#[derive(Component)]
struct AiTimePenaltyText;

/// Marker component for the orb reward text
#[derive(Component)]
struct AiOrbRewardText;

/// Marker component for the momentum bonus text
#[derive(Component)]
struct AiMomentumBonusText;

/// Marker component for the pitch penalty text
#[derive(Component)]
struct AiPitchPenaltyText;

fn setup_ai_debug_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");

    let label_font = TextFont {
        font: font.clone(),
        font_size: 16.0,
        ..default()
    };
    let value_font = TextFont {
        font: font.clone(),
        font_size: 18.0,
        ..default()
    };

    let label_color = TextColor(Color::srgba(0.7, 0.7, 0.7, 0.9));
    let value_color = TextColor(Color::srgba(1.0, 1.0, 1.0, 0.95));
    let positive_color = TextColor(Color::srgba(0.3, 1.0, 0.3, 0.95));
    let negative_color = TextColor(Color::srgba(1.0, 0.4, 0.4, 0.95));

    // Root panel - left side, vertically centered
    commands
        .spawn((
            AiDebugPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Percent(50.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            BorderRadius::all(Val::Px(8.0)),
        ))
        .with_children(|panel| {
            // Title
            panel.spawn((
                Text::new("AI Reward Breakdown"),
                TextFont {
                    font: font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.3, 1.0)),
            ));

            // Divider-like spacing
            panel.spawn(Node {
                height: Val::Px(4.0),
                ..default()
            });

            // Total Reward Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Total Step Reward:"), label_font.clone(), label_color));
                    row.spawn((
                        AiRewardTotalText,
                        Text::new("0.000"),
                        value_font.clone(),
                        value_color,
                    ));
                });

            // Divider
            panel.spawn(Node {
                height: Val::Px(2.0),
                ..default()
            });

            // Orb Collection Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Orb Collection:"), label_font.clone(), label_color));
                    row.spawn((
                        AiOrbRewardText,
                        Text::new("+0.000"),
                        value_font.clone(),
                        positive_color,
                    ));
                });

            // Momentum Bonus Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Momentum Bonus:"), label_font.clone(), label_color));
                    row.spawn((
                        AiMomentumBonusText,
                        Text::new("+0.000"),
                        value_font.clone(),
                        positive_color,
                    ));
                });

            // Time Penalty Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Time Penalty:"), label_font.clone(), label_color));
                    row.spawn((
                        AiTimePenaltyText,
                        Text::new("-0.000"),
                        value_font.clone(),
                        negative_color,
                    ));
                });

            // Camera Pitch Penalty Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Pitch Penalty:"), label_font.clone(), label_color));
                    row.spawn((
                        AiPitchPenaltyText,
                        Text::new("-0.000"),
                        value_font.clone(),
                        negative_color,
                    ));
                });
        });
}

fn update_ai_debug_ui(
    mut commands: Commands,
    reward_signal: Res<AiRewardSignal>,
    q_total: Query<Entity, With<AiRewardTotalText>>,
    q_time: Query<Entity, With<AiTimePenaltyText>>,
    q_orb: Query<Entity, With<AiOrbRewardText>>,
    q_momentum: Query<Entity, With<AiMomentumBonusText>>,
    q_pitch: Query<Entity, With<AiPitchPenaltyText>>,
) {
    // Update total reward
    if let Ok(entity) = q_total.single() {
        let value = reward_signal.step_reward;
        let text = if value >= 0.0 {
            format!("+{:.3}", value)
        } else {
            format!("{:.3}", value)
        };
        let color = if value >= 0.0 {
            Color::srgba(0.3, 1.0, 0.3, 0.95)
        } else {
            Color::srgba(1.0, 0.4, 0.4, 0.95)
        };
        commands.entity(entity).insert((Text::new(text), TextColor(color)));
    }

    // Update time penalty (always negative)
    if let Ok(entity) = q_time.single() {
        let text = format!("{:.3}", reward_signal.time_penalty);
        commands.entity(entity).insert(Text::new(text));
    }

    // Update orb reward (always positive or zero)
    if let Ok(entity) = q_orb.single() {
        let text = format!("+{:.3}", reward_signal.orb_reward);
        commands.entity(entity).insert(Text::new(text));
    }

    // Update momentum bonus (always positive or zero)
    if let Ok(entity) = q_momentum.single() {
        let text = format!("+{:.3}", reward_signal.momentum_bonus);
        commands.entity(entity).insert(Text::new(text));
    }

    // Update pitch penalty (always negative or zero)
    if let Ok(entity) = q_pitch.single() {
        let text = format!("{:.3}", reward_signal.pitch_penalty);
        commands.entity(entity).insert(Text::new(text));
    }
}
