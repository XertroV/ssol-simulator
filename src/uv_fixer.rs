use bevy::{math::Affine2, prelude::*, render::mesh::{MeshAabb, VertexAttributeValues}, scene::SceneInstanceReady};

//==============================================================================
// PLUGIN
//==============================================================================

/// A plugin that automatically flips the V-coordinate of UVs on newly loaded meshes.
/// This fixes textures appearing upside-down on models from some exporters.
pub struct UvFixerPlugin;

impl Plugin for UvFixerPlugin {
    fn build(&self, app: &mut App) {
        // app.add_systems(Update, fix_inverted_uvs_on_new_meshes);
        app.add_observer(fix_aabb)
            .add_observer(flip_uv_once);
    }
}


fn fix_aabb(
    ready: Trigger<SceneInstanceReady>,
    children: Query<&Children>,
    meshes: Res<Assets<Mesh>>,
    query: Query<(Entity, &Mesh3d), With<Mesh3d>>,
    mut commands: Commands,
) {
    // info!("Fixing AABBs for scene instance: {:?}", ready.target());
    for descendant in children.iter_descendants(ready.target()) {
        if let Ok((ent, m3d)) = query.get(descendant) {
            if let Some(mesh) = meshes.get(m3d) {
                if let Some(new_aabb) = mesh.compute_aabb() {
                    commands.entity(ent).insert(new_aabb);
                }
            }
        }
    }
}


fn flip_uv_once(
    mut ready: Trigger<SceneInstanceReady>,
    children: Query<&Children>,
    mesh_mats: Query<&MeshMaterial3d<StandardMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for descendant in children.iter_descendants(ready.target()) {
        if let Ok(mat_handle) = mesh_mats.get(descendant) {
            if let Some(mat) = mats.get_mut(&mat_handle.0) {
                // Check a marker: maybe via a custom extension field or name
                mat.uv_transform = Affine2::from_scale(Vec2::new(1.0, -1.0));
                mat.alpha_mode = AlphaMode::Mask(0.5);
                mat.cull_mode = None;
            }
        }
    }
}


// //==============================================================================
// // UPDATE SYSTEM
// //==============================================================================

// /// A system that queries for newly added mesh handles and flips their UVs.
// /// The `Added<Handle<Mesh>>` filter ensures this runs only once per mesh.
// fn fix_inverted_uvs_on_new_meshes(
//     mut meshes: ResMut<Assets<Mesh>>,
//     query: Query<&Mesh3d, Added<Mesh3d>>,
// ) {
//     for mesh_handle in query.iter() {
//         // Get mutable access to the mesh asset.
//         if let Some(mesh) = meshes.get_mut(mesh_handle) {
//             // Get the UV coordinates (if they exist).
//             if let Some(VertexAttributeValues::Float32x2(uvs)) =
//                 mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
//             {
//                 // For each [u, v] pair, replace v with 1.0 - v.
//                 for uv in uvs.iter_mut() {
//                     uv[1] = 1.0 - uv[1];
//                 }
//                 // This log helps confirm that the system is working.
//                 // You can comment it out once you know it's running correctly.
//                 info!("Flipped UVs for mesh: {:?}", mesh_handle.id());
//             }
//         }
//     }
// }
