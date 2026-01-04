//! Physics transform interpolation for smooth rendering.
//!
//! When physics runs in FixedUpdate (100Hz) and rendering happens at a different
//! rate (e.g., 60fps or 144fps), we need to interpolate between physics states
//! for smooth visuals. This module provides components and systems for that.

use bevy::prelude::*;

/// Plugin for physics transform interpolation
pub struct PhysicsInterpolationPlugin;

impl Plugin for PhysicsInterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPostUpdate,
            store_previous_transforms,
        )
        .add_systems(
            PostUpdate,
            interpolate_transforms,
        );
    }
}

/// Marker component for entities that should have their transforms interpolated.
/// Add this to any physics entity that needs smooth visual rendering.
#[derive(Component, Default)]
pub struct InterpolateTransform {
    /// If true, only interpolate translation (useful when rotation is controlled separately)
    pub translation_only: bool,
}

impl InterpolateTransform {
    pub fn new() -> Self {
        Self { translation_only: false }
    }

    pub fn translation_only() -> Self {
        Self { translation_only: true }
    }
}

/// Stores the previous physics transform for interpolation.
/// This is automatically managed by the interpolation systems.
#[derive(Component, Default)]
pub struct PreviousTransform {
    pub translation: Vec3,
    pub rotation: Quat,
}

/// Stores the current physics transform (the target we interpolate towards).
/// Updated at the end of each FixedUpdate.
#[derive(Component, Default)]
pub struct PhysicsTransform {
    pub translation: Vec3,
    pub rotation: Quat,
}

/// System that runs at the end of FixedUpdate to store transform states.
/// Previous becomes what was current, current becomes the new physics state.
fn store_previous_transforms(
    mut query: Query<
        (&Transform, &mut PreviousTransform, &mut PhysicsTransform),
        With<InterpolateTransform>,
    >,
) {
    for (transform, mut prev, mut current) in query.iter_mut() {
        // Move current to previous
        prev.translation = current.translation;
        prev.rotation = current.rotation;

        // Store new current from physics
        current.translation = transform.translation;
        current.rotation = transform.rotation;
    }
}

/// System that runs in PostUpdate to interpolate transforms for rendering.
/// Uses the overstep fraction to determine how far between physics ticks we are.
fn interpolate_transforms(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<
        (&mut Transform, &InterpolateTransform, &PreviousTransform, &PhysicsTransform),
    >,
) {
    // overstep_fraction() returns how far we are between the last FixedUpdate and the next
    // 0.0 = just after a FixedUpdate, approaching 1.0 = just before the next
    let alpha = fixed_time.overstep_fraction();

    for (mut transform, interp, prev, current) in query.iter_mut() {
        // Linearly interpolate position
        transform.translation = prev.translation.lerp(current.translation, alpha);

        // Only interpolate rotation if not in translation-only mode
        // (translation_only is useful when rotation is controlled by input, not physics)
        if !interp.translation_only {
            transform.rotation = prev.rotation.slerp(current.rotation, alpha);
        }
    }
}

/// Bundle for adding interpolation to an entity.
/// Add this to entities that have physics and need smooth rendering.
#[derive(Bundle, Default)]
pub struct InterpolationBundle {
    pub marker: InterpolateTransform,
    pub previous: PreviousTransform,
    pub current: PhysicsTransform,
}

impl InterpolationBundle {
    pub fn from_transform(transform: &Transform) -> Self {
        Self {
            marker: InterpolateTransform::new(),
            previous: PreviousTransform {
                translation: transform.translation,
                rotation: transform.rotation,
            },
            current: PhysicsTransform {
                translation: transform.translation,
                rotation: transform.rotation,
            },
        }
    }

    /// Create an interpolation bundle that only interpolates translation.
    /// Use this when rotation is controlled by input (e.g., mouse look) rather than physics.
    pub fn translation_only(transform: &Transform) -> Self {
        Self {
            marker: InterpolateTransform::translation_only(),
            previous: PreviousTransform {
                translation: transform.translation,
                rotation: transform.rotation,
            },
            current: PhysicsTransform {
                translation: transform.translation,
                rotation: transform.rotation,
            },
        }
    }
}
