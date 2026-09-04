//! Uniform-grid spatial index over point sets for sphere / box queries.

use std::collections::HashMap;

pub struct GridIndex {
    cell: f64,
    cells: HashMap<(i32, i32, i32), Vec<u32>>,
    pts: Vec<[f32; 3]>,
}

impl GridIndex {
    pub fn build(pts: Vec<[f32; 3]>, cell: f64) -> Self {
        let mut cells: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
        for (i, p) in pts.iter().enumerate() {
            cells.entry(Self::key(*p, cell)).or_default().push(i as u32);
        }
        GridIndex { cell, cells, pts }
    }

    #[inline]
    fn key(p: [f32; 3], cell: f64) -> (i32, i32, i32) {
        (
            (p[0] as f64 / cell).floor() as i32,
            (p[1] as f64 / cell).floor() as i32,
            (p[2] as f64 / cell).floor() as i32,
        )
    }

    pub fn len(&self) -> usize {
        self.pts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pts.is_empty()
    }

    /// Indices of points inside the sphere, with their distances, sorted by distance.
    pub fn sphere(&self, c: [f64; 3], r: f64) -> Vec<(u32, f64)> {
        let lo = Self::key([(c[0] - r) as f32, (c[1] - r) as f32, (c[2] - r) as f32], self.cell);
        let hi = Self::key([(c[0] + r) as f32, (c[1] + r) as f32, (c[2] + r) as f32], self.cell);
        let r2 = r * r;
        let mut out = Vec::new();
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    if let Some(v) = self.cells.get(&(x, y, z)) {
                        for &i in v {
                            let p = self.pts[i as usize];
                            let d2 = (p[0] as f64 - c[0]).powi(2) + (p[1] as f64 - c[1]).powi(2) + (p[2] as f64 - c[2]).powi(2);
                            if d2 <= r2 {
                                out.push((i, d2.sqrt()));
                            }
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out
    }

    /// Indices of points inside an axis-aligned box.
    pub fn aabb(&self, min: [f64; 3], max: [f64; 3]) -> Vec<u32> {
        let lo = Self::key([min[0] as f32, min[1] as f32, min[2] as f32], self.cell);
        let hi = Self::key([max[0] as f32, max[1] as f32, max[2] as f32], self.cell);
        let mut out = Vec::new();
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    if let Some(v) = self.cells.get(&(x, y, z)) {
                        for &i in v {
                            let p = self.pts[i as usize];
                            if (0..3).all(|k| (p[k] as f64) >= min[k] && (p[k] as f64) <= max[k]) {
                                out.push(i);
                            }
                        }
                    }
                }
            }
        }
        out
    }
}
