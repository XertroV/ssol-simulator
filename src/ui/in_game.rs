use bevy::prelude::*;
use bevy::state::commands;
use bevy::time::Stopwatch;
use iyes_perf_ui::prelude::PerfUiDefaultEntries;

use crate::camera_switcher::FreeCamPerfUI;
use crate::game_state::GameState;

pub struct InGameUiPlugin;

impl Plugin for InGameUiPlugin {
    fn build(&self, app: &mut App) {
        app
            // .init_resource::<GameStats>()
            .init_resource::<BorderFlash>()
            .add_event::<OrbUiUpdateEvent>()
            .add_systems(Startup, (setup_ui, setup_fps_stats_ui))
            .add_systems(
                Update,
                (
                    update_orb_counter,
                    update_speedometer,
                    update_timer,
                    update_border_flash,
                ),
            ).add_observer(on_ui_data_update);
    }
}

fn setup_fps_stats_ui(mut commands: Commands) {
    commands.spawn((FreeCamPerfUI, PerfUiDefaultEntries::default()));
}


#[derive(Component, Default)]
pub struct UiData;

#[derive(Component, Default, Clone, Copy)]
pub struct OrbUiData {
    pub orbs_collected: u32,
    pub orbs_total: u32,
}
#[derive(Component, Default, Clone, Copy)]
pub struct SpeedUiData {
    pub speed_fraction_c: f32,
    pub speed_abs: f32,
}
#[derive(Component, Default, Clone, Copy)]
pub struct TimeUiData {
    pub player_time: f32,
    pub world_time: f32,
}

#[derive(Event)]
pub enum OrbUiUpdateEvent {
    Orbs(OrbUiData),
    Speed(SpeedUiData),
    Time(TimeUiData),
}

#[derive(Resource)]
struct BorderFlash {
    timer: Option<Timer>,
}

impl Default for BorderFlash {
    fn default() -> Self {
        Self { timer: None }
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
                height: Val::Px(40.0),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                padding,
                ..default()
            })
            .with_children(|top| {
                // Timer (top left)
                top.spawn((
                    Text::new("00:00"),
                    font_c.clone(),
                    TimerText,
                ));
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
                    font_c.clone(),
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
        BorderColor(Color::NONE),
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
    q_text: Query<Entity, With<TimerText>>,
) {
    let Ok(text_ent) = q_text.single() else { return };
    let elapsed = state.player_time;
    let seconds = elapsed % 60.0;
    let minutes = elapsed as u32 / 60;
    commands.entity(text_ent).insert(Text::new(format!("{:02}:{:05.2}", minutes, seconds)));
}

fn update_border_flash(
    time: Res<Time>,
    mut flash: ResMut<BorderFlash>,
    mut query: Query<&mut BorderColor, With<BorderFlashNode>>,
) {
    if let Some(timer) = flash.timer.as_mut() {
        timer.tick(time.delta());
        let alpha: f32 = 1.0 - timer.fraction();
        if let Ok(mut border) = query.single_mut() {
            border.0 = Color::linear_rgba(1.0, 1.0, 0.0, alpha);
        }
        if timer.finished() {
            flash.timer = None;
        }
    }
}

fn on_ui_data_update(
    t_orb: Trigger<OrbUiUpdateEvent>,
    mut commands: Commands,
    mut flash: ResMut<BorderFlash>,
    q_data: Query<(Entity, Option<&OrbUiData>), With<UiData>>,
) {
    let Ok((data_ent, orbs)) = q_data.single() else { return };
    let mut ent_cmd = commands.entity(data_ent);
    match t_orb.event() {
        OrbUiUpdateEvent::Orbs(data) => {
            // If the orb count increased, flash the border.
            if data.orbs_collected > orbs.map(|o| o.orbs_collected).unwrap_or(0) {
                flash.timer = Some(Timer::from_seconds(0.5, TimerMode::Once));
            }
            ent_cmd.insert(*data)
        }
        OrbUiUpdateEvent::Speed(data) => ent_cmd.insert(*data),
        OrbUiUpdateEvent::Time(data) => ent_cmd.insert(*data),
    };
}
