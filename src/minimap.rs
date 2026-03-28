use bevy::{
    camera::{RenderTarget, ScalingMode},
    camera::visibility::RenderLayers,
    math::Affine2,
    prelude::*,
    render::render_resource::TextureFormat,
    window::{PrimaryWindow, WindowRef, WindowResolution},
};

use crate::{
    key_mapping::{KeyAction, KeyMapping},
    player::Player,
    relativity::rel_material::{RelativisticMaterial, RelativisticObject},
    scene::CalculatedData,
    scene_loader::setup_scene,
    ui::{is_pause_menu_open, PauseMenuState},
};

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_minimap.after(setup_scene))
            .add_systems(
                Update,
                (
                    // handle_popout_close must run before toggles so it only reacts to
                    // windows closed between frames, not windows just spawned via deferred commands
                    handle_popout_close,
                    (toggle_minimap_mode, toggle_minimap_popout),
                    (sync_minimap_visibility, update_minimap_camera),
                    (spawn_shadow_entities, sync_shadow_visibility),
                )
                    .chain()
                    .run_if(resource_exists::<MinimapState>),
            );
    }
}

#[derive(Resource)]
pub struct MinimapState {
    pub mode: MinimapMode,
    pub popped_out: bool,
    pub render_image: Handle<Image>,
    pub popout_window: Option<Entity>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MinimapMode {
    #[default]
    Off,
    WorldView,
    PlayerView,
}

#[derive(Component)]
struct MinimapWorldCam;

#[derive(Component)]
struct MinimapPlayerCam;

#[derive(Component)]
struct MinimapOverlay;

#[derive(Component)]
struct MinimapPlayerMarker;

#[derive(Component)]
struct MinimapShadowed;

/// Tracks the source parent entity so we can sync visibility (e.g. when orbs are collected).
#[derive(Component)]
struct ShadowSourceParent(Entity);

fn setup_minimap(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create 512x512 render target image (uses Bevy 0.17 helper)
    let render_image = images.add(Image::new_target_texture(
        512,
        512,
        TextureFormat::bevy_default(),
    ));

    // World view camera (layers 1+2): sees shadow entities + player marker
    commands.spawn((
        MinimapWorldCam,
        Camera3d::default(),
        Camera {
            target: render_image.clone().into(),
            order: -1,
            is_active: false,
            clear_color: Color::srgba(0.1, 0.1, 0.12, 1.0).into(),
            ..default()
        },
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 400.0 },
            near: 0.1,
            far: 500.0,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 300.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
        RenderLayers::from_layers(&[1, 2]),
        Name::new("MinimapWorldCam"),
    ));

    // Player view camera (layers 0+2): sees existing relativistic scene + player marker
    commands.spawn((
        MinimapPlayerCam,
        Camera3d::default(),
        Camera {
            target: render_image.clone().into(),
            order: -1,
            is_active: false,
            clear_color: Color::srgba(0.1, 0.1, 0.12, 1.0).into(),
            ..default()
        },
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 400.0 },
            near: 0.1,
            far: 500.0,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 300.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
        RenderLayers::from_layers(&[0, 2]),
        Name::new("MinimapPlayerCam"),
    ));

    // Player marker: flat red circle on layer 2, visible to both minimap cameras
    let marker_mesh = meshes.add(Circle::new(3.0));
    let marker_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.0, 0.0),
        unlit: true,
        ..default()
    });
    commands.spawn((
        MinimapPlayerMarker,
        Mesh3d(marker_mesh),
        MeshMaterial3d(marker_material),
        Transform::from_xyz(0.0, 5.0, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        RenderLayers::layer(2),
        Name::new("MinimapPlayerMarker"),
    ));

    // UI overlay: bottom-right corner, 200x200px
    commands.spawn((
        MinimapOverlay,
        ImageNode::new(render_image.clone()),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(80.0),
            right: Val::Px(16.0),
            width: Val::Px(200.0),
            height: Val::Px(200.0),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.5, 0.5, 0.5, 0.8)),
        GlobalZIndex(800),
        Visibility::Hidden,
        Name::new("MinimapOverlay"),
    ));

    commands.insert_resource(MinimapState {
        mode: MinimapMode::Off,
        popped_out: false,
        render_image,
        popout_window: None,
    });
}

fn toggle_minimap_mode(
    mut state: ResMut<MinimapState>,
    keys: Res<KeyMapping>,
    input: Res<ButtonInput<KeyCode>>,
    pause_menu: Option<Res<PauseMenuState>>,
) {
    if is_pause_menu_open(pause_menu.as_deref()) {
        return;
    }
    if input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight) {
        return;
    }
    if !keys.just_pressed(&input, KeyAction::MinimapToggle) {
        return;
    }
    state.mode = match state.mode {
        MinimapMode::Off => MinimapMode::WorldView,
        MinimapMode::WorldView => MinimapMode::PlayerView,
        MinimapMode::PlayerView => MinimapMode::Off,
    };
    info!("Minimap mode: {:?}", state.mode);
}

fn toggle_minimap_popout(
    mut commands: Commands,
    mut state: ResMut<MinimapState>,
    keys: Res<KeyMapping>,
    input: Res<ButtonInput<KeyCode>>,
    pause_menu: Option<Res<PauseMenuState>>,
    mut images: ResMut<Assets<Image>>,
    mut q_world_cam: Query<&mut Camera, (With<MinimapWorldCam>, Without<MinimapPlayerCam>)>,
    mut q_player_cam: Query<&mut Camera, (With<MinimapPlayerCam>, Without<MinimapWorldCam>)>,
    mut q_overlay: Query<&mut ImageNode, With<MinimapOverlay>>,
) {
    if is_pause_menu_open(pause_menu.as_deref()) {
        return;
    }
    if !(input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight)) {
        return;
    }
    if !keys.just_pressed(&input, KeyAction::MinimapToggle) {
        return;
    }

    if state.popped_out {
        // Close popout window
        if let Some(window_entity) = state.popout_window.take() {
            commands.entity(window_entity).despawn();
        }
        state.popped_out = false;
        // Create fresh 512x512 render image to reset aspect ratio
        let render_image = images.add(Image::new_target_texture(
            512,
            512,
            TextureFormat::bevy_default(),
        ));
        state.render_image = render_image.clone();
        // Retarget cameras back to new render image
        let target: RenderTarget = render_image.clone().into();
        if let Ok(mut cam) = q_world_cam.single_mut() {
            cam.target = target.clone();
        }
        if let Ok(mut cam) = q_player_cam.single_mut() {
            cam.target = target;
        }
        // Update overlay to use new image
        if let Ok(mut overlay) = q_overlay.single_mut() {
            overlay.image = render_image;
        }
        info!("Minimap popout closed");
    } else {
        // Open popout window
        if state.mode == MinimapMode::Off {
            state.mode = MinimapMode::WorldView;
        }
        let window_entity = commands
            .spawn(Window {
                title: "Minimap".to_string(),
                resolution: WindowResolution::new(400, 400),
                ..default()
            })
            .id();
        state.popout_window = Some(window_entity);
        state.popped_out = true;
        // Retarget cameras to the new window
        let target = RenderTarget::Window(WindowRef::Entity(window_entity));
        if let Ok(mut cam) = q_world_cam.single_mut() {
            cam.target = target.clone();
        }
        if let Ok(mut cam) = q_player_cam.single_mut() {
            cam.target = target;
        }
        info!("Minimap popped out to separate window");
    }
}

fn handle_popout_close(
    mut commands: Commands,
    mut state: ResMut<MinimapState>,
    mut images: ResMut<Assets<Image>>,
    q_windows: Query<&Window, Without<PrimaryWindow>>,
    q_primary: Query<&Window, With<PrimaryWindow>>,
    mut q_world_cam: Query<&mut Camera, (With<MinimapWorldCam>, Without<MinimapPlayerCam>)>,
    mut q_player_cam: Query<&mut Camera, (With<MinimapPlayerCam>, Without<MinimapWorldCam>)>,
    mut q_overlay: Query<&mut ImageNode, With<MinimapOverlay>>,
) {
    if !state.popped_out {
        return;
    }
    let Some(window_entity) = state.popout_window else {
        return;
    };

    // Close popout if its window was despawned (user clicked X)
    // or if the primary window was closed (app is exiting)
    let popout_gone = q_windows.get(window_entity).is_err();
    let primary_gone = q_primary.iter().next().is_none();

    if !popout_gone && !primary_gone {
        return;
    }

    if !popout_gone {
        // Primary is gone but popout still exists - despawn it
        commands.entity(window_entity).despawn();
    }

    state.popout_window = None;
    state.popped_out = false;
    // Create fresh 512x512 render image to reset aspect ratio
    let render_image = images.add(Image::new_target_texture(
        512,
        512,
        TextureFormat::bevy_default(),
    ));
    state.render_image = render_image.clone();
    let target: RenderTarget = render_image.clone().into();
    if let Ok(mut cam) = q_world_cam.single_mut() {
        cam.target = target.clone();
    }
    if let Ok(mut cam) = q_player_cam.single_mut() {
        cam.target = target;
    }
    if let Ok(mut overlay) = q_overlay.single_mut() {
        overlay.image = render_image;
    }
    info!("Minimap popout window closed");
}

fn sync_minimap_visibility(
    state: Res<MinimapState>,
    mut q_world_cam: Query<&mut Camera, (With<MinimapWorldCam>, Without<MinimapPlayerCam>)>,
    mut q_player_cam: Query<&mut Camera, (With<MinimapPlayerCam>, Without<MinimapWorldCam>)>,
    mut q_overlay: Query<&mut Visibility, With<MinimapOverlay>>,
) {
    let Ok(mut world_cam) = q_world_cam.single_mut() else { return };
    let Ok(mut player_cam) = q_player_cam.single_mut() else { return };
    let Ok(mut overlay_vis) = q_overlay.single_mut() else { return };

    let world_active = state.mode == MinimapMode::WorldView;
    let player_active = state.mode == MinimapMode::PlayerView;
    let overlay_visible = state.mode != MinimapMode::Off && !state.popped_out;

    if world_cam.is_active != world_active {
        world_cam.is_active = world_active;
    }
    if player_cam.is_active != player_active {
        player_cam.is_active = player_active;
    }
    let target_vis = if overlay_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if *overlay_vis != target_vis {
        *overlay_vis = target_vis;
    }
}

fn update_minimap_camera(
    state: Res<MinimapState>,
    calc_data: Res<CalculatedData>,
    q_player: Query<&Transform, With<Player>>,
    mut q_world_cam: Query<
        (&mut Transform, &mut Projection),
        (
            With<MinimapWorldCam>,
            Without<Player>,
            Without<MinimapPlayerCam>,
            Without<MinimapPlayerMarker>,
        ),
    >,
    mut q_player_cam: Query<
        (&mut Transform, &mut Projection),
        (
            With<MinimapPlayerCam>,
            Without<Player>,
            Without<MinimapWorldCam>,
            Without<MinimapPlayerMarker>,
        ),
    >,
    mut q_marker: Query<
        &mut Transform,
        (
            With<MinimapPlayerMarker>,
            Without<Player>,
            Without<MinimapWorldCam>,
            Without<MinimapPlayerCam>,
        ),
    >,
) {
    if state.mode == MinimapMode::Off {
        return;
    }
    let Ok(player_tf) = q_player.single() else {
        return;
    };

    // Compute view bounds: padded orb bbox expanded to always include the player
    let bb = calc_data.orbs_bb();
    let px = player_tf.translation.x;
    let pz = player_tf.translation.z;
    let max_extent = (bb.max.x - bb.min.x).max(bb.max.z - bb.min.z);
    let pad = (max_extent * 0.025).max(1.0); // 2.5% per side = 5% total
    let view_min_x = (bb.min.x - pad).min(px - pad);
    let view_max_x = (bb.max.x + pad).max(px + pad);
    let view_min_z = (bb.min.z - pad).min(pz - pad);
    let view_max_z = (bb.max.z + pad).max(pz + pad);
    let view_cx = (view_min_x + view_max_x) / 2.0;
    let view_cz = (view_min_z + view_max_z) / 2.0;
    let view_w = (view_max_x - view_min_x).max(50.0);
    let view_h = (view_max_z - view_min_z).max(50.0);

    let cam_pos = Vec3::new(view_cx, 300.0, view_cz);
    let look_at = Vec3::new(view_cx, 0.0, view_cz);
    let scaling = ScalingMode::AutoMin { min_width: view_w, min_height: view_h };

    if let Ok((mut tf, mut proj)) = q_world_cam.single_mut() {
        *tf = Transform::from_translation(cam_pos).looking_at(look_at, Vec3::NEG_Z);
        if let Projection::Orthographic(ref mut ortho) = *proj {
            ortho.scaling_mode = scaling;
        }
    }
    if let Ok((mut tf, mut proj)) = q_player_cam.single_mut() {
        *tf = Transform::from_translation(cam_pos).looking_at(look_at, Vec3::NEG_Z);
        if let Projection::Orthographic(ref mut ortho) = *proj {
            ortho.scaling_mode = scaling;
        }
    }

    // Update player marker position
    if let Ok(mut tf) = q_marker.single_mut() {
        tf.translation = Vec3::new(
            player_tf.translation.x,
            player_tf.translation.y + 5.0,
            player_tf.translation.z,
        );
    }
}

fn spawn_shadow_entities(
    mut commands: Commands,
    q_new: Query<Entity, (With<RelativisticObject>, Without<MinimapShadowed>)>,
    q_children: Query<&Children>,
    q_mesh: Query<(
        &Mesh3d,
        &MeshMaterial3d<RelativisticMaterial>,
        &GlobalTransform,
    )>,
    rel_mats: Res<Assets<RelativisticMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    for entity in q_new.iter() {
        for descendant in q_children.iter_descendants(entity) {
            let Ok((mesh, mat, global_tf)) = q_mesh.get(descendant) else {
                continue;
            };
            let Some(rel_mat) = rel_mats.get(mat.id()) else {
                continue;
            };
            let std_mat = std_mats.add(StandardMaterial {
                base_color_texture: Some(rel_mat.base_texture.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Mask(0.5),
                // Match the V-flip applied by UvFixerPlugin on the original materials
                uv_transform: Affine2::from_scale(Vec2::new(1.0, -1.0)),
                ..default()
            });
            commands.spawn((
                ShadowSourceParent(entity),
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(std_mat),
                global_tf.compute_transform(),
                RenderLayers::layer(1),
                Name::new("MinimapShadow"),
            ));
        }
        commands.entity(entity).insert(MinimapShadowed);
    }
}

/// Sync shadow entity visibility with source parent.
/// Handles both `Disabled` (curriculum, excluded from queries) and
/// `Visibility::Hidden` (orb collected).
fn sync_shadow_visibility(
    q_active_parents: Query<&Visibility, With<RelativisticObject>>,
    mut q_shadows: Query<(&ShadowSourceParent, &mut Visibility), Without<RelativisticObject>>,
) {
    for (source, mut vis) in q_shadows.iter_mut() {
        let target = match q_active_parents.get(source.0) {
            Ok(parent_vis) if *parent_vis != Visibility::Hidden => Visibility::Visible,
            _ => Visibility::Hidden, // parent hidden, disabled, or despawned
        };
        if *vis != target {
            *vis = target;
        }
    }
}
