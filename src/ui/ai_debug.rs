//! AI Debug UI - Shows reward breakdown when AI mode is enabled

use std::f32::consts::PI;

use bevy::prelude::*;

use crate::ai::{AiConfig, AiEpisodeControl, AiObservations, AiRewardSignal};
use crate::SimConfig;

pub struct AiDebugUiPlugin;

impl Plugin for AiDebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastEpisodeCount>()
            .add_systems(Startup, (setup_ai_debug_ui, setup_ray_donut_ui).run_if(is_ai_mode_enabled))
            .add_systems(Update, (
                update_ai_debug_ui,
                update_orb_checklist_ui,
                update_closest_orb_ui,
                update_waiting_indicator,
                update_ray_donut_ui,
                handle_ray_height_input,
            ).run_if(is_ai_mode_enabled));
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

/// Marker component for player position text
#[derive(Component)]
struct PlayerPositionText;

/// Marker component for the orb checklist container
#[derive(Component)]
struct OrbChecklistContainer;

/// Marker component for individual orb indicator bars (stores orb index)
#[derive(Component)]
struct OrbIndicator(usize);

/// Marker component for the "Waiting for AI..." indicator
#[derive(Component)]
struct WaitingIndicator;

/// Marker component for individual ray sector meshes (stores ray index 0-15)
#[derive(Component)]
struct RaySector(usize);

/// Marker component for the ray donut container entity
#[derive(Component)]
struct RayDonutContainer;

/// Marker component for the ray height offset text display
#[derive(Component)]
struct RayHeightOffsetText;

/// Marker component for the ray origin Y text display (actual value)
#[derive(Component)]
struct RayOriginYText;

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
                top: Val::Percent(15.0),
                align_content: AlignContent::Center,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
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

            // Divider for player position section
            panel.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });

            // Section title for player position
            panel.spawn((
                Text::new("Player Position"),
                TextFont {
                    font: font.clone(),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgba(0.3, 0.9, 1.0, 1.0)),
            ));

            // Player Position Row
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(24.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("Pos:"), label_font.clone(), label_color));
                    row.spawn((
                        PlayerPositionText,
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

    // Waiting for AI indicator - centered on screen, initially hidden
    commands.spawn((
        WaitingIndicator,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(40.0),
            left: Val::Percent(50.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(16.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        Visibility::Hidden,
        Text::new("Waiting for AI..."),
        TextFont {
            font: font.clone(),
            font_size: 32.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 0.9, 0.3, 1.0)),
    ));
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
    q_player_pos: Query<Entity, With<PlayerPositionText>>,
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

    // Update player position
    if let Ok(entity) = q_player_pos.single() {
        let pos = observations.player_position;
        let text = format!("({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z);
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

/// Update the "Waiting for AI..." indicator visibility
fn update_waiting_indicator(
    ai_config: Res<AiConfig>,
    mut q_waiting: Query<&mut Visibility, With<WaitingIndicator>>,
) {
    if let Ok(mut visibility) = q_waiting.single_mut() {
        *visibility = if ai_config.waiting_for_action {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Setup the 2D ray donut visualization on the right side of the screen
fn setup_ray_donut_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");

    // Donut parameters
    let num_rays = 16;
    let angle_per_ray = 2.0 * PI / num_rays as f32;

    // Spawn a container for the donut using Node for positioning
    commands
        .spawn((
            RayDonutContainer,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(100.0),
                top: Val::Percent(50.0),
                width: Val::Px(200.0),
                height: Val::Px(250.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|container| {
            // Label
            container.spawn((
                Text::new("Wall Rays (2D)"),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgba(0.8, 0.8, 0.8, 0.9)),
            ));

            // Ray height offset display
            container
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Height Offset:"),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.9)),
                    ));
                    row.spawn((
                        RayHeightOffsetText,
                        Text::new("-1.0"),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 0.5, 0.95)),
                    ));
                });

            // Ray origin Y display
            container
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    margin: UiRect::top(Val::Px(2.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Ray Origin Y:"),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.9)),
                    ));
                    row.spawn((
                        RayOriginYText,
                        Text::new("0.0"),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgba(0.5, 1.0, 1.0, 0.95)),
                    ));
                });

            // Instructions
            container.spawn((
                Text::new("[/] to adjust"),
                TextFont {
                    font: font.clone(),
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgba(0.5, 0.5, 0.5, 0.8)),
                Node {
                    margin: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
            ));

            // Spawn the ray visualization as a grid of colored bars
            // Arranged in a circular pattern using absolute positioning
            container.spawn(Node {
                height: Val::Px(10.0),
                ..default()
            });

            // Create a container for the donut visualization
            container
                .spawn(Node {
                    width: Val::Px(180.0),
                    height: Val::Px(180.0),
                    position_type: PositionType::Relative,
                    ..default()
                })
                .with_children(|donut_container| {
                    // Spawn 16 ray indicator bars arranged in a circle
                    // Since UI nodes don't support rotation via Transform,
                    // we use small square indicators positioned on a ring
                    for i in 0..num_rays {
                        let ray_angle = (i as f32) * angle_per_ray;

                        // Position on the ring
                        let ring_radius = 55.0;
                        let indicator_size = 22.0;

                        // Center of the container
                        let center_x = 90.0;
                        let center_y = 90.0;

                        // Position of the indicator on the ring
                        // Note: In UI coordinates, Y increases downward, so we negate sin
                        let indicator_x = center_x + ring_radius * ray_angle.cos() - indicator_size / 2.0;
                        let indicator_y = center_y - ring_radius * ray_angle.sin() - indicator_size / 2.0;

                        donut_container.spawn((
                            RaySector(i),
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(indicator_x),
                                top: Val::Px(indicator_y),
                                width: Val::Px(indicator_size),
                                height: Val::Px(indicator_size),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::hsl(60.0, 0.8, 0.5)),
                            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                        ));
                    }

                    // Add ray index labels around the outside
                    for i in [0, 4, 8, 12] {
                        let ray_angle = (i as f32) * angle_per_ray;
                        let label_radius = 85.0;
                        let label_x = 90.0 + label_radius * ray_angle.cos() - 8.0;
                        let label_y = 90.0 - label_radius * ray_angle.sin() - 8.0;

                        donut_container.spawn((
                            Text::new(format!("{}", i)),
                            TextFont {
                                font: font.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.6, 0.6, 0.6, 0.8)),
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(label_x),
                                top: Val::Px(label_y),
                                ..default()
                            },
                        ));
                    }
                });
        });
}

/// Update ray donut sector colors based on wall_rays values
fn update_ray_donut_ui(
    mut commands: Commands,
    observations: Res<AiObservations>,
    ai_config: Res<AiConfig>,
    mut q_sectors: Query<(&RaySector, &mut BackgroundColor)>,
    q_offset_text: Query<Entity, With<RayHeightOffsetText>>,
    q_origin_text: Query<Entity, With<RayOriginYText>>,
) {
    // Update sector colors
    for (sector, mut bg_color) in q_sectors.iter_mut() {
        let ray_idx = sector.0;
        if ray_idx < observations.wall_rays.len() {
            let distance = observations.wall_rays[ray_idx];

            // Color: red (0/close) -> yellow (0.5) -> green (1.0/far)
            // HSL hue: 0 = red, 60 = yellow, 120 = green
            let hue = distance * 120.0;
            let color = Color::hsl(hue, 0.9, 0.5);
            *bg_color = BackgroundColor(color);
        }
    }

    // Update height offset text
    if let Ok(entity) = q_offset_text.single() {
        let text = format!("{:.1}", ai_config.ray_height_offset);
        commands.entity(entity).insert(Text::new(text));
    }

    // Update ray origin Y text
    if let Ok(entity) = q_origin_text.single() {
        let text = format!("{:.1}", observations.ray_origin_y);
        commands.entity(entity).insert(Text::new(text));
    }
}

/// Handle keyboard input to adjust ray height offset
fn handle_ray_height_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut ai_config: ResMut<AiConfig>,
) {
    // Use [ and ] keys to adjust height offset
    // BracketLeft = '[', BracketRight = ']'
    let step = 0.25;

    if keyboard.just_pressed(KeyCode::BracketLeft) {
        ai_config.ray_height_offset -= step;
        info!("Ray height offset: {:.2}", ai_config.ray_height_offset);
    }

    if keyboard.just_pressed(KeyCode::BracketRight) {
        ai_config.ray_height_offset += step;
        info!("Ray height offset: {:.2}", ai_config.ray_height_offset);
    }
}
