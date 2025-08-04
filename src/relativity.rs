use bevy::prelude::*;

pub mod rel_material;
pub mod rel_parent;

/// Performs relativistic velocity addition.
/// Ported directly from the logic in MovementScripts.cs.
///
/// # Arguments
/// * `current_velocity` - The player's current velocity
/// * `acceleration`
/// * `lorentz_factor`
/// * `speed_of_light_sqrd`
///
/// # Returns
/// * The new velocity vector after relativistic addition.
pub fn add_relativistic_velocity(
    current_velocity: Vec3,
    acceleration: Vec3,
    lorentz_factor: f32,
    speed_of_light_sqrd: f32,
) -> Vec3 {
    if current_velocity.length_squared() == 0.0 {
        // If we are stationary, the new velocity is just the acceleration.
        return acceleration;
    }

    // Find the rotation that aligns the current velocity with the X-axis.
    let to_x_axis = Quat::from_rotation_arc(current_velocity.normalize(), Vec3::X);
    // And the inverse rotation to get back to world space.
    let from_x_axis = to_x_axis.inverse();

    // Rotate the current velocity and acceleration into the new frame of reference.
    let mut v = to_x_axis * current_velocity;
    let a = to_x_axis * acceleration;

    // Apply the relativistic velocity addition formula.
    // This is a direct port of the C# code:
    // v = 1f / (1f + v.x * a.x / c^2) * new Vector3(a.x + v.x, a.y * gamma, a.z * gamma);
    let denominator = 1.0 + (v.x * a.x) / speed_of_light_sqrd;
    v = (1.0 / denominator) * Vec3::new(a.x + v.x, a.y * lorentz_factor, a.z * lorentz_factor);

    // Rotate the new velocity back into world space.
    from_x_axis * v
}
