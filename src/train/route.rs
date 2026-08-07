//! WR (world-record) high-level route for level-zero.
//!
//! `orb_id` values match spawn-distance ordering (`OrbId` in the game).

use bevy::prelude::*;
use serde::Deserialize;
use std::path::Path;

/// One stop on the WR tour.
///
/// `seq` / `phase` come from the WR JSON asset (diagnostics / tooling); routing
/// uses `orb_id` + position.
#[derive(Clone, Debug, Deserialize)]
pub struct RouteStop {
    #[allow(dead_code)] // JSON schema / tooling; not needed for next_target
    pub seq: u32,
    pub orb_id: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    #[serde(default)]
    #[allow(dead_code)] // JSON schema / tooling; not needed for next_target
    pub phase: String,
}

impl RouteStop {
    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

/// Full WR route resource (ordered orb visits).
#[derive(Resource, Clone, Debug)]
pub struct WrRoute {
    pub stops: Vec<RouteStop>,
    pub last_orb_id: u8,
}

impl WrRoute {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read WR route {}: {e}", path.display()))?;
        Self::from_json_str(&text)
    }

    pub fn from_json_str(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            last_orb_id: u8,
            order: Vec<RouteStop>,
        }
        let file: File = serde_json::from_str(text)
            .map_err(|e| format!("failed to parse WR route JSON: {e}"))?;
        if file.order.is_empty() {
            return Err("WR route has empty order".into());
        }
        Ok(Self {
            stops: file.order,
            last_orb_id: file.last_orb_id,
        })
    }

    /// Next uncollected stop in WR order.
    ///
    /// `collected` is the set of orb_ids already picked up this episode.
    /// `active` if Some restricts to orbs present under the current curriculum.
    ///
    /// Runtime targeting uses [`crate::train::ActiveRoute`]; this remains the
    /// WR-only helper (unit tests / tooling).
    #[allow(dead_code)] // public WR API; episode loop uses ActiveRoute
    pub fn next_target(
        &self,
        collected: &std::collections::HashSet<u8>,
        active: Option<&std::collections::HashSet<u8>>,
    ) -> Option<&RouteStop> {
        self.stops.iter().find(|s| {
            if collected.contains(&s.orb_id) {
                return false;
            }
            if let Some(active) = active {
                if !active.contains(&s.orb_id) {
                    return false;
                }
            }
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const MINI_ROUTE: &str = r#"{
        "last_orb_id": 2,
        "order": [
            {"seq": 0, "orb_id": 4, "x": 1.0, "y": 0.0, "z": 2.0, "phase": "blue_path"},
            {"seq": 1, "orb_id": 1, "x": 3.0, "y": 0.0, "z": 4.0, "phase": "blue_path"},
            {"seq": 2, "orb_id": 2, "x": 5.0, "y": 0.0, "z": 6.0, "phase": "straight_shot"}
        ]
    }"#;

    #[test]
    fn parses_and_selects_next_target() {
        let route = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        assert_eq!(route.stops.len(), 3);
        assert_eq!(route.last_orb_id, 2);

        let mut collected = HashSet::new();
        let t0 = route.next_target(&collected, None).unwrap();
        assert_eq!(t0.orb_id, 4);

        collected.insert(4);
        let t1 = route.next_target(&collected, None).unwrap();
        assert_eq!(t1.orb_id, 1);

        collected.insert(1);
        collected.insert(2);
        assert!(route.next_target(&collected, None).is_none());
    }

    #[test]
    fn respects_active_curriculum_set() {
        let route = WrRoute::from_json_str(MINI_ROUTE).unwrap();
        let collected = HashSet::new();
        let active: HashSet<u8> = [1u8, 2].into_iter().collect();
        let t = route.next_target(&collected, Some(&active)).unwrap();
        // orb 4 is first in WR order but not active → skip to 1
        assert_eq!(t.orb_id, 1);
    }
}
