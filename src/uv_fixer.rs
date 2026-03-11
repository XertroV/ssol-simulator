use bevy::{camera::primitives::MeshAabb, math::Affine2, prelude::*, scene::SceneInstanceReady};

/// A plugin that fixes up meshes after we load them.
/// Flips the V-coordinate of UVs to fix materials.
/// Also updates the AABBs of meshes to ensure they are correct after transformations.
pub struct UvFixerPlugin;

impl Plugin for UvFixerPlugin {
    fn build(&self, app: &mut App) {
        // app.add_systems(Update, fix_inverted_uvs_on_new_meshes);
        app
        //     // .add_observer(fix_aabb)
            .add_observer(flip_uv_once)
            ;
    }
}

#[deprecated(note = "moved to relativity material processing to set a much larger AABB")]
#[allow(dead_code, unused_variables, unused_mut)]
fn fix_aabb(
    ready: On<SceneInstanceReady>,
    children: Query<&Children>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<(Entity, &Mesh3d), With<Mesh3d>>,
    mut commands: Commands,
) {
    panic!("deprecated");
    // info!("Fixing AABBs for scene instance: {:?}", ready.entity);
    #[allow(unreachable_code)]
    for descendant in children.iter_descendants(ready.entity) {
        if let Ok((ent, m3d)) = query.get(descendant) {
            if let Some(mesh) = meshes.get_mut(m3d) {
                // if mesh.indices().is_none() {
                //     // mesh.insert_indices(Indices::U16((0..mesh.count_vertices() as u16).collect()));
                //     info!("Inserted indices for mesh: {:?}", ent);
                // }
                // assert!(mesh.indices().is_some(), "Mesh {:?} has no indices", ent);
                // mesh.compute_smooth_normals();
                // if let Err(e) = mesh.generate_tangents() {
                //     warn!("Failed to generate tangents for mesh {:?}: {:?}", ent, e);
                // }
                // let nb_normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().len();
                // if nb_normals != mesh.count_vertices() {
                //     warn!("Mesh {:?} has {} normals, expected {}", ent, nb_normals, mesh.count_vertices());
                // }
                // let vertex_count = mesh.count_vertices();
                if let Some(new_aabb) = mesh.compute_aabb() {
                    commands.entity(ent).insert(new_aabb);
                }
            }
        }
    }
}


/// Fix UVs by flipping Y axis. Also set alpha_mode etc on plant materials.
fn flip_uv_once(
    ready: On<SceneInstanceReady>,
    children: Query<&Children>,
    mesh_mats: Query<&MeshMaterial3d<StandardMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for descendant in children.iter_descendants(ready.entity) {
        if let Ok(mat_handle) = mesh_mats.get(descendant) {
            if let Some(mat) = mats.get_mut(&mat_handle.0) {
                mat.uv_transform = Affine2::from_scale(Vec2::new(1.0, -1.0));
                // if mat.unlit {
                //     continue; // already set
                // }

                // if let Some(label) = mat.base_color_texture.as_ref().and_then(|t| t.path()) {
                //     let label = label.to_string();
                //     if label.contains("/shrub") || label.contains("/bush") || label.contains("/weeds") {
                //         // info!("Fixing UVs for shrub/bush/weeds material: {}", label);
                //         mat.unlit = true;
                //         mat.alpha_mode = AlphaMode::Mask(0.5);
                //         mat.cull_mode = None;
                //     }
                // }
            }
        }
    }
}
