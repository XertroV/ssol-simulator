use bevy::{math::bounding::{Aabb3d, BoundingVolume}, prelude::*};

#[derive(Resource, Debug)]
pub struct CalculatedData {
    pub orbs_bb: Option<Aabb3d>,
}

impl Default for CalculatedData {
    fn default() -> Self {
        Self {
            orbs_bb: None,
        }
    }
}

const HALF_SIZE: Vec3 = Vec3::splat(0.5);

impl CalculatedData {
    pub fn reset(&mut self) {
        self.orbs_bb = None;
    }

    pub fn merge_orb(&mut self, pos: Vec3) {
        let new_bb = Aabb3d::new(pos, HALF_SIZE);
        self.orbs_bb = match self.orbs_bb {
            Some(bb) => Some(bb.merge(&new_bb)),
            None => Some(new_bb),
        };
    }

    pub fn orbs_bb(&self) -> Aabb3d {
        self.orbs_bb.unwrap_or_else(|| {
            Aabb3d::new(Vec3::ZERO, Vec3::ONE)
        })
    }
}
