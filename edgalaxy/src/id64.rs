//! SystemAddress (id64) bit layout and boxel geometry, as implemented by
//! `GalaxyNames_SystemAddressToName` (0x143CEE810) in EliteDangerous64.exe.
//!
//! ```text
//! bits 0-2        mass code mc (0=a .. 7=h); boxel edge = 10 << mc ly
//! bits 3 ..       z boxel index   (14-mc bits)
//! bit  17-mc ..   y boxel index   (13-mc bits)
//! bit  30-2mc ..  x boxel index   (14-mc bits)
//! bit  44-3mc ..  n2 = system number inside the boxel (3mc+11 bits)
//! ```
//! The galaxy frame has its origin 49985/40985/24105 ly below Sol; sectors are 1280 ly
//! (128 "a" boxels), 7 bits in x and z, 6 bits in y.

use std::fmt;

/// In-game (Sol-centred) coordinates of the galaxy-frame origin, negated.
pub const GALAXY_ORIGIN: [f64; 3] = [49985.0, 40985.0, 24105.0];
pub const MASS_CODE_LETTERS: [char; 8] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
pub const SECTOR_LY: f64 = 1280.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SystemAddress(pub u64);

/// A boxel identified by mass code and integer boxel indices (units of the boxel edge).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Boxel {
    pub mc: u8,
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl Boxel {
    pub fn size_ly(&self) -> f64 {
        (10u32 << self.mc) as f64
    }

    /// Coordinates in 10-ly units ("a"-boxel units) of the minimum corner.
    pub fn coords10(&self) -> (u32, u32, u32) {
        (self.x << self.mc, self.y << self.mc, self.z << self.mc)
    }

    /// In-game coordinates of the minimum corner.
    pub fn origin_ly(&self) -> [f64; 3] {
        let (x10, y10, z10) = self.coords10();
        [
            x10 as f64 * 10.0 - GALAXY_ORIGIN[0],
            y10 as f64 * 10.0 - GALAXY_ORIGIN[1],
            z10 as f64 * 10.0 - GALAXY_ORIGIN[2],
        ]
    }

    pub fn center_ly(&self) -> [f64; 3] {
        let o = self.origin_ly();
        let h = self.size_ly() / 2.0;
        [o[0] + h, o[1] + h, o[2] + h]
    }

    /// Sector indices (1280 ly cells).
    pub fn sector(&self) -> (u32, u32, u32) {
        let (x10, y10, z10) = self.coords10();
        ((x10 >> 7) & 0x7F, (y10 >> 7) & 0x3F, (z10 >> 7) & 0x7F)
    }

    /// Key fed to the procedural sector-name generator: `xs | ys<<7 | zs<<14`.
    pub fn sector_key(&self) -> u32 {
        let (xs, ys, zs) = self.sector();
        xs | (ys << 7) | (zs << 14)
    }

    /// Boxel index relative to the enclosing 1280-ly sector (`Boxel_IndexWithinSector`).
    pub fn index_in_sector(&self) -> u32 {
        let m = (128u32 >> self.mc) - 1;
        (self.x & m) | ((self.y & m) << 7) | ((self.z & m) << 14)
    }

    /// Boxel index relative to a hand-authored region origin given in 1/32 ly
    /// (`SystemAddress_FromCoords32` + `Boxel_IndexWithinSector`).
    pub fn index_in_region(&self, origin_1_32: [i32; 3]) -> u32 {
        let snap = !((1i64 << self.mc) - 1);
        let o = |v: i32| ((v as i64) / 320) & snap;
        let (x10, y10, z10) = self.coords10();
        let d = |a: u32, b: i64| (((a as i64) - b).unsigned_abs() >> self.mc) as u32;
        let dx = d(x10, o(origin_1_32[0]));
        let dy = d(y10, o(origin_1_32[1]));
        let dz = d(z10, o(origin_1_32[2]));
        (dx & 0x7F) | ((dy & 0x7F) << 7) | (dz << 14)
    }

    /// Boxel of a given mass code containing an in-game position, if inside the galaxy box.
    pub fn containing(pos: [f64; 3], mc: u8) -> Option<Boxel> {
        let size = (10u32 << mc) as f64;
        let idx = |p: f64, o: f64, limit: u32| -> Option<u32> {
            let g = p + o;
            if g < 0.0 {
                return None;
            }
            let i = (g / size).floor() as u64;
            (i < limit as u64).then_some(i as u32)
        };
        let x = idx(pos[0], GALAXY_ORIGIN[0], 1 << (14 - mc))?;
        let y = idx(pos[1], GALAXY_ORIGIN[1], 1 << (13 - mc))?;
        let z = idx(pos[2], GALAXY_ORIGIN[2], 1 << (14 - mc))?;
        Some(Boxel { mc, x, y, z })
    }

    pub fn address(&self, n2: u64) -> SystemAddress {
        let mc = self.mc as u32;
        let v = (mc as u64)
            | ((self.z as u64) << 3)
            | ((self.y as u64) << (17 - mc))
            | ((self.x as u64) << (30 - 2 * mc))
            | (n2 << (44 - 3 * mc));
        SystemAddress(v)
    }
}

impl SystemAddress {
    pub fn mass_code(&self) -> u8 {
        (self.0 & 7) as u8
    }

    pub fn boxel(&self) -> Boxel {
        let a = self.0;
        let mc = (a & 7) as u32;
        Boxel {
            mc: mc as u8,
            z: ((a >> 3) & ((1u64 << (14 - mc)) - 1)) as u32,
            y: ((a >> (17 - mc)) & ((1u64 << (13 - mc)) - 1)) as u32,
            x: ((a >> (30 - 2 * mc)) & ((1u64 << (14 - mc)) - 1)) as u32,
        }
    }

    /// System number inside the boxel.
    pub fn n2(&self) -> u64 {
        let mc = (self.0 & 7) as u32;
        (self.0 >> (44 - 3 * mc)) & ((1u64 << (3 * mc + 11)) - 1)
    }

    /// Address with n2 stripped (the "coordinate part" the game compares on).
    pub fn coordinate_part(&self) -> u64 {
        let mc = (self.0 & 7) as u32;
        self.0 & ((1u64 << (44 - 3 * mc)) - 1)
    }
}

impl fmt::Display for SystemAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// "AB-C" letters for a boxel index (`Boxel_LettersFromIndex`).
pub fn letters(idx: u32) -> String {
    let l = idx % 17576;
    let c = |v: u32| (b'A' + (v % 26) as u8) as char;
    format!("{}{}-{}", c(l), c(l / 26), c(l / 676))
}

/// "AB-C d12-34" / "AB-C h7" body of a name.
pub fn boxel_body(mc: u8, idx: u32, n2: u64) -> String {
    let n1 = idx / 17576;
    let mcc = MASS_CODE_LETTERS[mc as usize & 7];
    if n1 != 0 {
        format!("{} {}{}-{}", letters(idx), mcc, n1, n2)
    } else {
        format!("{} {}{}", letters(idx), mcc, n2)
    }
}

pub fn parse_address(s: &str) -> Option<SystemAddress> {
    let s = s.trim();
    let v = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()?
    } else {
        s.parse::<u64>().ok()?
    };
    Some(SystemAddress(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colonia_roundtrip() {
        let a = SystemAddress(3238296097059);
        let b = a.boxel();
        assert_eq!(b.mc, 3);
        assert_eq!(b.address(a.n2()), a);
        let o = b.origin_ly();
        assert_eq!(o, [-9585.0, -985.0, 19735.0]);
        assert_eq!(boxel_body(b.mc, b.index_in_sector(), a.n2()), "RS-T d3-94");
    }

    #[test]
    fn sol_boxel() {
        let a = SystemAddress(10477373803);
        assert_eq!(a.boxel().origin_ly(), [-65.0, -25.0, -25.0]);
        assert_eq!(Boxel::containing([0.0, 0.0, 0.0], 3), Some(a.boxel()));
    }
}
