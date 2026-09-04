//! Hand-authored star system records (96 bytes each in the game's naming object).
//!
//! Layout recovered from `GalaxyOctree_GenerateBoxel`, the name search code and the live dump:
//! ```text
//!  0 u64 SystemAddress          8 char* name          16 u32 HIP        20 u32 HD
//! 24 char* Gliese designation  32 u32 (unknown)      36 i32 cluster id (-1 = none)
//! 40 u16 mass x256 (Msun)      44 i32 abs magnitude x65536   48 u32 temperature (K)
//! 52 u32 radius x32768 (Rsun)  56 u32 (default 0xF4240000)   60 u16 age (Myr)
//! 62 i16 metallicity index     64 u16 class word (see classes.rs)
//! 66/68/70 u16 position inside the boxel, 1/32 ly     72 ptr extra data
//! 84 u16 (default 5)           86 u16 (0xFFFF)        88 u8 kind (5 = star, 0 = cluster sphere)
//! ```

use crate::classes::StarClass;
use crate::id64::SystemAddress;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct RawRecord {
    addr: u64,
    name: Option<String>,
    gl: Option<String>,
    raw: String,
}

#[derive(Debug, Clone)]
pub struct Authored {
    pub address: SystemAddress,
    pub name: Option<String>,
    pub gliese: Option<String>,
    pub hip: u32,
    pub hd: u32,
    pub field32: u32,
    pub cluster_id: i32,
    pub mass_x256: u16,
    pub abs_mag_x65536: i32,
    pub temperature_k: u32,
    pub radius_x32768: u32,
    pub age_myr: u16,
    pub metallicity: i16,
    pub class_bits: u16,
    pub pos_1_32: [u16; 3],
    pub kind: u8,
    /// Cached in-game position (ly).
    pub pos: [f32; 3],
}

impl Authored {
    pub fn mass_solar(&self) -> f64 {
        self.mass_x256 as f64 / 256.0
    }
    pub fn radius_solar(&self) -> f64 {
        self.radius_x32768 as f64 / 32768.0
    }
    pub fn abs_magnitude(&self) -> f64 {
        self.abs_mag_x65536 as f64 / 65536.0
    }
    pub fn class(&self) -> StarClass {
        StarClass::from_bits(self.class_bits)
    }
    pub fn position(&self) -> [f64; 3] {
        [self.pos[0] as f64, self.pos[1] as f64, self.pos[2] as f64]
    }
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }
}

fn hex_bytes(s: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(s.len().is_multiple_of(2), "odd hex length");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}

fn le_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn le_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl Authored {
    fn from_raw(r: RawRecord) -> Result<Self> {
        let b = hex_bytes(&r.raw)?;
        anyhow::ensure!(b.len() >= 96, "record shorter than 96 bytes");
        let address = SystemAddress(r.addr);
        let pos_1_32 = [le_u16(&b, 66), le_u16(&b, 68), le_u16(&b, 70)];
        let o = address.boxel().origin_ly();
        let pos = [
            (o[0] + pos_1_32[0] as f64 / 32.0) as f32,
            (o[1] + pos_1_32[1] as f64 / 32.0) as f32,
            (o[2] + pos_1_32[2] as f64 / 32.0) as f32,
        ];
        Ok(Authored {
            address,
            name: r.name.filter(|s| !s.is_empty()),
            gliese: r.gl.filter(|s| !s.is_empty()),
            hip: le_u32(&b, 16),
            hd: le_u32(&b, 20),
            field32: le_u32(&b, 32),
            cluster_id: le_u32(&b, 36) as i32,
            mass_x256: le_u16(&b, 40),
            abs_mag_x65536: le_u32(&b, 44) as i32,
            temperature_k: le_u32(&b, 48),
            radius_x32768: le_u32(&b, 52),
            age_myr: le_u16(&b, 60),
            metallicity: le_u16(&b, 62) as i16,
            class_bits: le_u16(&b, 64),
            pos_1_32,
            kind: b[88],
            pos,
        })
    }
}

/// Load `authored_systems.json.gz`.
pub fn load(path: &Path) -> Result<Vec<Authored>> {
    let f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(f));
    let raw: Vec<RawRecord> = serde_json::from_reader(std::io::BufReader::new(gz))
        .with_context(|| format!("parse {}", path.display()))?;
    raw.into_iter().map(Authored::from_raw).collect()
}
