//! Procedural sector naming (`SectorName_Generate` 0x144CA0A80 and
//! `SectorName_GenerateWord` 0x144CA0FA0) plus hand-authored region overrides.

use crate::id64::{boxel_body, SystemAddress};
use crate::regions::Regions;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct TablesFile {
    masscode_letters: String,
    prefixes: Vec<(String, u32)>,
    infix_after_vowel_initial: Vec<(String, u32)>,
    infix_after_consonant_initial: Vec<(String, u32)>,
    suffix_after_vowel_initial: Vec<String>,
    suffix_after_consonant_initial: Vec<String>,
    regions: Vec<crate::regions::RegionFile>,
}

/// Weighted fragment table: entries carry cumulative weights, exactly like the in-game
/// `{const char*, u32 cum}` arrays.
#[derive(Debug, Clone)]
pub struct Weighted {
    frags: Vec<String>,
    cum: Vec<u32>,
}

impl Weighted {
    fn new(v: Vec<(String, u32)>) -> Self {
        let (frags, cum) = v.into_iter().unzip();
        Weighted { frags, cum }
    }
    fn total(&self) -> u32 {
        *self.cum.last().expect("non-empty table")
    }
    /// upper_bound(cum, rem): first index whose cumulative weight exceeds `rem`.
    fn bucket(&self, rem: u32) -> usize {
        self.cum.partition_point(|&c| c <= rem)
    }
    pub fn len(&self) -> usize {
        self.frags.len()
    }
    pub fn is_empty(&self) -> bool {
        self.frags.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, u32)> {
        self.frags.iter().map(String::as_str).zip(self.cum.iter().copied())
    }
}

#[derive(Debug, Clone)]
pub struct NameTables {
    pub masscodes: Vec<char>,
    pub prefixes: Weighted,
    pub infix_vowel: Weighted,
    pub infix_consonant: Weighted,
    pub suffix_vowel: Vec<String>,
    pub suffix_consonant: Vec<String>,
}

pub struct Loaded {
    pub tables: NameTables,
    pub regions: Regions,
}

pub fn load(path: &Path) -> Result<Loaded> {
    let f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let t: TablesFile = serde_json::from_reader(std::io::BufReader::new(f))
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(Loaded {
        tables: NameTables {
            masscodes: t.masscode_letters.chars().collect(),
            prefixes: Weighted::new(t.prefixes),
            infix_vowel: Weighted::new(t.infix_after_vowel_initial),
            infix_consonant: Weighted::new(t.infix_after_consonant_initial),
            suffix_vowel: t.suffix_after_vowel_initial,
            suffix_consonant: t.suffix_after_consonant_initial,
        },
        regions: Regions::from_file_entries(t.regions),
    })
}

/// Thomas Wang 32-bit integer hash, as compiled.
#[inline]
pub fn wang32(mut k: u32) -> u32 {
    k = k.wrapping_mul(4097);
    k ^= k >> 22;
    k = k.wrapping_mul(17);
    k ^= k >> 9;
    k = k.wrapping_mul(1025);
    k ^= k >> 2;
    k = k.wrapping_mul(129);
    k ^= k >> 12;
    k
}

#[inline]
fn is_vowel(b: u8) -> bool {
    matches!(b, b'A' | b'E' | b'I' | b'O' | b'U' | b'a' | b'e' | b'i' | b'o' | b'u')
}

impl NameTables {
    /// `SectorName_GenerateWord`: mixed-radix decomposition of `value`.
    pub fn word(&self, mut value: u32, out: &mut String) {
        let mut table = &self.prefixes;
        loop {
            let total = table.total();
            let rem = value % total;
            let quot = value / total;
            let i = table.bucket(rem);
            let frag = &table.frags[i];
            let hi = table.cum[i];
            let lo = if i == 0 { 0 } else { table.cum[i - 1] };
            out.push_str(frag);
            // residue re-encoded inside the chosen bucket
            value = rem.wrapping_add(quot.wrapping_mul(hi - lo)).wrapping_sub(lo);
            if is_vowel(frag.as_bytes()[0]) {
                if (value as usize) < self.suffix_vowel.len() {
                    out.push_str(&self.suffix_vowel[value as usize]);
                    return;
                }
                table = &self.infix_vowel;
            } else {
                if (value as usize) < self.suffix_consonant.len() {
                    out.push_str(&self.suffix_consonant[value as usize]);
                    return;
                }
                table = &self.infix_consonant;
            }
        }
    }

    /// `SectorName_Generate`: one or two words from the sector key.
    pub fn sector_name(&self, mut key: u32) -> String {
        let nwords = 1 + (wang32(key) % if key >= 0x4000 { 2 } else { 1 }) as usize;
        let mut words = [0u32; 2];
        let mut pos = [0u32; 2];
        let mut i = 0;
        while key != 0 {
            words[i] |= (key & 1) << pos[i];
            pos[i] += 1;
            key >>= 1;
            i = (i + 1) % nwords;
        }
        let mut out = String::with_capacity(24);
        for (k, w) in words.iter().take(nwords).enumerate() {
            if k > 0 {
                out.push(' ');
            }
            self.word(*w, &mut out);
        }
        out
    }

    /// Name ignoring hand-authored regions.
    pub fn procedural_name(&self, a: SystemAddress) -> String {
        let b = a.boxel();
        format!("{} {}", self.sector_name(b.sector_key()), boxel_body(b.mc, b.index_in_sector(), a.n2()))
    }

    /// Full name; `pos` (in-game ly) is needed to resolve hand-authored regions.
    pub fn name(&self, a: SystemAddress, pos: Option<[f64; 3]>, regions: &Regions) -> String {
        if let Some(p) = pos {
            if let Some(r) = regions.find(p) {
                let b = a.boxel();
                return format!("{} {}", r.name, boxel_body(b.mc, b.index_in_region(r.origin_1_32), a.n2()));
            }
        }
        self.procedural_name(a)
    }
}
