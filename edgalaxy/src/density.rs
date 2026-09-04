//! Galaxy density model: a literal port of `GalaxyDensity_BoxelMass` (0x144CA2790),
//! `GalaxyDensity_CylSummedArea` (0x144CA2E40), the bilinear table sampler (0x144CA3350) and
//! the population-weight lookup (0x144CA2D60 / 0x143C3CB30), operating on the tables dumped
//! from the live game (`density/` directory). All arithmetic is the game's fixed point.

use crate::id64::Boxel;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct MapInfo {
    name: String,
    w: u32,
    h: u32,
}
#[derive(Debug, Deserialize)]
struct Manifest {
    maps: Vec<MapInfo>,
    weights: MapInfo,
    cyl: MapInfo,
}

pub struct Map<T> {
    pub w: u32,
    pub h: u32,
    pub data: Vec<T>,
}

impl<T: Copy + Into<u64>> Map<T> {
    #[inline]
    fn at(&self, x: u32, y: u32) -> u64 {
        self.data[(y * self.w + x) as usize].into()
    }
    /// `sub_144CA3530/3460`: value at x and x+1 on one row (x+1 clamped to the row end).
    #[inline]
    fn pair(&self, x: u32, y: u32) -> (u64, u64) {
        let x1 = if x + 1 >= self.w { self.w - 1 } else { x + 1 };
        (self.at(x, y), self.at(x1, y))
    }
    /// `sub_144CA3350`: bilinear sample with 9-bit fractional coordinates (a3 = column
    /// coordinate, a4 = row coordinate). Returns 0 outside the interior.
    pub fn bilinear(&self, a3: u32, a4: u32) -> u64 {
        let x = a3 >> 9;
        let y = a4 >> 9;
        if x + 1 >= self.w || y + 1 >= self.h {
            return 0;
        }
        let fx = ((a3 & 0x1FF) << 6) as u64;
        let fy = ((a4 & 0x1FF) << 6) as u64;
        let (v00, v01) = self.pair(x, y);
        let (v10, v11) = self.pair(x, y + 1);
        let row0 = (((0x8000 - fx) * v00) >> 15) + ((fx * v01) >> 15);
        let row1 = ((fx * v11) >> 15) + (((0x8000 - fx) * v10) >> 15);
        (((0x8000 - fy) * row0) >> 15) + ((fy * row1) >> 15)
    }
}

pub struct Density {
    /// First 0x200 bytes of the in-game density object.
    obj: Vec<u8>,
    pub mips: Vec<Map<u32>>,
    pub weights: Map<u16>,
    pub cyl: Map<u16>,
}

fn read_map<T, F: Fn(&[u8]) -> T>(dir: &Path, info: &MapInfo, elem: usize, f: F) -> Result<Map<T>> {
    let p = dir.join(&info.name);
    let b = std::fs::read(&p).with_context(|| format!("read {}", p.display()))?;
    anyhow::ensure!(b.len() == (info.w * info.h) as usize * elem, "size mismatch in {}", p.display());
    Ok(Map { w: info.w, h: info.h, data: b.chunks_exact(elem).map(f).collect() })
}

impl Density {
    pub fn load(dir: &Path) -> Result<Self> {
        let m: Manifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
        let obj = std::fs::read(dir.join("density_object.bin"))?;
        anyhow::ensure!(obj.len() >= 0x200, "density_object.bin too short");
        let u32le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let u16le = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]);
        let mut mips = Vec::new();
        for info in &m.maps {
            mips.push(read_map(dir, info, 4, u32le)?);
        }
        anyhow::ensure!(mips.len() == 8, "expected 8 density mips");
        Ok(Density {
            obj,
            mips,
            weights: read_map(dir, &m.weights, 2, u16le)?,
            cyl: read_map(dir, &m.cyl, 2, u16le)?,
        })
    }

    #[inline]
    fn u32(&self, byte_off: usize) -> u32 {
        u32::from_le_bytes(self.obj[byte_off..byte_off + 4].try_into().unwrap())
    }

    /// Galaxy centre used by the cylindrical model, in 10-ly units (fields +44/+48/+52 / 20).
    pub fn center10(&self) -> [u32; 3] {
        [self.u32(44) / 20, self.u32(48) / 20, self.u32(52) / 20]
    }

    /// `GalaxyDensity_CylSummedArea`: vertical/radial factor for a boxel (15-bit fixed point).
    pub fn cyl_factor(&self, b: &Boxel) -> u64 {
        let mc = b.mc as u32;
        let (x10, y10, z10) = b.coords10();
        let x0 = (x10 & 0x3FFF) as i64;
        let y0 = (y10 & 0x1FFF) as i64;
        let z0 = (z10 & 0x3FFF) as i64;
        let e = 1i64 << mc;
        let c = self.center10();
        let (cx, cy, cz) = (c[0] as i64, c[1] as i64, c[2] as i64);
        let dx0 = (x0 - cx).abs() as f64;
        let dx1 = (x0 + e - cx).abs() as f64;
        let dz0 = (z0 - cz).abs() as f64;
        let dz1 = (z0 + e - cz).abs() as f64;
        let mut hy0 = (y0 - cy).unsigned_abs();
        let mut hy1 = (y0 + e - cy).unsigned_abs();
        if hy0 > hy1 {
            std::mem::swap(&mut hy0, &mut hy1);
        }
        let rscale = self.u32(12) as u64;
        let hscale = self.u32(8) as u64;
        let radius_coord = |dz: f64, dx: f64| -> u32 {
            let r = (dz * dz + dx * dx).sqrt() * 32768.0;
            let ri = if r >= 9.223372036854776e18 { (r - 9.223372036854776e18) as u64 | (1u64 << 63) } else { r as u64 };
            ((rscale.wrapping_mul(ri)) >> 37) as u32
        };
        let r00 = radius_coord(dz0, dx0);
        let r01 = radius_coord(dz0, dx1);
        let r10 = radius_coord(dz1, dx0);
        let r11 = radius_coord(dz1, dx1);
        let hlo = (((hscale * (hy0 << 24)) >> 32) >> 7) as u32;
        let hhi = (((hscale * (hy1 << 24)) >> 32) >> 7) as u32;
        let t = |h: u32, r: u32| self.cyl.bilinear(h, r) as i64;
        let sum = t(hlo, r00) + t(hlo, r01) + t(hlo, r10) + t(hlo, r11) - t(hhi, r00) - t(hhi, r01) - t(hhi, r10) - t(hhi, r11);
        ((sum as u32) >> 3) as u64
    }

    /// `GalaxyDensity_BoxelMass`: raw mass budget of a boxel (game fixed point; the generator
    /// multiplies this by 256 and treats 1<<24 as one unit of the per-mass-code factor).
    pub fn boxel_mass(&self, b: &Boxel) -> u64 {
        let mc = b.mc as u32;
        let map = &self.mips[mc as usize];
        let (x10, _, z10) = b.coords10();
        let v8 = (self.u32(0) as u64) << (2 * mc);
        let zc = ((((self.u32(16) as u64) * (((z10 & 0x3FFF) as u64) << 9)) >> 31) >> mc) as u32;
        let xc = ((((self.u32(4) as u64) * (((x10 & 0x3FFF) as u64) << 9)) >> 31) >> mc) as u32;
        let v12 = if (xc >> 9) + 1 >= map.w || (zc >> 9) + 1 >= map.h {
            0
        } else {
            let fx = ((xc & 0x1FF) << 6) as u64;
            let fz = ((zc & 0x1FF) << 6) as u64;
            let (v16, v17) = map.pair(xc >> 9, zc >> 9);
            let (v14, v15) = map.pair(xc >> 9, (zc >> 9) + 1);
            ((fz * ((((0x8000 - fx) * v14) >> 15) + ((fx * v15) >> 15))) >> 15)
                + (((0x8000 - fz) * (((fx * v17) >> 15) + (((0x8000 - fx) * v16) >> 15))) >> 15)
        };
        let cylf = self.cyl_factor(b);
        (cylf * ((v8 * v12) >> 15)) >> 15
    }

    /// Population-weight sample for a boxel (`sub_144CA2D60`), 15-bit fixed point.
    pub fn weight_sample(&self, b: &Boxel) -> u64 {
        let mc = b.mc as u32;
        let (x10, _, z10) = b.coords10();
        let w = self.weights.w as u64;
        let h = self.weights.h as u64;
        let sx = ((w << 15) << 15) / ((self.u32(44) as u64) << 15);
        let sz = ((h << 15) << 15) / ((self.u32(52) as u64) << 15);
        let px = ((5u64 << mc) + 10 * ((x10 & 0x3FFF) as u64)) << 9;
        let pz = ((5u64 << mc) + 10 * ((z10 & 0x3FFF) as u64)) << 9;
        self.weights.bilinear(((sx * px) >> 15) as u32, ((sz * pz) >> 15) as u32)
    }

    /// `GalaxyDensity_RegionType` weights for the three stellar populations (0, 1, 2),
    /// each 0..=32768; `None` when all three are equal (the game returns -1).
    pub fn population_weights(&self, b: &Boxel) -> Option<[u32; 3]> {
        let v = self.weight_sample(b);
        let f = |i: usize| self.u32(4 * i) as u64;
        let clamp = |x: u64| x.min(0x8000) as u32;
        let w0 = if v <= f(93) {
            0
        } else if v <= f(94) {
            clamp(f(84) + ((v * f(83)) >> 15))
        } else {
            0x8000
        };
        let w1 = if v < f(97) {
            if v > f(95) { clamp(f(89) + ((f(87) * v) >> 15)) } else { 0 }
        } else if v >= f(96) {
            0
        } else {
            clamp(f(90) + ((v * f(88)) >> 15))
        };
        let w2 = if v >= f(92) {
            0
        } else if v <= f(91) {
            0x8000
        } else {
            clamp(f(86) + ((v * f(85)) >> 15))
        };
        if w0 == w1 && w1 == w2 {
            None
        } else {
            Some([w0, w1, w2])
        }
    }

    pub fn mip_dims(&self) -> Vec<(u8, u32, u32)> {
        self.mips.iter().enumerate().map(|(i, m)| (i as u8, m.w, m.h)).collect()
    }
}
