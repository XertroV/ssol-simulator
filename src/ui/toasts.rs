use bevy::prelude::*;

use crate::ui::theme::{self, text_font};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ToastKind {
    Info,
    Warning,
}

#[derive(Event, Debug, Clone)]
pub struct ToastEvent {
    pub message: String,
    pub kind: ToastKind,
    pub duration_secs: f32,
}

impl ToastEvent {
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Warning,
            duration_secs: 3.0,
        }
    }

    #[allow(dead_code)]
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Info,
            duration_secs: 2.5,
        }
    }
}

pub struct ToastUiPlugin;

impl Plugin for ToastUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_toast_ui)
            .add_systems(Update, update_toasts)
            .add_observer(on_toast);
    }
}

#[derive(Component)]
pub(crate) struct ToastRoot;

#[derive(Component)]
pub(crate) struct ToastEntry {
    timer: Timer,
    kind: ToastKind,
}

#[derive(Component)]
struct ToastMessage;

fn setup_toast_ui(mut commands: Commands) {
    commands.spawn((
        ToastRoot,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            right: Val::Px(14.0),
            width: Val::Px(360.0),
            max_width: Val::Percent(36.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Stretch,
            row_gap: Val::Px(6.0),
            ..default()
        },
        GlobalZIndex(1000),
    ));
}

fn on_toast(
    trigger: On<ToastEvent>,
    mut commands: Commands,
    q_root: Query<Entity, With<ToastRoot>>,
    asset_server: Res<AssetServer>,
) {
    let Ok(root) = q_root.single() else {
        return;
    };

    let (bg, border, text, kind_label) = match trigger.kind {
        ToastKind::Info => (
            theme::INFO_BG,
            theme::ACCENT,
            theme::TEXT,
            "INFO",
        ),
        ToastKind::Warning => (
            theme::WARN_BG,
            theme::WARN,
            Color::srgba(1.0, 0.95, 0.88, 1.0),
            "WARN",
        ),
    };
    let font = asset_server.load("fonts/neuton/Neuton-Regular.ttf");

    commands.entity(root).with_children(|root| {
        root.spawn((
            ToastEntry {
                timer: Timer::from_seconds(trigger.duration_secs, TimerMode::Once),
                kind: trigger.kind,
            },
            Node {
                width: Val::Percent(100.0),
                // Thicker left edge as kind accent stripe
                border: UiRect {
                    left: Val::Px(3.0),
                    right: Val::Px(1.0),
                    top: Val::Px(1.0),
                    bottom: Val::Px(1.0),
                },
                border_radius: BorderRadius::all(Val::Px(theme::RADIUS_SM)),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
        ))
        .with_children(|toast| {
            toast.spawn((
                Text::new(kind_label),
                text_font(font.clone(), 11.0),
                TextColor(border),
            ));
            toast.spawn((
                ToastMessage,
                Text::new(trigger.message.clone()),
                text_font(font, 18.0),
                TextColor(text),
            ));
        });
    });
}

fn update_toasts(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut q_toasts: Query<
        (
            Entity,
            &mut ToastEntry,
            &mut BackgroundColor,
            &mut BorderColor,
            &Children,
        ),
        With<ToastEntry>,
    >,
    mut q_text: Query<&mut TextColor, With<ToastMessage>>,
) {
    for (entity, mut toast, mut background, mut border, children) in &mut q_toasts {
        toast.timer.tick(time.delta());

        let alpha = if toast.timer.fraction() < 0.7 {
            1.0
        } else {
            1.0 - ((toast.timer.fraction() - 0.7) / 0.3)
        }
        .clamp(0.0, 1.0);

        let (bg, border_color, text_color) = match toast.kind {
            ToastKind::Info => (
                Color::srgba(0.05, 0.08, 0.14, 0.94 * alpha),
                Color::srgba(0.45, 0.78, 1.0, 0.98 * alpha),
                Color::srgba(0.96, 0.97, 1.0, alpha),
            ),
            ToastKind::Warning => (
                Color::srgba(0.16, 0.07, 0.02, 0.94 * alpha),
                Color::srgba(1.0, 0.58, 0.22, 0.98 * alpha),
                Color::srgba(1.0, 0.95, 0.88, alpha),
            ),
        };
        *background = BackgroundColor(bg);
        *border = BorderColor::all(border_color);

        for child in children.iter() {
            if let Ok(mut text) = q_text.get_mut(child) {
                *text = TextColor(text_color);
            }
        }

        if toast.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
