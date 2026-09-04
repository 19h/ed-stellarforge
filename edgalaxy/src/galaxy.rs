//! Aggregate of all loaded data with the queries the CLI needs.

use crate::density::Density;
use crate::id64::{Boxel, SystemAddress, MASS_CODE_LETTERS};
use crate::names::NameTables;
use crate::records::Authored;
use crate::regions::Regions;
use crate::spatial::GridIndex;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct Galaxy {
    pub data_dir: PathBuf,
    pub tables: NameTables,
    pub regions: Regions,
    pub systems: Vec<Authored>,
    pub index: GridIndex,
    by_name: HashMap<String, Vec<u32>>,
    by_address: HashMap<u64, u32>,
    /// Renamed procedural systems (game "Overrides" resource): id64 -> display name.
    pub overrides: HashMap<u64, String>,
    density: OnceLock<Option<Density>>,
}

#[derive(serde::Deserialize)]
struct OverrideFile {
    records: Vec<OverrideRec>,
}
#[derive(serde::Deserialize)]
struct OverrideRec {
    addr: String,
    #[serde(default)]
    flags: u32,
    #[serde(default)]
    name: Option<String>,
}

fn load_overrides(path: &Path) -> Result<HashMap<u64, String>> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let f: OverrideFile = serde_json::from_slice(&std::fs::read(path)?).with_context(|| format!("parse {}", path.display()))?;
    Ok(f.records
        .into_iter()
        .filter(|r| r.flags & 0x8000 != 0)
        .filter_map(|r| Some((r.addr.parse::<u64>().ok()?, r.name?)))
        .collect())
}

/// Locate the data directory: explicit, `$EDGALAXY_DATA`, or a parent of the executable /
/// current directory containing `galaxy_name_tables.json`.
pub fn find_data_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("EDGALAXY_DATA") {
        return Ok(PathBuf::from(p));
    }
    let mut starts = vec![std::env::current_dir()?];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            starts.push(d.to_path_buf());
        }
    }
    for s in starts {
        let mut cur: Option<&Path> = Some(&s);
        while let Some(d) = cur {
            if d.join("galaxy_name_tables.json").is_file() {
                return Ok(d.to_path_buf());
            }
            cur = d.parent();
        }
    }
    bail!("data directory not found; pass --data-dir or set EDGALAXY_DATA")
}

impl Galaxy {
    pub fn load(data_dir: &Path) -> Result<Self> {
        let loaded = crate::names::load(&data_dir.join("galaxy_name_tables.json"))?;
        let systems = crate::records::load(&data_dir.join("authored_systems.json.gz"))
            .context("loading authored systems")?;
        let pts: Vec<[f32; 3]> = systems.iter().map(|s| s.pos).collect();
        let index = GridIndex::build(pts, 128.0);
        let mut by_name: HashMap<String, Vec<u32>> = HashMap::with_capacity(systems.len());
        let mut by_address = HashMap::with_capacity(systems.len());
        for (i, s) in systems.iter().enumerate() {
            if let Some(n) = &s.name {
                by_name.entry(n.to_ascii_lowercase()).or_default().push(i as u32);
            }
            by_address.insert(s.address.0, i as u32);
        }
        let overrides = load_overrides(&data_dir.join("overrides.json"))?;
        Ok(Galaxy {
            overrides,
            data_dir: data_dir.to_path_buf(),
            tables: loaded.tables,
            regions: loaded.regions,
            systems,
            index,
            by_name,
            by_address,
            density: OnceLock::new(),
        })
    }

    pub fn density(&self) -> Option<&Density> {
        self.density
            .get_or_init(|| {
                let dir = self.data_dir.join("density");
                match Density::load(&dir) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        eprintln!("warning: density tables unavailable ({e:#})");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// Authored record with exactly this name (case-insensitive).
    pub fn by_name(&self, name: &str) -> Vec<&Authored> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .map(|v| v.iter().map(|&i| &self.systems[i as usize]).collect())
            .unwrap_or_default()
    }

    pub fn by_address(&self, a: SystemAddress) -> Option<&Authored> {
        self.by_address.get(&a.0).map(|&i| &self.systems[i as usize])
    }

    /// The name the game shows: override name (renamed procedural system) if any, else the
    /// authored name if the address is authored, else the procedural name (region-aware when
    /// a position is known). Mirrors the order in `GalaxyNames_SystemAddressToName`.
    pub fn name_of(&self, a: SystemAddress, pos: Option<[f64; 3]>) -> String {
        if let Some(n) = self.overrides.get(&a.0) {
            return n.clone();
        }
        if let Some(s) = self.by_address(a) {
            if let Some(n) = &s.name {
                return n.clone();
            }
        }
        let pos = pos.or_else(|| self.by_address(a).map(|s| s.position()));
        self.tables.name(a, pos, &self.regions)
    }

    /// Procedural name a record would carry if it were not hand-named.
    pub fn procedural_name_for(&self, s: &Authored) -> String {
        self.tables.name(s.address, Some(s.position()), &self.regions)
    }

    /// Resolve "Sol", "0x27000...", "10477373803" or a procedural name (via search) to a position.
    pub fn resolve_position(&self, what: &str) -> Result<([f64; 3], String)> {
        if let Some(a) = crate::id64::parse_address(what) {
            if let Some(s) = self.by_address(a) {
                return Ok((s.position(), s.display_name().to_string()));
            }
            let b = a.boxel();
            return Ok((b.center_ly(), self.tables.procedural_name(a)));
        }
        let hits = self.by_name(what);
        if let Some(s) = hits.first() {
            return Ok((s.position(), s.display_name().to_string()));
        }
        bail!("unknown system '{what}' (authored names and id64 values are accepted)")
    }

    /// All 1280-ly sectors whose procedural name equals `name` (case-insensitive). Scans the
    /// full 128 x 64 x 128 sector grid in parallel.
    pub fn find_sectors(&self, name: &str) -> Vec<(u32, String)> {
        use rayon::prelude::*;
        let want = name.trim().to_ascii_lowercase();
        let mut v: Vec<(u32, String)> = (0u32..(1 << 21))
            .into_par_iter()
            .filter_map(|key| {
                let n = self.tables.sector_name(key);
                (n.to_ascii_lowercase() == want).then_some((key, n))
            })
            .collect();
        v.sort_unstable();
        v
    }

    /// Boxels of one mass code intersecting a sphere, with their name prefixes.
    pub fn boxels_in_sphere(&self, c: [f64; 3], r: f64, mc: u8) -> Vec<(Boxel, String)> {
        let size = (10u32 << mc) as f64;
        let lo = |v: f64| ((v - r) / size).floor() as i64;
        let hi = |v: f64| ((v + r) / size).floor() as i64;
        let mut out = Vec::new();
        let g = crate::id64::GALAXY_ORIGIN;
        for x in lo(c[0] + g[0])..=hi(c[0] + g[0]) {
            for y in lo(c[1] + g[1])..=hi(c[1] + g[1]) {
                for z in lo(c[2] + g[2])..=hi(c[2] + g[2]) {
                    if x < 0 || y < 0 || z < 0 || x >= 1 << (14 - mc) || y >= 1 << (13 - mc) || z >= 1 << (14 - mc) {
                        continue;
                    }
                    let b = Boxel { mc, x: x as u32, y: y as u32, z: z as u32 };
                    let o = b.origin_ly();
                    // sphere / cube overlap test
                    let mut d2 = 0.0;
                    for k in 0..3 {
                        let v = c[k].clamp(o[k], o[k] + size);
                        d2 += (v - c[k]).powi(2);
                    }
                    if d2 > r * r {
                        continue;
                    }
                    let center = b.center_ly();
                    let prefix = match self.regions.find(center) {
                        Some(reg) => format!("{} {}", reg.name, crate::id64::letters(b.index_in_region(reg.origin_1_32))),
                        None => format!("{} {}", self.tables.sector_name(b.sector_key()), crate::id64::letters(b.index_in_sector())),
                    };
                    let idx = match self.regions.find(center) {
                        Some(reg) => b.index_in_region(reg.origin_1_32),
                        None => b.index_in_sector(),
                    };
                    let n1 = idx / 17576;
                    let tail = if n1 != 0 { format!(" {}{}-", MASS_CODE_LETTERS[mc as usize], n1) } else { format!(" {}", MASS_CODE_LETTERS[mc as usize]) };
                    out.push((b, format!("{prefix}{tail}")));
                }
            }
        }
        out
    }
}
