use std::{f32::consts::FRAC_PI_2, ops::DerefMut};

use bevy::{
    core_pipeline::smaa::Smaa, input::mouse::AccumulatedMouseMotion, pbr::ShadowFilteringMethod, prelude::*, state::commands, window::{CursorGrabMode, PrimaryWindow}
};
use bevy_rapier3d::prelude::*;

use crate::{
    audio::movement::{MovementAudioState, PlayMovementSound},
    camera_switcher::{is_1st_person_mode, is_free_cam_mode},
    game_state::{self, is_not_hard_paused, reset_game_state, GameState, GameStatePaused, Orb, OrbParent, PlayerPhysState},
    relativity,
    scene_loader::PlayerStart
};

pub use orbs::*;

mod orbs;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // let respawn_sys: SystemId = app.register_system(respawn_player);
        app.init_resource::<MovementSettings>()
            .init_resource::<PlayerAcceleration>()
            // .insert_resource(PlayerCtrl {
            //     respawn_sys,
            // })
            .add_event::<PlayerRespawnRequest>()
            .add_observer(on_player_respawn_request)
            .add_systems(
                Startup,
                spawn_player.after(crate::scene_loader::setup_scene),
            )
            .add_systems(
                Update,
                ((
                    player_update_start,
                    (
                        unpause_player_movement,
                        game_state::speed_boost_decay_system,
                        detect_orb_collisions,
                        calculate_player_acceleration,
                        apply_relativistic_physics,
                        trigger_decelerate_event,
                        apply_collision_drag,
                        update_misc,
                        update_player_look,
                    )
                    .chain()
                        .run_if(is_1st_person_mode)
                        .run_if(is_not_hard_paused),
                    (
                        pause_player_movement,
                    ).run_if(is_free_cam_mode)
                        .run_if(is_movement_not_already_paused),
                    cursor_grab,
                    player_update_done,
                ).chain(),),
            );
    }
}

#[derive(Event)]
pub struct PlayerRespawnRequest;

// #[derive(Resource)]
// pub struct PlayerCtrl {
//     pub respawn_sys: SystemId,
// }

#[derive(Resource)]
pub struct MovementSettings {
    pub free_cam_speed: f32,
    pub mouse_sens: f32,
}

impl Default for MovementSettings {
    fn default() -> Self {
        Self {
            free_cam_speed: 40.0,
            mouse_sens: 0.00012,
        }
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerCamera;

#[derive(Component)]
pub struct PlayerModelEnt;

#[derive(Resource, Default)]
struct PlayerAcceleration(Vec3);

pub fn spawn_player(
    mut commands: Commands,
    // asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    q: Single<(Entity, &Transform), With<PlayerStart>>,
) {
    let (entity, transform) = *q;
    // Spawn a player entity with a mesh and material
    // commands.spawn((PerfUiDefaultEntries::default(),));
    commands
        .spawn((
            Player,
            // the transform has scale = 4. already for the size of the collider
            Collider::cuboid(0.5, 2.5, 0.5), // Collider for the player; original game is square
            transform.clone(),
            GlobalTransform::default(),
            RigidBody::Dynamic,
            LockedAxes::from_bits_retain(
                LockedAxes::ROTATION_LOCKED.bits() | LockedAxes::TRANSLATION_LOCKED.bits(),
            ),
            Friction::coefficient(0.0),
            Sleeping::disabled(),
            Velocity::zero(),
            GravityScale(0.0), // Disable gravity for the player
            Ccd::disabled(),
            Name::new("Player"),
            // KinematicCharacterController {
            //     ..Default::default()
            // },
        ))
        .insert((
            InheritedVisibility::HIDDEN,
            ActiveEvents::COLLISION_EVENTS,
            // ExternalImpulse::default(),
            // LockedAxes::TRANSLATION_LOCKED_Y,
        ))
        .with_children(|p| {
            p.spawn((
                PlayerCamera,
                Camera3d::default(),
                Projection::from(PerspectiveProjection {
                    fov: 60.0f32.to_radians(),
                    near: 0.3,
                    far: 10000.0,
                    ..default()
                }),
                Transform::IDENTITY,
                ShadowFilteringMethod::Gaussian,
                GlobalTransform::default(),
                Smaa::default(),
                Name::new("PlayerCamera"),
                IsDefaultUiCamera,
            ));
        })
        .with_children(|p| {
            let nose_length = 0.4;
            p.spawn((
                PlayerModelEnt,
                Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.7, 0.7, 0.7),
                    ..default()
                })),
                Visibility::Hidden,
                Name::new("PlayerModel"),
            )).with_child((
                Mesh3d(meshes.add(Cone::new(0.125, nose_length))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.8, 0.8, 0.2),
                    ..default()
                })),
                // `- nose_len / 3.` -> Don't move the cone entirely out of the capsule.
                Transform::from_translation(Vec3::new(0.0, 0.4, -0.5 - nose_length / 3.0))
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Name::new("PlayerModelFacePointer"),
            ));
        });
}


pub fn on_player_respawn_request(
    _trigger: Trigger<PlayerRespawnRequest>,
    mut commands: Commands,
    mut q_player: Query<(Entity, &mut Transform, &mut Velocity), (With<Player>, Without<PlayerCamera>, Without<PlayerStart>)>,
    mut q_camera: Query<&mut Transform, (With<PlayerCamera>, Without<Player>, Without<PlayerStart>)>,
    q_start: Query<&Transform, (With<PlayerStart>, Without<Player>, Without<PlayerCamera>)>,
    q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
    q_orbs: Query<(), With<OrbParent>>,
    mut state: ResMut<GameState>,
) {
    if q_player.is_empty() || q_camera.is_empty() || q_start.is_empty() {
        warn!("Player or camera not found for respawn. qp: {}, qc: {}, qs: {}", !q_player.is_empty(), !q_camera.is_empty(), !q_start.is_empty());
        return;
    }

    // Reset the game state
    game_state::reset_game_state(state.deref_mut(), &q_orbs);
    // Reset all orb visibilities
    reset_all_orb_visibilities(q_orb_p_vis);

    let Ok((p_ent, mut p_tform, mut p_vel)) = q_player.single_mut() else { return };
    let Ok(mut camera_tform) = q_camera.single_mut() else { return };
    let Ok(start_transform) = q_start.single() else { return };

    // Reset the player position and velocity
    p_tform.clone_from(&*start_transform);
    p_tform.translation = start_transform.translation;
    p_vel.linvel = Vec3::ZERO;
    // Reset the camera position to match the player
    camera_tform.rotation = Quat::IDENTITY;

    // disable physics for respawn (will be removed in next update)
    commands.entity(p_ent).insert((
        RigidBodyDisabled,
    ));

    info!("Player respawned.\nGameState: {:?}\nPlayer: {:?}", state, (p_tform, p_vel));
}


#[deprecated(since = "0.1.0", note = "Old movement system")]
#[allow(dead_code, unused_variables, unreachable_code)]
fn move_player_simple(
    q_player: Query<(&mut Velocity, &mut Transform), With<Player>>,
    // q_camera: Query<&Transform, (With<FlyCam>, Without<Player>)>, // if not using player camera
    settings: Res<MovementSettings>,
    game_state: Res<GameState>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    panic!("This function is deprecated. Use `calculate_player_acceleration` instead.");

    let Ok((mut velocity, mut transform)) = q_player.single_mut() else {
        return;
    };
    // let Ok(camera_transform) = q_camera.single() else { return; };

    // Calculate forward and right vectors based on the camera's orientation
    // let forward = (camera_transform.forward().as_vec3() * Vec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
    // let right = Vec3::new(-forward.z, 0.0, forward.x);
    let forward = transform.forward().as_vec3();
    let right = transform.right().as_vec3();

    // Reset velocity
    let mut direction = Vec3::ZERO;

    // Move based on input
    if input.pressed(KeyCode::KeyW) {
        direction += forward;
    }
    if input.pressed(KeyCode::KeyS) {
        direction -= forward;
    }
    if input.pressed(KeyCode::KeyA) {
        direction -= right;
    }
    if input.pressed(KeyCode::KeyD) {
        direction += right;
    }

    velocity.linvel = direction * settings.free_cam_speed;
    // Apply the movement to the player entity
    transform.translation += velocity.linvel * time.delta_secs();
}

fn update_player_look(
    mut q_player: Query<&mut Transform, With<Player>>,
    mut q_camera: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
    mouse: Res<AccumulatedMouseMotion>,
    settings: Res<MovementSettings>,
    game_state: Res<GameState>,
    q_window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = q_window.single() else {
        return;
    };
    if window.cursor_options.grab_mode == CursorGrabMode::None {
        return; // Skip if cursor is not grabbed
    }

    let Ok(mut player_transform) = q_player.single_mut() else {
        return;
    };
    let Ok(mut camera_transform) = q_camera.single_mut() else {
        return;
    };

    let (mut yaw, _, _) = player_transform.rotation.to_euler(EulerRot::YXZ);
    let (_, mut pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);

    let window_scale = window.height().min(window.width());
    yaw -= (mouse.delta.x * settings.mouse_sens * window_scale).to_radians();
    pitch -= (mouse.delta.y * settings.mouse_sens * window_scale).to_radians();
    pitch = pitch.clamp(-FRAC_PI_2, FRAC_PI_2);
    // mouse.

    // Apply mouse movement to the player's rotation
    player_transform.rotation = Quat::from_axis_angle(Vec3::Y, yaw);
    camera_transform.rotation = Quat::from_axis_angle(Vec3::X, pitch);
}

fn cursor_grab(
    mut q_window: Query<&mut Window, With<PrimaryWindow>>,
    mut input: ResMut<ButtonInput<KeyCode>>,
) {
    let Ok(window) = q_window.single_mut() else { return };
    // Toggle cursor grab mode on Escape key press
    if input.just_pressed(KeyCode::Escape) {
        let grab_mode = match window.cursor_options.grab_mode {
            CursorGrabMode::None => CursorGrabMode::Locked,
            _ => CursorGrabMode::None,
        };
        set_grab_mode(window, grab_mode);
        // window.cursor_options.grab_mode = grab_mode;
        // window.cursor_options.visible = !window.cursor_options.visible;
        // window.cursor_options.visible = grab_mode != CursorGrabMode::Locked;
        // clear input so we can't react to it again.
        input.clear_just_pressed(KeyCode::Escape);
    }
}

pub fn set_grab_mode(
    mut window: Mut<Window>,
    grab_mode: CursorGrabMode,
) {
    window.cursor_options.grab_mode = grab_mode;
    window.cursor_options.visible = grab_mode != CursorGrabMode::Locked;
}

pub fn reset_all_orb_visibilities(
    q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
) {
    set_all_orb_visibilities(q_orb_p_vis, Visibility::Visible);
}

pub fn set_all_orb_visibilities(
    mut q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
    value: Visibility,
) {
    for mut orb_p_vis in q_orb_p_vis.iter_mut() {
        *orb_p_vis = value;
    }
}


// Movement Scripts

fn calculate_player_acceleration(
    mut commands: Commands,
    mut accel: ResMut<PlayerAcceleration>,
    q_player: Query<(&Transform, &Velocity), With<Player>>,
    _settings: Res<MovementSettings>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let Ok((transform, vel)) = q_player.single() else {
        return;
    };

    let mut desired_accel = Vec3::ZERO;
    let accel_rate = 20.0; // From MovementScripts.cs

    if input.pressed(KeyCode::KeyW) {
        desired_accel -= transform.forward().as_vec3();
    }
    if input.pressed(KeyCode::KeyS) {
        desired_accel += transform.forward().as_vec3();
    }
    if input.pressed(KeyCode::KeyA) {
        desired_accel += transform.right().as_vec3();
    }
    if input.pressed(KeyCode::KeyD) {
        desired_accel -= transform.right().as_vec3();
    }

    accel.0 = desired_accel.normalize_or_zero() * accel_rate * time.delta_secs();

    // check if we should emit accelerate
    if desired_accel.length_squared() > 0.0 && vel.linvel.length_squared() <= 1.0 {
        commands.trigger(PlayMovementSound::Accelerate);
    }
}

fn trigger_decelerate_event(
    mut commands: Commands,
    accel: Res<PlayerAcceleration>,
    state: Res<GameState>,
    m_state: Res<MovementAudioState>,
) {
    if m_state.is_decelerating_triggered && accel.0.length_squared() < 0.01 && state.player_speed > 0.1 {
        commands.trigger(PlayMovementSound::Decelerate);
    }
}

fn apply_relativistic_physics(
    mut q_player: Query<&mut Velocity, With<Player>>,
    mut state: ResMut<GameState>,
    accel: Res<PlayerAcceleration>,
    time: Res<Time>,
) {
    let Ok(mut velocity) = q_player.single_mut() else {
        return;
    };

    let drag = 2.0;
    if accel.0.length_squared() == 0.0 {
        let vel = state.player_velocity_vector;
        state.player_velocity_vector -= vel * drag * time.delta_secs();
    }
    state.player_velocity_vector = relativity::add_relativistic_velocity(
        state.player_velocity_vector,
        accel.0,
        state.inv_lorentz_factor,
        state.speed_of_light * state.speed_of_light,
    );

    let max_speed = state.max_player_speed * state.speed_multiplier;
    state.player_speed = state.player_velocity_vector.length();
    if state.player_speed > max_speed {
        state.player_velocity_vector = state.player_velocity_vector.normalize_or_zero() * max_speed;
        state.player_speed = max_speed;
    }

    let v_sq = state.player_speed * state.player_speed;
    let c_sq = state.speed_of_light * state.speed_of_light;
    state.inv_lorentz_factor = (1.0 - v_sq / c_sq).sqrt();

    if state.inv_lorentz_factor.is_nan() {
        velocity.linvel = Vec3::ZERO;
    } else {
        velocity.linvel = -1.0 * (state.player_velocity_vector / state.inv_lorentz_factor);
    }
}

fn apply_collision_drag(
    mut state: ResMut<GameState>,
    mut q_player: Query<(Entity, &mut Transform, &mut Velocity), With<Player>>,
    q_others: Query<Option<&Velocity>, Without<Player>>,
    rapier_ctx: ReadRapierContext,
    time: Res<Time>,
) {
    let Ok((player_entity, mut transform, velocity)) = q_player.single_mut() else {
        return;
    };
    let Ok(rapier_ctx) = rapier_ctx.single() else {
        return;
    };
    // info!("Checking collisions for player: {:?}", player.0);

    for contact_pair in rapier_ctx.contact_pairs_with(player_entity) {
        if !contact_pair.has_any_active_contact() {
            continue;
        }
        for contact in contact_pair.manifolds() {
            let normal = contact.normal();
            let speed2 = contact.rigid_body1().and_then(|e| q_others.get(e).ok()).unwrap_or_default().map_or(0.0, |v| {
                info!("Got Ent2");
                v.linvel.length()
            });
            // let r = &contact_pair.raw;
            // info!("Collision detected with player: {:?} {:?}", r.collider1, r.collider2);

            // Apply drag to the player velocity
            state.player_velocity_vector *= 1.0 - (0.98 * time.delta_secs());

            let speed = velocity.linvel.length() + speed2;
            transform.translation += normal.with_y(0.).normalize_or_zero() * speed * 1.25 * time.delta_secs();

            // Log the collision for debugging
            // info!("Collision detected with player: {:?}", contact_pair);
        }
    }
}

fn update_misc(
    mut q_player: Query<(&mut Transform, &mut Velocity), With<Player>>,
    state: Res<GameState>,
    time: Res<Time>,
) {
    let Ok((mut transform, mut velocity)) = q_player.single_mut() else {
        return;
    };
    velocity.angvel = Vec3::ZERO; // Reset angular velocity

    // Update the player's position based on the velocity
    // transform.translation += velocity.linvel * time.delta_secs();
    // transform.translation -= state.player_velocity_vector * time.delta_secs();
    // Reset the velocity to zero after applying it
    // velocity.linvel = Vec3::ZERO;
}


fn is_movement_not_already_paused(
    state: Res<GameState>,
) -> bool {
    !state.has_cam_paused_player_movement()
}


fn pause_player_movement(
    mut commands: Commands,
    mut q_player: Query<(&mut Velocity, &mut Transform), With<Player>>,
    mut state: ResMut<GameState>,
    // q_orbs: Query<(), With<OrbParent>>,
) {
    if state.as_ref().has_cam_paused_player_movement() {
        return;
    }

    let Ok((mut velocity, transform)) = q_player.single_mut() else { return };

    // Save the current player velocity

    let saved_state = state.as_ref().clone();
    let saved = Some((saved_state, PlayerPhysState::from((&*velocity, &*transform))).into());
    // reset_game_state(&mut state, &q_orbs);
    state.movement_frozen = saved;
    state.is_hard_paused = false;

    // Stop the player movement
    velocity.linvel = Vec3::ZERO;
    commands.trigger(GameStatePaused::CameraPaused);
    info!("Player movement paused, state saved.");
}

fn unpause_player_movement(
    mut commands: Commands,
    mut q_player: Query<(&mut Velocity, &mut Transform), (With<Player>, Without<RigidBodyDisabled>)>,
    q_player_pre: Query<Entity, (With<Player>, With<RigidBodyDisabled>)>,
    // q_start: Query<&Transform, (With<PlayerStart>, Without<Player>)>,
    mut state: ResMut<GameState>,
) {
    let p_with_rbd = q_player_pre.single();

    let should_unpause_movement = state.as_ref().should_unpause_movement();
    let should_remove_rbd = p_with_rbd.is_ok();

    if !should_unpause_movement && !should_remove_rbd {
        return; // Don't resume
    }

    if let Ok(p_ent) = p_with_rbd {
        commands.entity(p_ent).remove::<RigidBodyDisabled>();
        info!("Player movement resumed, RigidBodyDisabled removed.");
        return;
        // player.0.linvel = Vec3::ZERO; // Reset velocity
        // *player.1 = start_transform.clone();
    }

    let Ok(mut player) = q_player.single_mut() else { return };
    // let Ok(start_transform) = q_start.single() else { return };

    // Restore the saved player velocity and position
    if let Some(saved_state) = state.movement_frozen.take() {
        player.0.linvel = saved_state.1.velocity;
        player.1.translation = saved_state.1.position;
        state.clone_from(&saved_state.0);
        info!("Player movement resumed, state restored");
        commands.trigger(match state.is_hard_paused {
            true => GameStatePaused::PlayerPaused,
            false => GameStatePaused::Unpaused,
        });
    }
}

pub(crate) fn player_update_done() {}
pub(crate) fn player_update_start() {}
