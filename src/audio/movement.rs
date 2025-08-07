use bevy::{audio::Volume, prelude::*};

use crate::{audio::{AudioSFX, AudioSettings}, game_state::GameState};

use super::GameSounds;

pub struct MovementAudioPlugin;

impl Plugin for MovementAudioPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<MovementAudioState>()
            .add_event::<PlayMovementSound>()
            .add_observer(on_play_movement_sound)
            .add_observer(on_game_state_paused)
            .add_systems(Startup, setup_movement_audio.after(super::setup_audio))
            .add_systems(Update, (
                update_continuous_movement_audio,
            //     sync_movement_audio,
            //     play_movement_audio,
            ))
            ;
    }
}


#[derive(Event, Debug, Clone, Copy)]
pub enum PlayMovementSound {
    /// Play the sound for accelerating.
    Accelerate,
    /// Play the sound for decelerating.
    Decelerate,
}


#[derive(Resource)]
pub struct MovementAudioState {
    pub velocity_counter: f32, // Seconds to ignore acceleration/deceleration
    pub fade_in_hum_progress: f32, // From AudioScripts.fadeIn
    pub is_decelerating_triggered: bool, // AudioScripts.slowDown -- true after acceleration, false after deceleration
    pub high_speed_fade_in_progress: f32, // From AudioScripts.maxFadeIn
    pub high_speed_fade_out_progress: f32, // From AudioScripts.maxFadeOut
    pub is_frozen_by_game_state: bool, // To track if audio was paused by game state
}

impl Default for MovementAudioState {
    fn default() -> Self {
        Self {
            velocity_counter: 0.0,
            fade_in_hum_progress: 0.0,
            is_decelerating_triggered: false,
            high_speed_fade_in_progress: 0.0,
            high_speed_fade_out_progress: 0.0,
            is_frozen_by_game_state: false,
        }
    }
}


#[derive(Component)]
pub struct LowSpeedHumSound;

#[derive(Component)]
pub struct HighSpeedHumSound;

#[derive(Component)]
pub struct OneShotAccelerateSound;

#[derive(Component)]
pub struct OneShotDecelerateSound;





fn on_play_movement_sound(
    trig: Trigger<PlayMovementSound>,
    mut commands: Commands,
    sounds: Res<GameSounds>,
    vols: Res<AudioSettings>,
    mut state: ResMut<MovementAudioState>,
    // q_sink: Query<&mut AudioSink, With<LowSpeedHumSound>>,
    q_accel: Query<(), With<OneShotAccelerateSound>>,
    q_decel: Query<(), With<OneShotDecelerateSound>>,
) {
    let (vol_coef, sound) = match *trig {
        PlayMovementSound::Accelerate => match q_accel.single() {
            Ok(_) => return,
            Err(_) => {
                state.velocity_counter = 1.5;
                state.is_decelerating_triggered = true;
                (0.6, sounds.accelerate.clone())
            }
        },
        PlayMovementSound::Decelerate => match q_decel.single() {
            Ok(_) => return,
            Err(_) => {
                if !state.is_decelerating_triggered { return; }
                state.is_decelerating_triggered = false;
                (0.3, sounds.decelerate.clone())
            }
        },
    };
    let components = (
        AudioSFX,
        AudioPlayer::new(sound),
        PlaybackSettings::DESPAWN.with_volume(vols.get_sfx_v() * Volume::Linear(vol_coef)),
    );
    match *trig {
        PlayMovementSound::Accelerate => {
            commands.spawn((OneShotAccelerateSound, components));
        }
        PlayMovementSound::Decelerate => {
            commands.spawn((OneShotDecelerateSound, components));
        }
    }
}



fn setup_movement_audio(
    mut commands: Commands,
    sounds: Res<GameSounds>,
) {
    // Volumes should be 0 initially so that they fade in.
    commands.spawn((
        LowSpeedHumSound,
        AudioSFX,
        AudioPlayer::new(sounds.move_loop.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
    ));
    commands.spawn((
        HighSpeedHumSound,
        AudioSFX,
        AudioPlayer::new(sounds.max_speed_loop.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
    ));
}


/// Manage low/high speed hums.
fn update_continuous_movement_audio(
    mut m_state: ResMut<MovementAudioState>,
    g_state: Res<GameState>,
    vols: Res<AudioSettings>,
    mut q_low_hum: Query<&mut AudioSink, (With<LowSpeedHumSound>, Without<HighSpeedHumSound>)>,
    mut q_high_hum: Query<&mut AudioSink, (With<HighSpeedHumSound>, Without<LowSpeedHumSound>)>,
    mut q_accel: Query<&mut AudioSink, (With<OneShotAccelerateSound>, Without<OneShotDecelerateSound>, Without<HighSpeedHumSound>, Without<LowSpeedHumSound>)>,
    time: Res<Time>,
) {
    let Ok(mut low_hum) = q_low_hum.single_mut() else { return };
    let Ok(mut high_hum) = q_high_hum.single_mut() else { return };

    // if !unfrozen: return

    if m_state.velocity_counter > 0.0 {
        m_state.velocity_counter -= time.delta_secs();
        if m_state.velocity_counter <= 0.0 && low_hum.is_paused() {
            low_hum.play();
            m_state.fade_in_hum_progress = 0.1;
        }
    }
    if m_state.fade_in_hum_progress < 0.99 {
        m_state.fade_in_hum_progress += time.delta_secs();

        let hum_prog_vol = Volume::Linear(m_state.fade_in_hum_progress);
        let accel_fade_out = Volume::Linear((1.0 - m_state.fade_in_hum_progress).max(0.0));

        low_hum.set_volume(vols.get_sfx_v() * hum_prog_vol);
        if let Ok(mut accel_sink) = q_accel.single_mut() {
            accel_sink.set_volume(vols.get_sfx_v() * accel_fade_out);
        }
        return;
    }

    let speed_pct = g_state.player_velocity_vector.length() / g_state.max_player_speed;
    low_hum.set_volume(vols.get_sfx_v() * Volume::Linear(speed_pct * 0.5));

    if speed_pct >= 0.9 {
        // if high hum is not playing, start it
        if high_hum.is_paused() {
            high_hum.play();
            m_state.high_speed_fade_in_progress = 1.0;
            m_state.high_speed_fade_out_progress = 1.0;
        } else if m_state.high_speed_fade_in_progress > 0.0 {
            high_hum.set_volume(vols.get_sfx_v() * Volume::Linear(1.0 - m_state.high_speed_fade_in_progress));
            m_state.high_speed_fade_in_progress -= 0.3 * time.delta_secs();
        }
    } else if !high_hum.is_paused() {
        if m_state.high_speed_fade_out_progress < 0.0 {
            high_hum.pause();
        } else {
            high_hum.set_volume(vols.get_sfx_v() * Volume::Linear(m_state.high_speed_fade_out_progress));
            m_state.high_speed_fade_out_progress -= 0.3 * time.delta_secs();
        }
        m_state.high_speed_fade_in_progress = 0.0;
    }
}



fn on_game_state_paused(
    _t: Trigger<crate::game_state::GameStatePaused>,
    mut m_state: ResMut<MovementAudioState>,
    q_low_hum: Query<&AudioSink, With<LowSpeedHumSound>>,
    q_high_hum: Query<&AudioSink, With<HighSpeedHumSound>>,
    q_accel: Query<&AudioSink, With<OneShotAccelerateSound>>,
    q_decel: Query<&AudioSink, With<OneShotDecelerateSound>>,
) {
    // are we pausing or unpausing?
    let set_paused = _t.is_paused();
    m_state.is_frozen_by_game_state = set_paused;
    set_sink_paused(q_low_hum.single().ok(), set_paused);
    set_sink_paused(q_high_hum.single().ok(), set_paused);
    set_sink_paused(q_accel.single().ok(), set_paused);
    set_sink_paused(q_decel.single().ok(), set_paused);
}

fn set_sink_paused(sink: Option<&AudioSink>, set_paused: bool) {
    let Some(sink) = sink else { return };
    match set_paused {
        true => sink.pause(),
        false => sink.play()
    };
}


//     let Ok(low_hum) = q_low_hum.single() else { return; };
//     let Ok(high_hum) = q_high_hum.single() else { return; };
//     match set_paused {
//         true => {
//             low_hum.pause();
//             high_hum.pause();
//         }
//         false => {
//             low_hum.play();
//             high_hum.play();
//         }
//     }

//     if let Ok(accel_sink) = q_accel.single() {
//         match set_paused {
//             true => accel_sink.pause(),
//             false => accel_sink.play(),
//         }
//     }
//     if let Ok(decel_sink) = q_decel.single() {
//         match set_paused {
//             true => decel_sink.pause(),
//             false => decel_sink.play(),
//         }
//     }
// }
