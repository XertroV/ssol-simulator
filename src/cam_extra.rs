use bevy::render::camera::{PerspectiveProjection, Projection};


pub trait HasFov {
    fn get_fov(&self) -> f32;
    fn set_fov(&mut self, fov: f32);
}

impl HasFov for Projection {
    fn get_fov(&self) -> f32 {
        match self {
            Projection::Custom(p) => match p.get::<PerspectiveProjection>() {
                Some(perspective) => perspective.fov,
                None => 0.0,
            },
            Projection::Perspective(perspective) => perspective.fov,
            Projection::Orthographic(_) => 0.0,
        }
    }

    fn set_fov(&mut self, fov: f32) {
        match self {
            Projection::Perspective(perspective) => perspective.fov = fov,
            Projection::Orthographic(_) => {},
            Projection::Custom(p) => {
                if let Some(perspective) = p.get_mut::<PerspectiveProjection>() {
                    perspective.fov = fov;
                }
            },
        }
    }
}
