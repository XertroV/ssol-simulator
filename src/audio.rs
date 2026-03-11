use bevy::{audio::Volume, prelude::*};

use crate::game_state::GameState;

pub mod movement;

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(movement::MovementAudioPlugin)
            .init_resource::<AudioSettings>()
            .add_observer(on_play_orb_pickup_sound)
            .add_observer(on_play_white_arch_pass_sound)
            .add_systems(Startup, setup_audio)
            .add_systems(
                Update,
                (sync_audio_settings,)
            );
    }
}

#[derive(Event)]
pub struct PlayOrbPickupSound {
    /// The number of orbs after the pickup.
    pub orb_count: u32,
    /// Total number of orbs in the map.
    pub nb_orbs: u32,
}

impl PlayOrbPickupSound {
    pub fn is_last_orb(&self) -> bool {
        self.orb_count == self.nb_orbs
    }
}

impl From<&GameState> for PlayOrbPickupSound {
    fn from(state: &GameState) -> Self {
        Self {
            orb_count: state.score,
            nb_orbs: state.nb_orbs,
        }
    }
}

#[derive(Event)]
pub struct PlayWhiteArchPassSound;

/// Parent component for game audio (in case we need to add multiple audio components to an entity).
#[derive(Component)]
pub struct GameAudioComponent;

/// Component for BG music
#[derive(Component)]
pub struct AudioMusic;
/// Component for AudioSFX
#[derive(Component)]
#[require(Visibility)]
pub struct AudioSFX;

/// If we need a place that sound is heard from.
#[derive(Component)]
pub struct PlayerAudioListener;

#[derive(Resource)]
struct GameSounds {
    music: Handle<AudioSource>,
    orb_pickups: Vec<Handle<AudioSource>>,
    final_orb: Handle<AudioSource>,
    white_arch_pass: Handle<AudioSource>,
    move_loop: Handle<AudioSource>,
    max_speed_loop: Handle<AudioSource>,
    ending_music: Handle<AudioSource>,
    accelerate: Handle<AudioSource>,
    decelerate: Handle<AudioSource>,
}

#[derive(Resource)]
pub struct AudioSettings {
    pub master_v: f32,
    pub music_v: f32,
    pub sfx_v: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_v: 0.5,
            music_v: 0.5,
            sfx_v: 0.5,
        }
    }
}

impl AudioSettings {
    /// Clamps the volume settings to be between 0.0 and 1.0.
    pub fn clamp_volumes(&mut self) {
        self.master_v = self.master_v.clamp(0.0, 1.0);
        self.music_v = self.music_v.clamp(0.0, 1.0);
        self.sfx_v = self.sfx_v.clamp(0.0, 1.0);
    }

    fn get_sfx_v(&self) -> Volume {
        Volume::Linear(self.master_v * self.sfx_v)
    }

    fn get_music_v(&self) -> Volume {
        Volume::Linear(self.master_v * self.music_v)
    }
}

fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>, audio_settings: Res<AudioSettings>) {
    // Load all sounds at startup and store them in a resource.
    let music = asset_server.load("audio/Relativity_Music.ogg");
    let orb_pickups = vec![
        asset_server.load("audio/orb01.ogg"),
        asset_server.load("audio/orb02.ogg"),
        asset_server.load("audio/orb03.ogg"),
        asset_server.load("audio/orb04.ogg"),
        asset_server.load("audio/orb05.ogg"),
        asset_server.load("audio/orb06.ogg"),
        asset_server.load("audio/orb07.ogg"),
        asset_server.load("audio/orb08.ogg"),
        asset_server.load("audio/orb09.ogg"),
        asset_server.load("audio/orb10.ogg"),
    ];
    let final_orb = asset_server.load("audio/orb11.ogg");
    // The original game reuses the orb11 finish sound when crossing the white arch.
    let white_arch_pass = asset_server.load("audio/orb11.ogg");
    let move_loop = asset_server.load("audio/Move_Loop.ogg");
    let max_speed_loop = asset_server.load("audio/MaxSpeed_Loop.ogg");
    let ending_music = asset_server.load("audio/Ending_Music.ogg");
    let accelerate = asset_server.load("audio/Accelerate.ogg");
    let decelerate = asset_server.load("audio/Decelerate.ogg");

    let game_sounds = GameSounds { music, orb_pickups, final_orb, white_arch_pass, move_loop, max_speed_loop, ending_music, accelerate, decelerate };

    // Start playing the background music, looped.
    commands.spawn((
        GameAudioComponent,
        Name::new("GameAudio"),
    )).with_child((
        AudioMusic,
        Name::new("BgMusic"),
        AudioPlayer::new(game_sounds.music.clone()),
        PlaybackSettings::LOOP.with_volume(audio_settings.get_music_v()),
    ));

    commands.insert_resource(game_sounds);
}


fn on_play_orb_pickup_sound(
    _t: On<PlayOrbPickupSound>,
    mut commands: Commands,
    sounds: Res<GameSounds>,
    vols: Res<AudioSettings>,
) {
    let sound = match _t.is_last_orb() {
        true => sounds.final_orb.clone(),
        false => sounds.orb_pickups[_t.orb_count as usize % sounds.orb_pickups.len()].clone(),
    };
    commands.spawn((
        AudioSFX,
        AudioPlayer::new(sound),
        PlaybackSettings::DESPAWN.with_volume(vols.get_sfx_v() * Volume::Linear(0.7)),
    ));
}

fn on_play_white_arch_pass_sound(
    _t: On<PlayWhiteArchPassSound>,
    mut commands: Commands,
    sounds: Res<GameSounds>,
    vols: Res<AudioSettings>,
) {
    commands.spawn((
        AudioSFX,
        AudioPlayer::new(sounds.white_arch_pass.clone()),
        PlaybackSettings::DESPAWN.with_volume(vols.get_sfx_v() * Volume::Linear(0.7)),
    ));
}



fn sync_audio_settings(
    mut audio_settings: ResMut<AudioSettings>,
    mut q_bg_musics: Query<&mut AudioSink, (With<AudioMusic>, Without<AudioSFX>)>,
    mut q_sfx: Query<&mut AudioSink, (With<AudioSFX>, Without<AudioMusic>)>,
) {
    if audio_settings.is_changed() {
        audio_settings.clamp_volumes();
        let bg_vol = audio_settings.get_music_v();
        let sfx_vol = audio_settings.get_sfx_v();
        q_bg_musics.iter_mut().for_each(|mut p| p.set_volume(bg_vol));
        q_sfx.iter_mut().for_each(|mut p| p.set_volume(sfx_vol));
    }
}
