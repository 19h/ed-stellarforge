//! Stellar class enums recovered from the executable (class-name table at 0x145E9AB70,
//! luminosity-class table at 0x145E9AA98). The 16-bit "class word" stored per star is:
//! bits 0-5 class, bits 6-9 subclass (0-9), bits 10-14 luminosity class.

pub const STAR_CLASSES: [&str; 49] = [
    "O", "B", "A", "F", "G", "K", "M", "L", "T", "TTS", "AeBe", "Y", "W", "WN", "WNC", "WC", "WO",
    "CS", "C", "CN", "CJ", "CH", "CHd", "MS", "S", "D", "DA", "DAB", "DAO", "DAZ", "DAV", "DB",
    "DBZ", "DBV", "DO", "DOV", "DQ", "DC", "DCV", "DX", "N", "H", "SupermassiveBlackHole", "X",
    "Nebula", "RemnantNebula", "BigText", "NoSystem", "ERROR",
];

pub const LUMINOSITY_CLASSES: [&str; 26] = [
    "I", "Ia0", "Ia", "Ib", "Iab", "II", "IIa", "IIab", "IIb", "III", "IIIa", "IIIab", "IIIb", "IV",
    "IVa", "IVab", "IVb", "V", "Va", "Vab", "Vb", "Vz", "VI", "VII", "NoSystem", "ERROR",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StarClass {
    pub class: u8,
    pub subclass: u8,
    pub luminosity: u8,
}

impl StarClass {
    pub fn from_bits(bits: u16) -> Self {
        StarClass {
            class: (bits & 0x3F) as u8,
            subclass: ((bits >> 6) & 0xF) as u8,
            luminosity: ((bits >> 10) & 0x1F) as u8,
        }
    }
    pub fn class_name(&self) -> &'static str {
        STAR_CLASSES.get(self.class as usize).copied().unwrap_or("?")
    }
    pub fn luminosity_name(&self) -> &'static str {
        LUMINOSITY_CLASSES.get(self.luminosity as usize).copied().unwrap_or("?")
    }
    /// "G2 Va", "DA", "N", "H" ...
    pub fn label(&self) -> String {
        let c = self.class_name();
        if self.class <= 11 || (17..=24).contains(&self.class) {
            format!("{}{} {}", c, self.subclass, self.luminosity_name())
        } else {
            c.to_string()
        }
    }
    /// Look up a class index by its short name (case-insensitive).
    pub fn index_of(name: &str) -> Option<u8> {
        STAR_CLASSES.iter().position(|c| c.eq_ignore_ascii_case(name)).map(|i| i as u8)
    }
}

/// Meaning of the `type` byte at record offset 88 (observed values).
pub fn record_kind_name(kind: u8) -> &'static str {
    match kind {
        0 => "cluster-sphere",
        1 => "type1",
        2 => "type2",
        3 => "type3",
        4 => "nebula",
        5 => "star-system",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sol_is_g2() {
        let c = StarClass::from_bits(18564);
        assert_eq!(c.label(), "G2 Va");
    }
}
