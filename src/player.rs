use std::{f32::consts::FRAC_PI_2, ops::DerefMut};

use bevy::{
    anti_alias::smaa::Smaa, camera::visibility::{InheritedVisibility, ViewVisibility}, ecs::entity_disabling::Disabled, input::mouse::AccumulatedMouseMotion, light::ShadowFilteringMethod, prelude::*, window::{CursorGrabMode, CursorOptions, PrimaryWindow}
};
use bevy_rapier3d::{parry::either::Either::Right, prelude::*};

use crate::{
    ai::{not_waiting_for_ai, AiActionInput, AiConfig, curriculum::CurriculumConfig},
    audio::movement::{MovementAudioState, PlayMovementSound}, camera_switcher::{is_1st_person_mode, is_free_cam_mode}, game_state::{self, hide_white_arch, is_not_hard_paused, GameState, GameStatePaused, OrbParent, PlayerPhysState}, key_mapping::KeyMapping, orb_curriculum::{apply_curriculum_to_spawned_orbs, collect_orb_data, OrbId}, physics_interpolation::InterpolationBundle, relativity, scene_loader::{PlayerStart, WhiteFinishArch}, ui::in_game::OrbUiUpdateEvent
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
            .add_observer(on_player_respawn_request)
            .add_systems(
                Startup,
                (set_init_ui, spawn_player)
                            .after(crate::scene_loader::setup_scene)
                            .after(game_state::set_orb_count),
            )
            // Physics systems run in FixedUpdate for deterministic 100Hz simulation
            // Must run AFTER Rapier's physics step to read contact pairs correctly
            .add_systems(
                FixedUpdate,
                ((
                    player_update_start,
                    // Look update runs early in the chain so AI actions are applied before movement
                    update_player_look
                        .run_if(is_1st_person_mode)
                        .run_if(is_not_hard_paused),
                    (
                        unpause_player_movement,
                        game_state::speed_boost_decay_system,
                        detect_orb_collisions,
                        update_speed_of_light,
                        calculate_player_acceleration,
                        apply_relativistic_physics,
                        trigger_decelerate_event,
                        apply_collision_drag,
                        update_misc,
                    )
                    .chain()
                        .run_if(is_1st_person_mode)
                        .run_if(is_not_hard_paused),
                    (
                        pause_player_movement,
                    ).run_if(is_free_cam_mode)
                        .run_if(is_movement_not_already_paused),
                    player_update_done,
                ).chain()
                    .after(PhysicsSet::StepSimulation)
                    .run_if(not_waiting_for_ai),
            ),
            )
            // Human input systems stay in Update for responsiveness
            .add_systems(
                Update,
                (
                    cursor_grab,
                    process_debug_inputs,
                ),
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
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    q: Single<(Entity, &Transform), With<PlayerStart>>,
) {
    let (entity, transform) = *q;
    // let transform = transform.clone();
    let model_path = "models/MovingPerson.gltf";
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
            // Lock rotation only - let Rapier handle translation via velocity.linvel
            // This allows Rapier's built-in collision response to work correctly at all speeds
            // Velocity set/updated in apply_collision_drag and other places.
            LockedAxes::ROTATION_LOCKED,
            Friction::coefficient(0.0),
            Sleeping::disabled(),
            Velocity::zero(),
            GravityScale(0.0), // Disable gravity for the player
            Ccd::disabled(),
            Name::new("Player"),
            // Add interpolation for smooth rendering between physics ticks
            // Use translation_only since rotation is controlled by mouse look, not physics
            InterpolationBundle::translation_only(&transform),
            // KinematicCharacterController {
            //     ..Default::default()
            // },
        ))
        .insert((
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
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
                    far: 100_000.0,
                    ..default()
                }),
                Transform::IDENTITY,
                ShadowFilteringMethod::Gaussian,
                GlobalTransform::default(),
                Smaa::default(),
                Name::new("PlayerCamera"),
                IsDefaultUiCamera,
                // Add visibility components to maintain proper hierarchy
                Visibility::Inherited,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
        })
        .with_children(|p| {
            // let nose_length = 0.4;
            p.spawn((
                PlayerModelEnt,
                Visibility::Hidden,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                Name::new("PlayerModel"),
            ))
            .insert((
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(model_path))),
                Transform::from_scale(1.0 / transform.scale * 0.775)
                    .with_translation(transform.translation * 0.7 + Vec3::Y * 0.11),
            ))
            // .insert((
            //     Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
            //     MeshMaterial3d(materials.add(StandardMaterial {
            //         base_color: Color::srgb(0.7, 0.7, 0.7),
            //         ..default()
            //     })),
            // ))
            // .with_child((
            //     Mesh3d(meshes.add(Cone::new(0.125, nose_length))),
            //     MeshMaterial3d(materials.add(StandardMaterial {
            //         base_color: Color::srgb(0.8, 0.8, 0.2),
            //         ..default()
            //     })),
            //     // `- nose_len / 3.` -> Don't move the cone entirely out of the capsule.
            //     Transform::from_translation(Vec3::new(0.0, 0.4, -0.5 - nose_length / 3.0))
            //         .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            //     Name::new("PlayerModelFacePointer"),
            // ))
            ;
        });
}

fn set_init_ui(
    mut commands: Commands,
    mut state: ResMut<GameState>,
) {
    game_state::return_growth(state.deref_mut());
    commands.trigger(OrbUiUpdateEvent::Orbs(state.as_ref().into()));
}

pub fn on_player_respawn_request(
    _trigger: On<PlayerRespawnRequest>,
    mut commands: Commands,
    mut q_player: Query<(Entity, &mut Transform, &mut Velocity), (With<Player>, Without<PlayerCamera>, Without<PlayerStart>)>,
    mut q_camera: Query<&mut Transform, (With<PlayerCamera>, Without<Player>, Without<PlayerStart>)>,
    q_start: Query<&Transform, (With<PlayerStart>, Without<Player>, Without<PlayerCamera>)>,
    q_orb_p_vis: Query<&mut Visibility, With<OrbParent>>,
    // Query ALL orbs including disabled ones - need to re-enable/disable based on curriculum
    q_orbs_all: Query<(Entity, &OrbId, &GlobalTransform, Has<Disabled>), With<OrbParent>>,
    mut state: ResMut<GameState>,
    q_white_arch: Query<Entity, With<WhiteFinishArch>>,
    mut curriculum: ResMut<CurriculumConfig>,
) {
    if q_player.is_empty() || q_camera.is_empty() || q_start.is_empty() {
        warn!("Player or camera not found for respawn. qp: {}, qc: {}, qs: {}", !q_player.is_empty(), !q_camera.is_empty(), !q_start.is_empty());
        return;
    }

    // Reset all orb visibilities FIRST - before applying curriculum
    // This ensures all orbs start visible, then curriculum can hide the ones outside limits
    reset_all_orb_visibilities(q_orb_p_vis);

    // Apply curriculum by enabling/disabling orbs based on max_orbs and radius
    let orb_data = collect_orb_data(q_orbs_all.iter());
    let active_count = apply_curriculum_to_spawned_orbs(&mut commands, &orb_data, &mut curriculum);
    info!("Applied curriculum: {} orbs active (max_orbs={:?})", active_count, curriculum.max_orbs);

    // Reset game state with the active orb count
    *state = GameState::default();
    state.nb_orbs = active_count;
    game_state::return_growth(state.deref_mut());
    info!("Game state reset (nb_orbs={})", state.nb_orbs);

    // Note: reset_all_orb_visibilities is called BEFORE the curriculum loop above
    // so that curriculum can properly hide orbs outside limits
    hide_white_arch(&mut commands, q_white_arch);

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

    info!("Player respawned");
}


#[deprecated(since = "0.1.0", note = "Old movement system")]
#[allow(dead_code, unused_variables, unreachable_code)]
fn move_player_simple(
    q_player: Query<(&mut Velocity, &mut Transform), With<Player>>,
    // q_camera: Query<&Transform, (With<FlyCam>, Without<Player>)>, // if not using player camera
    settings: Res<MovementSettings>,
    game_state: Res<GameState>,
    input: Res<ButtonInput<KeyCode>>,
    mapping: Res<KeyMapping>,
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
    if input.pressed(mapping.forward) {
        direction += forward;
    }
    if input.pressed(mapping.backward) {
        direction -= forward;
    }
    if input.pressed(mapping.left) {
        direction -= right;
    }
    if input.pressed(mapping.right) {
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
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_cursor: Query<&CursorOptions, With<PrimaryWindow>>,
    ai_config: Option<Res<AiConfig>>,
    ai_input: Option<Res<AiActionInput>>,
) {
    let Ok(mut player_transform) = q_player.single_mut() else {
        return;
    };
    let Ok(mut camera_transform) = q_camera.single_mut() else {
        return;
    };

    let (mut yaw, _, _) = player_transform.rotation.to_euler(EulerRot::YXZ);
    let (_, mut pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);

    let ai_enabled = ai_config.as_ref().map(|c| c.enabled).unwrap_or(false);
    if ai_enabled {
        // AI mode: read look delta from AiActionInput
        // look.x = pitch delta (ignored), look.y = yaw delta (applied)
        if let Some(ref ai_input) = ai_input {
            yaw -= ai_input.look.y;  // yaw is in look.y
            // pitch control disabled for AI - it doesn't affect movement and is annoying to watch
        }
    } else {
        // Human mode: read from mouse
        let Ok(window) = q_window.single() else {
            return;
        };
        let Ok(cursor_options) = q_cursor.single() else {
            return;
        };
        if cursor_options.grab_mode == CursorGrabMode::None {
            return; // Skip if cursor is not grabbed
        }

        let window_scale = window.height().min(window.width());
        yaw -= (mouse.delta.x * settings.mouse_sens * window_scale).to_radians();
        pitch -= (mouse.delta.y * settings.mouse_sens * window_scale).to_radians();
    }

    pitch = pitch.clamp(-FRAC_PI_2, FRAC_PI_2);

    // Apply mouse movement to the player's rotation
    player_transform.rotation = Quat::from_axis_angle(Vec3::Y, yaw);
    camera_transform.rotation = Quat::from_axis_angle(Vec3::X, pitch);
}

fn cursor_grab(
    mut q_cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut key_input: ResMut<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mapping: Res<KeyMapping>,
) {
    let Ok(mut cursor_options) = q_cursor.single_mut() else { return };

    // Re-grab cursor when clicking into the window (if not currently grabbed)
    if cursor_options.grab_mode == CursorGrabMode::None && mouse_input.just_pressed(MouseButton::Left) {
        set_grab_mode(&mut cursor_options, CursorGrabMode::Locked);
        return;
    }

    // Toggle cursor grab mode on Escape key press
    if key_input.just_pressed(mapping.escape) {
        let grab_mode = match cursor_options.grab_mode {
            CursorGrabMode::None => CursorGrabMode::Locked,
            _ => CursorGrabMode::None,
        };
        set_grab_mode(&mut cursor_options, grab_mode);
        // cursor_options.grab_mode = grab_mode;
        // cursor_options.visible = !cursor_options.visible;
        // cursor_options.visible = grab_mode != CursorGrabMode::Locked;
        // clear input so we can't react to it again.
        key_input.clear_just_pressed(mapping.escape);
    }
}

pub fn set_grab_mode(
    cursor_options: &mut CursorOptions,
    grab_mode: CursorGrabMode,
) {
    cursor_options.grab_mode = grab_mode;
    cursor_options.visible = grab_mode != CursorGrabMode::Locked;
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
    time: Res<Time<Fixed>>,
    mapping: Res<KeyMapping>,
    ai_config: Option<Res<AiConfig>>,
    ai_input: Option<Res<AiActionInput>>,
) {
    let Ok((transform, vel)) = q_player.single() else {
        return;
    };

    let mut desired_accel = Vec3::ZERO;
    let accel_rate = 20.0; // From MovementScripts.cs

    let ai_enabled = ai_config.as_ref().map(|c| c.enabled).unwrap_or(false);
    if ai_enabled {
        // AI mode: read from AiActionInput.move_dir (x = right, y = forward)
        // move_dir is in [-1, 1] range for each axis
        if let Some(ref ai_input) = ai_input {
            let move_forward = -ai_input.move_dir.y; // Negative because forward is -Z
            let move_right = -ai_input.move_dir.x;   // Negative because right is -X in Bevy

            desired_accel += transform.forward().as_vec3() * move_forward;
            desired_accel += transform.right().as_vec3() * move_right;
        }
    } else {
        // Human mode: read from keyboard
        if input.pressed(mapping.forward) {
            desired_accel -= transform.forward().as_vec3();
        }
        if input.pressed(mapping.backward) {
            desired_accel += transform.forward().as_vec3();
        }
        if input.pressed(mapping.left) {
            desired_accel += transform.right().as_vec3();
        }
        if input.pressed(mapping.right) {
            desired_accel -= transform.right().as_vec3();
        }
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
    m_state: Option<Res<MovementAudioState>>,
) {
    // MovementAudioState only exists in non-headless mode
    let Some(m_state) = m_state else { return };
    if m_state.is_decelerating_triggered && accel.0.length_squared() < 0.01 && state.player_speed > 0.1 {
        commands.trigger(PlayMovementSound::Decelerate);
    }
}

fn apply_relativistic_physics(
    mut q_player: Query<&mut Velocity, With<Player>>,
    mut state: ResMut<GameState>,
    accel: Res<PlayerAcceleration>,
    time: Res<Time<Fixed>>,
) {
    let Ok(mut velocity) = q_player.single_mut() else {
        return;
    };

    let drag = 2.0;
    if accel.0.length_squared() == 0.0 {
        let vel = state.player_velocity_vector;
        state.player_velocity_vector -= vel * drag * time.delta_secs();
    }

    let c_sq = state.speed_of_light * state.speed_of_light;

    state.player_velocity_vector = relativity::add_relativistic_velocity(
        state.player_velocity_vector,
        accel.0,
        state.lorentz_factor,
        c_sq,
    );

    let max_speed = state.max_player_speed * state.speed_multiplier;
    state.player_speed = state.player_velocity_vector.length();
    if state.player_speed > max_speed {
        state.player_velocity_vector = state.player_velocity_vector.normalize_or_zero() * max_speed;
        state.player_speed = max_speed;
    }
    let v_sq = state.player_speed * state.player_speed;

    // should this be updated before or after velocity vecotr?
    state.lorentz_factor = (1.0 - v_sq / c_sq).sqrt();

    if state.lorentz_factor.is_nan() {
        velocity.linvel = Vec3::ZERO;
    } else {
        velocity.linvel = -1.0 * (state.player_velocity_vector / state.lorentz_factor);
    }
}

fn apply_collision_drag(
    mut state: ResMut<GameState>,
    mut q_player: Query<(Entity, &mut Transform, &mut Velocity), With<Player>>,
    q_others: Query<Option<&Velocity>, Without<Player>>,
    rapier_ctx: ReadRapierContext,
    time: Res<Time<Fixed>>,
) {
    let Ok((player_entity, mut transform, velocity)) = q_player.single_mut() else {
        return;
    };
    let Ok(rapier_ctx) = rapier_ctx.single() else {
        return;
    };

    for contact_pair in rapier_ctx.contact_pairs_with(player_entity) {
        if !contact_pair.has_any_active_contact() {
            continue;
        }
        // IMPORTANT: Do not modify this. We need it to match the original game's physics.
        for contact in contact_pair.manifolds() {
            let normal = contact.normal();
            let speed2 = contact.rigid_body1().and_then(|e| q_others.get(e).ok()).unwrap_or_default().map_or(0.0, |v| {
                v.linvel.length()
            });

            // Apply drag to the player velocity
            state.player_velocity_vector *= 1.0 - (0.98 * time.delta_secs());

            // todo: this might not handle moving objects correctly (speed + speed != len(velocity - velocity))
            let speed = velocity.linvel.length() + speed2;
            transform.translation += normal.with_y(0.).normalize_or_zero() * speed * 1.25 * time.delta_secs();
        }
    }
}


fn update_speed_of_light(
    mut state: ResMut<GameState>,
) {
    state.sol_target = state.sol_target.max(0.0);
    if state.speed_of_light < state.sol_target as f32 * 0.995 {
        state.speed_of_light += state.sol_step;
    } else if state.speed_of_light > state.sol_target as f32 * 1.005 {
        state.speed_of_light -= state.sol_step;
    } else if state.speed_of_light != state.sol_target as f32 {
        state.speed_of_light = state.sol_target as f32;
    }
}




fn update_misc(
    mut _commands: Commands,
    mut q_player: Query<(&mut Transform, &mut Velocity), With<Player>>,
    mut state: ResMut<GameState>,
    time: Res<Time<Fixed>>,
) {
    let Ok((mut transform, mut velocity)) = q_player.single_mut() else { return };
    velocity.angvel = Vec3::ZERO; // Reset angular velocity

    // do not update player time if the game is won
    if state.game_win { return; }

    if state.player_time > 0.0 || velocity.linvel.length_squared() > 0.0 {
        // Update the player time and world time
        state.player_time += time.delta_secs();
        state.world_time += time.delta_secs() / state.lorentz_factor;
    }
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
        debug!("Player movement resumed, RigidBodyDisabled removed.");
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
        debug!("Player movement resumed, state restored");
        commands.trigger(match state.is_hard_paused {
            true => GameStatePaused::PlayerPaused,
            false => GameStatePaused::Unpaused,
        });
    }
}

pub(crate) fn player_update_done() {}
pub(crate) fn player_update_start() {}


fn process_debug_inputs(
    // mut commands: Commands,
    mut state: ResMut<GameState>,
    input: Res<ButtonInput<KeyCode>>,
    mapping: Res<KeyMapping>,
    mut q_white_arch: Query<(Entity, &mut Visibility), With<WhiteFinishArch>>,
) {
    if input.just_pressed(mapping.toggle_white_arch) {
        let Ok((_white_arch, mut vis)) = q_white_arch.single_mut() else {
            warn!("No white arch found for toggling visibility.");
            return;
        };
        vis.toggle_visible_hidden();
    } else if input.just_pressed(mapping.cheat_99_orbs) {
        state.score = 99.max(state.score);
        state.speed_of_light = state.max_player_speed;
        state.t_step = 99.max(state.t_step);
    }
}
