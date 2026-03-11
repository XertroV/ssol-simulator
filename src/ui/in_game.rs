use bevy::color::palettes::tailwind::GRAY_700;
use bevy::prelude::*;
use iyes_perf_ui::prelude::PerfUiDefaultEntries;
use iyes_perf_ui::entries::PerfUiFixedTimeEntries;

use crate::ai_support::ActionCounter;
use crate::camera_switcher::FreeCamPerfUI;
use crate::config::GraphicsSettings;
use crate::game_state::GameState;

pub struct InGameUiPlugin;

impl Plugin for InGameUiPlugin {
    fn build(&self, app: &mut App) {
        app
            // .init_resource::<GameStats>()
            .init_resource::<BorderFlash>()
            .init_resource::<PhysicsTickCounter>()
            .init_resource::<ActionCounter>()
            .add_systems(Startup, (setup_ui, setup_fps_stats_ui))
            .add_systems(
                FixedUpdate,
                count_physics_ticks,
            )
            .add_systems(
                Update,
                (
                    update_orb_counter,
                    update_speedometer,
                    update_timer,
                    update_border_flash,
                    update_physics_tick_display,
                    sync_perf_ui_visibility,
                ),
            )
            .add_observer(on_ui_data_update)
            .add_observer(on_ui_flash);
    }
}

fn setup_fps_stats_ui(mut commands: Commands) {
    // Combined default entries and fixed timestep entries into one UI element
    commands.spawn((
        FreeCamPerfUI,
        PerfUiDefaultEntries::default(),
        PerfUiFixedTimeEntries::default(),
    ));

    // Physics tick counter UI - positioned bottom-left to avoid overlap
    commands.spawn((
        PhysicsTickText,
        FreeCamPerfUI,
        Text::new("Physics: 0 ticks/s"),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.0, 1.0, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            left: Val::Px(200.0),
            ..default()
        },
    ));
}

/// Resource to track physics ticks per second
#[derive(Resource)]
struct PhysicsTickCounter {
    ticks_this_second: u32,
    ticks_per_second: u32,
    last_update: f64,
}

impl Default for PhysicsTickCounter {
    fn default() -> Self {
        Self {
            ticks_this_second: 0,
            ticks_per_second: 0,
            last_update: 0.0,
        }
    }
}

#[derive(Component)]
struct PhysicsTickText;

/// Runs in FixedUpdate to count physics ticks - just increments counter
fn count_physics_ticks(mut counter: ResMut<PhysicsTickCounter>) {
    counter.ticks_this_second += 1;
}

/// Updates the physics tick display - runs in Update, uses Real time
fn update_physics_tick_display(
    mut counter: ResMut<PhysicsTickCounter>,
    mut action_counter: ResMut<ActionCounter>,
    time: Res<Time<Real>>,
    mut query: Query<&mut Text, With<PhysicsTickText>>,
) {
    let now = time.elapsed_secs_f64();
    let elapsed = now - counter.last_update;

    if elapsed >= 1.0 {
        counter.ticks_per_second = counter.ticks_this_second;
        counter.ticks_this_second = 0;
        action_counter.actions_per_second = action_counter.actions_this_second;
        action_counter.actions_this_second = 0;
        counter.last_update = now;
    }

    for mut text in &mut query {
        **text = format!(
            "Physics: {} ticks/s | Actions: {} /s",
            counter.ticks_per_second,
            action_counter.actions_per_second
        );
    }
}


#[derive(Component, Default)]
pub struct UiData;

#[derive(Component, Default, Clone, Copy)]
pub struct OrbUiData {
    pub orbs_collected: u32,
    pub orbs_total: u32,
}


#[derive(Event)]
pub enum OrbUiUpdateEvent {
    Orbs(OrbUiData),
}

#[derive(Resource)]
struct BorderFlash {
    timer: Option<Timer>,
    color: Color,
}

impl Default for BorderFlash {
    fn default() -> Self {
        Self {
            timer: None,
            color: Color::linear_rgb(1.0, 1.0, 0.0),
        }
    }
}

#[derive(Event, Debug, Clone, Copy)]
pub struct UiFlashEvent {
    pub color: Color,
    pub duration_secs: f32,
}

impl UiFlashEvent {
    pub fn warning() -> Self {
        Self {
            color: Color::linear_rgb(1.0, 0.55, 0.0),
            duration_secs: 0.8,
        }
    }
}

#[derive(Component)]
struct OrbCounterText;

#[derive(Component)]
struct SpeedAbsText;
#[derive(Component)]
struct MaxSpeedMultText;
#[derive(Component)]
struct SpeedOfLightText;

#[derive(Component)]
struct SpeedVsLightText;

#[derive(Component)]
struct TimerText;
#[derive(Component)]
struct WorldTimerText;

#[derive(Component)]
struct BorderFlashNode;

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");

    let font_c = (
        TextFont {
            font: font.clone(),
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::WHITE),
    );
    let big_font_c = (
        TextFont {
            font: font.clone(),
            font_size: 48.0,
            ..default()
        },
        TextColor(Color::WHITE),
    );

    let padding = UiRect::all(Val::Px(16.0));

    // Data node
    commands.spawn((UiData,));

    // Root node
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexStart,
            flex_direction: FlexDirection::Column,
            // padding: UiRect::all(Val::Vh(2.0)),
            ..default()
        })
        .with_children(|root| {
            // Top row
            root.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(20.0),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                padding,
                ..default()
            })
            .with_children(|top| {
                // Timer (top left)
                let mut timer_col = top.spawn(Node {
                    width: Val::Percent(50.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::FlexStart,
                    align_items: AlignItems::Start,
                    flex_direction: FlexDirection::Column,
                    ..default()
                });
                timer_col.with_children(|timer_col| {
                    timer_col.spawn((Text::new("00:00"), font_c.clone(), TimerText));
                    timer_col.spawn((Text::new("00:00"), font_c.clone().0, TextColor(GRAY_700.into())))
                        .insert((Visibility::Hidden, WorldTimerText));
                });
            });

            // Spacer (middle content)
            root.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });

            // Bottom row
            root.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(60.0),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexEnd,
                padding,
                ..default()
            })
            .with_children(|bottom| {
                // Orb counter (bottom left)
                bottom.spawn((
                    Text::new("Orbs: 0 / 0"),
                    big_font_c.clone(),
                    OrbCounterText,
                ));

                // Spectrum graph (bottom center)
                bottom
                    .spawn(Node {
                        width: Val::Percent(50.0),
                        height: Val::Px(20.0),
                        // background_color: BackgroundColor(Color::WHITE),
                        ..default()
                    })
                    .with_children(|spectrum| {
                        spectrum.spawn(Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            // background_color: BackgroundColor(Color::linear_rgb(1.0, 0.0, 0.0)),
                            ..default()
                        });
                    });

                // Speedometer (bottom right)
                bottom
                    .spawn((Node {
                        height: Val::Px(24.0 * 1.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::FlexEnd,
                        align_items: AlignItems::FlexEnd,
                        ..default()
                    },))
                    .with_children(|speeds| {
                        speeds.spawn((Text::new("c = 0 u/s"), SpeedOfLightText, font_c.clone()));
                        speeds.spawn((Text::new("0 x 0 u/s"), MaxSpeedMultText, font_c.clone()));
                        speeds.spawn((Text::new("0.0 % c"), SpeedVsLightText, font_c.clone()));
                        speeds.spawn((Text::new("0.00 u/s"), SpeedAbsText, font_c.clone()));
                    });
            });
        });

    // Border flash overlay
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            border: UiRect::all(Val::Vh(1.5)),
            ..default()
        },
        BorderColor::all(Color::NONE),
        BorderFlashNode,
    ));
}

// fn update_orb_counter(q_new_data: Query<&OrbUiData, Changed<OrbUiData>>, mut q_text: Query<&mut Text, With<OrbCounterText>>) {
//     let Ok(data) = q_new_data.single() else { return };
//     let Ok(mut text) = q_text.single_mut() else { return };
//     *text = Text::new(format!("{} / {}", data.orbs_collected, data.orbs_total));
// }

fn update_orb_counter(
    mut commands: Commands,
    state: Res<GameState>,
    q_text: Query<Entity, With<OrbCounterText>>
) {
    let Ok(text_ent) = q_text.single() else { return };
    commands.entity(text_ent).insert(
        Text::new(format!("{} / {}", state.score, state.nb_orbs))
    );
}

fn update_speedometer(
    mut commands: Commands,
    state: Res<GameState>,
    mut q_set: ParamSet<(
        Query<Entity, With<SpeedVsLightText>>,
        Query<Entity, With<SpeedAbsText>>,
        Query<Entity, With<MaxSpeedMultText>>,
        Query<Entity, With<SpeedOfLightText>>,
    )>,
) {
    let Ok(speed_vs_light) = q_set.p0().single() else { return };
    let Ok(speed_abs) = q_set.p1().single() else { return };
    let Ok(max_speed_mult) = q_set.p2().single() else { return };
    let Ok(speed_of_light) = q_set.p3().single() else { return };
    commands.entity(speed_vs_light).insert(Text::new(format!("{:.1} % c", state.speed_as_pct_of_light() * 100.0)));
    commands.entity(speed_abs).insert(Text::new(format!("{:.2} u/s", state.player_speed)));
    commands.entity(max_speed_mult).insert(Text::new(format!("{:.2} x {:.0} u/s", state.speed_multiplier, state.max_player_speed)));
    commands.entity(speed_of_light).insert(Text::new(format!("c = {:.1} u/s", state.speed_of_light)));
}

fn update_timer(
    mut commands: Commands,
    state: Res<GameState>,
    mut q_text: ParamSet<(
        Query<Entity, With<TimerText>>,
        Query<Entity, With<WorldTimerText>>,
    )>,
) {
    let Ok(text_ent) = q_text.p0().single() else { return };
    let Ok(world_text_ent) = q_text.p1().single() else { return };
    commands.entity(text_ent).insert(Text::new(time_str(state.player_time)));
    commands.entity(world_text_ent).insert(Text::new(time_str(state.world_time)));
    commands.entity(world_text_ent).insert(match state.game_win {
        true => Visibility::Visible,
        false => Visibility::Hidden,
    });
}

fn time_str(time: f32) -> String {
    let seconds = time % 60.0;
    let minutes = time as u32 / 60;
    format!("{:02}:{:05.2}", minutes, seconds)
}

fn update_border_flash(
    time: Res<Time>,
    mut flash: ResMut<BorderFlash>,
    mut query: Query<&mut BorderColor, With<BorderFlashNode>>,
) {
    let base = flash.color.to_linear();
    if let Some(timer) = flash.timer.as_mut() {
        timer.tick(time.delta());
        let alpha: f32 = 1.0 - timer.fraction();
        if let Ok(mut border) = query.single_mut() {
            border.set_all(Color::linear_rgba(base.red, base.green, base.blue, alpha));
        }
        if timer.is_finished() {
            flash.timer = None;
        }
    }
}

fn on_ui_data_update(
    t_orb: On<OrbUiUpdateEvent>,
    mut commands: Commands,
    mut flash: ResMut<BorderFlash>,
) {
    match t_orb.event() {
        OrbUiUpdateEvent::Orbs(data) => {
            if data.orbs_collected > 0 {
                flash.color = Color::linear_rgb(1.0, 1.0, 0.0);
                commands.trigger(UiFlashEvent {
                    color: flash.color,
                    duration_secs: 0.5,
                });
            }
        }
    };
}

fn on_ui_flash(
    trigger: On<UiFlashEvent>,
    mut flash: ResMut<BorderFlash>,
) {
    flash.color = trigger.color;
    flash.timer = Some(Timer::from_seconds(trigger.duration_secs, TimerMode::Once));
}

fn sync_perf_ui_visibility(
    graphics: Res<GraphicsSettings>,
    mut perf_ui: Query<&mut Visibility, With<FreeCamPerfUI>>,
) {
    if !graphics.is_changed() {
        return;
    }

    let desired = match graphics.show_perf_hud {
        true => Visibility::Visible,
        false => Visibility::Hidden,
    };

    for mut visibility in &mut perf_ui {
        *visibility = desired;
    }
}
