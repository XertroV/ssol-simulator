use bevy::prelude::*;
use iyes_perf_ui::prelude::PerfUiDefaultEntries;
use iyes_perf_ui::entries::PerfUiFixedTimeEntries;

use crate::ai_support::ActionCounter;
use crate::camera_switcher::FreeCamPerfUI;
use crate::config::GraphicsSettings;
use crate::game_state::GameState;
use crate::ui::finish_screen::{FinishFlowState, FinishPhase};
use crate::ui::theme::{self, text_font};

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
                    sync_hud_chip_visibility,
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
            font_size: FontSize::Px(12.0),
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
pub(crate) struct BorderFlash {
    pub(crate) timer: Option<Timer>,
    pub(crate) color: Color,
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
/// Entire local-time instrument chip (hidden after win so finish HUD owns timing).
#[derive(Component)]
struct TimerChipRoot;
#[derive(Component)]
struct OrbChipRoot;
#[derive(Component)]
struct VelocityChipRoot;

#[derive(Component)]
struct BorderFlashNode;

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");
    let label = text_font(font.clone(), 12.0);
    let value = text_font(font.clone(), 22.0);
    let hero = text_font(font.clone(), 34.0);
    let mono = text_font(font.clone(), 16.0);

    commands.spawn((UiData,));

    // Full-screen pass-through root: chrome sits in absolute instrument chips
    // so the center FOV stays clear (no empty flex rows burning vertical space).
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        })
        .with_children(|root| {
            // Top-left: proper time chip
            root.spawn((
                TimerChipRoot,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(14.0),
                    left: Val::Px(14.0),
                    min_width: Val::Px(120.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(theme::RADIUS_SM)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("LOCAL TIME"),
                    label.clone(),
                    TextColor(theme::TEXT_MUTED),
                ));
                panel.spawn((
                    TimerText,
                    Text::new("00:00.00"),
                    value.clone(),
                    TextColor(theme::TEXT),
                ));
                panel.spawn((
                    WorldTimerText,
                    Visibility::Hidden,
                    Text::new("00:00.00"),
                    text_font(font.clone(), 14.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });

            // Bottom-left: orb tally (primary run goal)
            root.spawn((
                OrbChipRoot,
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(14.0),
                    left: Val::Px(14.0),
                    min_width: Val::Px(148.0),
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(theme::RADIUS_SM)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::BORDER_STRONG),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("ORBS"),
                    label.clone(),
                    TextColor(theme::ACCENT),
                ));
                panel.spawn((
                    OrbCounterText,
                    Text::new("0 / 0"),
                    hero,
                    TextColor(theme::TEXT),
                ));
            });

            // Bottom-right: relativistic speed stack
            root.spawn((
                VelocityChipRoot,
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(14.0),
                    right: Val::Px(14.0),
                    min_width: Val::Px(168.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(theme::RADIUS_SM)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(1.0),
                    align_items: AlignItems::FlexEnd,
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("VELOCITY"),
                    label.clone(),
                    TextColor(theme::TEXT_MUTED),
                ));
                panel.spawn((
                    SpeedAbsText,
                    Text::new("0.00 u/s"),
                    value.clone(),
                    TextColor(theme::TEXT),
                ));
                panel.spawn((
                    SpeedVsLightText,
                    Text::new("0.0 % c"),
                    mono.clone(),
                    TextColor(theme::ACCENT),
                ));
                panel.spawn((
                    MaxSpeedMultText,
                    Text::new("0.00 × 0 u/s"),
                    text_font(font.clone(), 13.0),
                    TextColor(theme::TEXT_DIM),
                ));
                panel.spawn((
                    SpeedOfLightText,
                    Text::new("c = 0.0 u/s"),
                    text_font(font.clone(), 13.0),
                    TextColor(theme::TEXT_MUTED),
                ));
            });
        });

    // Border flash overlay (warning / orb pickup)
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            border: UiRect::all(Val::Px(4.0)),
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
    commands.entity(world_text_ent).insert(Visibility::Hidden);
}

/// Keep instrument chips out of the way of finish flow overlays.
fn sync_hud_chip_visibility(
    mut commands: Commands,
    state: Res<GameState>,
    finish: Option<Res<FinishFlowState>>,
    q_timer: Query<Entity, With<TimerChipRoot>>,
    q_orb: Query<Entity, With<OrbChipRoot>>,
    q_vel: Query<Entity, With<VelocityChipRoot>>,
) {
    let phase = finish.map(|f| f.phase).unwrap_or(FinishPhase::NotWon);
    let end_overlay = phase == FinishPhase::EndOverlayOpen;
    let won = state.game_win || phase != FinishPhase::NotWon;

    let timer_vis = if won {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    // Orbs + velocity stay during win roam; hide under full results overlay.
    let run_hud_vis = if end_overlay {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };

    if let Ok(e) = q_timer.single() {
        commands.entity(e).insert(timer_vis);
    }
    if let Ok(e) = q_orb.single() {
        commands.entity(e).insert(run_hud_vis);
    }
    if let Ok(e) = q_vel.single() {
        commands.entity(e).insert(run_hud_vis);
    }
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
