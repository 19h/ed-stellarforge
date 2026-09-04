//! Hand-authored named regions ("Col 285 Sector", "Pleiades Sector", ...). In the game these
//! are spheres stored in an octree hanging off the naming manager (+360); the smallest sphere
//! containing the star wins. 485 entries were dumped from the live game.

use crate::id64::GALAXY_ORIGIN;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RegionFile {
    pub name: String,
    pub sphere_center_galaxy_frame_ly: [f32; 3],
    pub radius_ly: f64,
    pub origin_1_32_ly: [i32; 3],
    #[serde(default)]
    pub cube_max_1_32_ly: Option<[i32; 3]>,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub name: String,
    /// Sphere centre in the galaxy frame (not Sol-relative).
    pub center: [f64; 3],
    pub radius: f64,
    pub radius2: f64,
    /// Cube origin in 1/32 ly units (galaxy frame); boxel indices are relative to this.
    pub origin_1_32: [i32; 3],
}

impl Region {
    pub fn center_ingame(&self) -> [f64; 3] {
        [
            self.center[0] - GALAXY_ORIGIN[0],
            self.center[1] - GALAXY_ORIGIN[1],
            self.center[2] - GALAXY_ORIGIN[2],
        ]
    }
    pub fn origin_ingame(&self) -> [f64; 3] {
        [
            self.origin_1_32[0] as f64 / 32.0 - GALAXY_ORIGIN[0],
            self.origin_1_32[1] as f64 / 32.0 - GALAXY_ORIGIN[1],
            self.origin_1_32[2] as f64 / 32.0 - GALAXY_ORIGIN[2],
        ]
    }
}

#[derive(Debug, Clone, Default)]
pub struct Regions {
    pub list: Vec<Region>,
}

impl Regions {
    pub fn from_file_entries(v: Vec<RegionFile>) -> Self {
        let list = v
            .into_iter()
            .map(|r| Region {
                name: r.name,
                center: [
                    r.sphere_center_galaxy_frame_ly[0] as f64,
                    r.sphere_center_galaxy_frame_ly[1] as f64,
                    r.sphere_center_galaxy_frame_ly[2] as f64,
                ],
                radius: r.radius_ly,
                // the game compares against the stored float radius²; reproduce that rounding
                radius2: ((r.radius_ly * r.radius_ly) as f32) as f64,
                origin_1_32: r.origin_1_32_ly,
            })
            .collect();
        Regions { list }
    }

    /// Smallest region sphere containing an in-game position.
    pub fn find(&self, pos: [f64; 3]) -> Option<&Region> {
        let g = [pos[0] + GALAXY_ORIGIN[0], pos[1] + GALAXY_ORIGIN[1], pos[2] + GALAXY_ORIGIN[2]];
        let mut best: Option<&Region> = None;
        for r in &self.list {
            let d2 = (g[0] - r.center[0]).powi(2) + (g[1] - r.center[1]).powi(2) + (g[2] - r.center[2]).powi(2);
            if d2 < r.radius2 && best.is_none_or(|b| r.radius2 < b.radius2) {
                best = Some(r);
            }
        }
        best
    }

    /// Regions whose sphere intersects a sphere (in-game centre / radius).
    pub fn intersecting(&self, center: [f64; 3], radius: f64) -> Vec<&Region> {
        let c = center_to_galaxy(center);
        self.list
            .iter()
            .filter(|r| {
                let d = ((c[0] - r.center[0]).powi(2) + (c[1] - r.center[1]).powi(2) + (c[2] - r.center[2]).powi(2)).sqrt();
                d < r.radius + radius
            })
            .collect()
    }

    pub fn by_name(&self, name: &str) -> Vec<&Region> {
        let n = name.to_ascii_lowercase();
        self.list.iter().filter(|r| r.name.to_ascii_lowercase().contains(&n)).collect()
    }
}

fn center_to_galaxy(p: [f64; 3]) -> [f64; 3] {
    [p[0] + GALAXY_ORIGIN[0], p[1] + GALAXY_ORIGIN[1], p[2] + GALAXY_ORIGIN[2]]
}
