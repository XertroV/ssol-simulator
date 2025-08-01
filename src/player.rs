use std::f32::consts::FRAC_PI_2;

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    pbr::ShadowFilteringMethod,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};
use bevy_rapier3d::prelude::*;

use crate::{
    game_state::{self, GameState, Orb},
    relativity,
    scene_loader::PlayerStart,
};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementSettings>()
            .init_resource::<PlayerAcceleration>()
            .add_systems(
                Startup,
                spawn_player.after(crate::scene_loader::setup_scene),
            )
            .add_systems(
                Update,
                (
                    (
                        calculate_player_acceleration,
                        apply_collision_drag,
                        apply_relativistic_physics,
                        update_misc,
                        update_player_look,
                    )
                        .chain(),
                    cursor_grab,
                    detect_orb_collisions,
                    game_state::speed_boost_decay_system,
                ),
            );
    }
}

#[derive(Resource)]
pub struct MovementSettings {
    pub speed: f32,
    pub sensitivity: f32,
}

impl Default for MovementSettings {
    fn default() -> Self {
        Self {
            speed: 40.0,
            sensitivity: 0.00012,
        }
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerCamera;

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
    commands
        .spawn((
            Player,
            PlayerCamera,
            // the transform has scale = 4. already for the size of the collider
            Collider::cuboid(0.5, 0.5, 0.5), // Collider for the player; original game is square
            transform.clone(),
            GlobalTransform::default(),
            RigidBody::Dynamic,
            LockedAxes::from_bits_retain(
                LockedAxes::ROTATION_LOCKED.bits() | LockedAxes::TRANSLATION_LOCKED.bits(),
            ),
            Friction::coefficient(1.0),
            Sleeping::disabled(),
            Velocity::zero(),
            GravityScale(0.0), // Disable gravity for the player
            Ccd::enabled(),
            Name::new("Player"),
            // KinematicCharacterController {
            //     ..Default::default()
            // },
        ))
        .insert((
            InheritedVisibility::HIDDEN,
            ActiveEvents::COLLISION_EVENTS,
            ExternalImpulse::default(),
            // LockedAxes::TRANSLATION_LOCKED_Y,
            // Mesh3d(meshes.add(Cuboid::from_length(1.0))),
            // MeshMaterial3d(materials.add(StandardMaterial {
            //     base_color: Color::srgba(0.2, 0.8, 0.2, 0.9),
            //     cull_mode: None,
            //     alpha_mode: AlphaMode::Add,
            //     unlit: true,
            //     ..default()
            // })),
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
            ));
        });
}

fn move_player_simple(
    mut q_player: Query<(&mut Velocity, &mut Transform), With<Player>>,
    // q_camera: Query<&Transform, (With<FlyCam>, Without<Player>)>, // if not using player camera
    settings: Res<MovementSettings>,
    game_state: Res<GameState>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    panic!("This function is deprecated. Use `calculate_player_acceleration` instead.");
    if !game_state.use_player_cam {
        return; // Skip if not using player camera
    }

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

    velocity.linvel = direction * settings.speed;
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
    if !game_state.use_player_cam {
        return; // Skip if not using player camera
    }

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
    yaw -= (mouse.delta.x * settings.sensitivity * window_scale).to_radians();
    pitch -= (mouse.delta.y * settings.sensitivity * window_scale).to_radians();
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
    let Ok(mut window) = q_window.single_mut() else {
        return;
    };

    // Toggle cursor grab mode on Escape key press
    if input.just_pressed(KeyCode::Escape) {
        window.cursor_options.grab_mode = match window.cursor_options.grab_mode {
            CursorGrabMode::None => CursorGrabMode::Locked,
            _ => CursorGrabMode::None,
        };
        window.cursor_options.visible = !window.cursor_options.visible;
        input.clear_just_pressed(KeyCode::Escape);
    }
}

fn detect_orb_collisions(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    mut q_player: Query<(Entity, &mut Velocity), With<Player>>,
    q_orbs: Query<(Entity, &ChildOf), (With<ChildOf>, With<Orb>)>,
    time: Res<Time>,
) {
    let Ok(mut player) = q_player.single_mut() else {
        return;
    };
    for event in collision_events.read() {
        if let CollisionEvent::Started(ent1, ent2, _) = event {
            // info!("Collision detected: {:?} with {:?}", ent1, ent2);
            let (collided_obj, _) = match (*ent1 == player.0, *ent2 == player.0) {
                (true, false) => (ent2, ent1),
                (false, true) => (ent1, ent2),
                _ => continue, // Not a collision with the player
            };

            // did we hit an orb?
            if let Ok(orb_ent) = q_orbs.get(*collided_obj) {
                let orb_p = orb_ent.1.parent();
                commands.entity(orb_p).despawn();
                commands.trigger(game_state::OrbPickedUp(orb_p));
                continue;
            }
        }
    }
}

// Movement Scripts

fn calculate_player_acceleration(
    mut accel: ResMut<PlayerAcceleration>,
    q_player: Query<&Transform, With<Player>>,
    settings: Res<MovementSettings>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let Ok(transform) = q_player.single() else {
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
    if state.player_velocity_vector.length_squared() > max_speed * max_speed {
        state.player_velocity_vector = state.player_velocity_vector.normalize_or_zero() * max_speed;
    }

    let v_sq = state.player_velocity_vector.length_squared();
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
    rapier_ctx: ReadRapierContext,
    time: Res<Time>,
) {
    let Ok((player_entity, mut transform, mut velocity)) = q_player.single_mut() else {
        return;
    };
    let Ok(rapier_ctx) = rapier_ctx.single() else {
        return;
    };
    // info!("Checking collisions for player: {:?}", player.0);

    for contact_pair in rapier_ctx.contact_pairs_with(player_entity) {
        if contact_pair.has_any_active_contact() {
            // let r = &contact_pair.raw;
            // info!("Collision detected with player: {:?} {:?}", r.collider1, r.collider2);

            // Apply drag to the player velocity
            state.player_velocity_vector *= 1.0 - (0.98 * time.delta_secs());

            transform.translation -= velocity.linvel * time.delta_secs() * 2.0;

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
    // transform.translation -= velocity.linvel * time.delta_secs();
    // transform.translation -= state.player_velocity_vector * time.delta_secs();
    // Reset the velocity to zero after applying it
    // velocity.linvel = Vec3::ZERO;
}
