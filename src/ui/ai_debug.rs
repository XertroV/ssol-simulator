//! AI Debug UI - Shows reward breakdown when AI mode is enabled

use bevy::prelude::*;

use crate::ai::{AiEpisodeControl, AiObservations, AiRewardSignal};
use crate::SimConfig;

pub struct AiDebugUiPlugin;

impl Plugin for AiDebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastEpisodeCount>()
            .add_systems(Startup, setup_ai_debug_ui.run_if(is_ai_mode_enabled))
            .add_systems(Update, (update_ai_debug_ui, update_orb_checklist_ui, update_closest_orb_ui).run_if(is_ai_mode_enabled));
    }
}

/// Run condition that checks if AI mode is enabled
fn is_ai_mode_enabled(config: Res<SimConfig>) -> bool {
    config.ai_mode
}

/// Resource to track last episode count for detecting episode resets
#[derive(Resource, Default)]
struct LastEpisodeCount(u32);

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

/// Marker component for closest orb ID text
#[derive(Component)]
struct ClosestOrbIdText;

/// Marker component for closest orb distance text
#[derive(Component)]
struct ClosestOrbDistanceText;

/// Marker component for closest orb direction text
#[derive(Component)]
struct ClosestOrbDirectionText;

/// Marker component for the orb checklist container
#[derive(Component)]
struct OrbChecklistContainer;

/// Marker component for individual orb indicator bars (stores orb index)
#[derive(Component)]
struct OrbIndicator(usize);

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
                left: Val::Px(0.0),
                top: Val::Percent(35.0),
                align_content: AlignContent::Center,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            // BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            // BorderRadius::all(Val::Px(8.0)),
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

            // Divider for closest orb section
            panel.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });

            // Section title for closest orb info
            panel.spawn((
                Text::new("Closest Orb Target"),
                TextFont {
                    font: font.clone(),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgba(0.3, 0.9, 1.0, 1.0)),
            ));

            // Closest Orb ID Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Orb ID:"), label_font.clone(), label_color));
                    row.spawn((
                        ClosestOrbIdText,
                        Text::new("--"),
                        value_font.clone(),
                        value_color,
                    ));
                });

            // Closest Orb Distance Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Distance:"), label_font.clone(), label_color));
                    row.spawn((
                        ClosestOrbDistanceText,
                        Text::new("--"),
                        value_font.clone(),
                        value_color,
                    ));
                });

            // Closest Orb Direction Row (local coordinates)
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Dir (local):"), label_font.clone(), label_color));
                    row.spawn((
                        ClosestOrbDirectionText,
                        Text::new("(---, ---, ---)"),
                        value_font.clone(),
                        value_color,
                    ));
                });
        });

    // Orb Checklist Panel - 100 vertical bars at bottom of screen
    commands
        .spawn((
            OrbChecklistContainer,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                bottom: Val::Px(64.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ))
        .with_children(|container| {
            // Label
            container.spawn((
                Text::new("Orb Checklist (sent to AI)"),
                TextFont {
                    font: font.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(0.8, 0.8, 0.8, 0.9)),
            ));

            // Bars container - two rows of 50 bars each
            container
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                })
                .with_children(|bars_container| {
                    // First row (orbs 0-49)
                    bars_container
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(1.0),
                            ..default()
                        })
                        .with_children(|row| {
                            for i in 0..50 {
                                row.spawn((
                                    OrbIndicator(i),
                                    Node {
                                        width: Val::Px(4.0),
                                        height: Val::Px(20.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.3, 0.3, 0.3, 0.8)), // Gray = inactive
                                ));
                            }
                        });

                    // Second row (orbs 50-99)
                    bars_container
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(1.0),
                            ..default()
                        })
                        .with_children(|row| {
                            for i in 50..100 {
                                row.spawn((
                                    OrbIndicator(i),
                                    Node {
                                        width: Val::Px(4.0),
                                        height: Val::Px(20.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.3, 0.3, 0.3, 0.8)), // Gray = inactive
                                ));
                            }
                        });
                });
        });
}

fn update_ai_debug_ui(
    mut commands: Commands,
    reward_signal: Res<AiRewardSignal>,
    episode_control: Res<AiEpisodeControl>,
    mut last_episode: ResMut<LastEpisodeCount>,
    q_total: Query<Entity, With<AiRewardTotalText>>,
    q_time: Query<Entity, With<AiTimePenaltyText>>,
    q_orb: Query<Entity, With<AiOrbRewardText>>,
    q_momentum: Query<Entity, With<AiMomentumBonusText>>,
    q_pitch: Query<Entity, With<AiPitchPenaltyText>>,
) {
    // Detect episode reset and show zeroed values on first frame of new episode
    let is_new_episode = episode_control.episode_count != last_episode.0;
    if is_new_episode {
        last_episode.0 = episode_control.episode_count;
    }

    // Use current values (they reset each step via reset_step, so always show current step values)
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

/// Update the closest orb info display from AiObservations::orb_targets[0]
fn update_closest_orb_ui(
    mut commands: Commands,
    observations: Res<AiObservations>,
    q_orb_id: Query<Entity, With<ClosestOrbIdText>>,
    q_orb_dist: Query<Entity, With<ClosestOrbDistanceText>>,
    q_orb_dir: Query<Entity, With<ClosestOrbDirectionText>>,
) {
    // orb_targets[0] is the closest orb: (direction_local, distance, orb_id)
    // orb_id is -1.0 for empty/no target
    // distance > 10000 indicates uninitialized/garbage data
    let (direction, distance, orb_id) = observations.orb_targets[0];

    let has_target = orb_id >= 0.0 && distance < 10000.0;

    // Update orb ID
    if let Ok(entity) = q_orb_id.single() {
        let text = if has_target {
            format!("{}", orb_id as i32)
        } else {
            "--".to_string()
        };
        commands.entity(entity).insert(Text::new(text));
    }

    // Update distance
    if let Ok(entity) = q_orb_dist.single() {
        let text = if has_target {
            format!("{:.1}m", distance)
        } else {
            "--".to_string()
        };
        commands.entity(entity).insert(Text::new(text));
    }

    // Update direction (in local player coordinates)
    if let Ok(entity) = q_orb_dir.single() {
        let text = if has_target {
            format!("({:.2}, {:.2}, {:.2})", direction.x, direction.y, direction.z)
        } else {
            "(---, ---, ---)".to_string()
        };
        commands.entity(entity).insert(Text::new(text));
    }
}

/// Update the orb checklist visualization based on AiObservations
fn update_orb_checklist_ui(
    observations: Res<AiObservations>,
    mut q_indicators: Query<(&OrbIndicator, &mut BackgroundColor)>,
) {
    // Active = green, collected/inactive = gray
    let active_color = Color::srgba(0.3, 1.0, 0.3, 0.9);
    let inactive_color = Color::srgba(0.3, 0.3, 0.3, 0.8);

    for (indicator, mut bg) in q_indicators.iter_mut() {
        let orb_idx = indicator.0;
        if orb_idx < 100 {
            let is_active = observations.orb_checklist[orb_idx] > 0.5;
            *bg = if is_active {
                BackgroundColor(active_color)
            } else {
                BackgroundColor(inactive_color)
            };
        }
    }
}
