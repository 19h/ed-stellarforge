use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use edgalaxy::classes::{record_kind_name, StarClass, STAR_CLASSES};
use edgalaxy::id64::{parse_address, Boxel, SystemAddress, MASS_CODE_LETTERS};
use edgalaxy::records::Authored;
use edgalaxy::Galaxy;
use rayon::prelude::*;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "edgalaxy", version, about = "Elite Dangerous StellarForge data explorer", long_about = None)]
struct Cli {
    /// Directory holding galaxy_name_tables.json, authored_systems.json.gz and density/
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Name one or more SystemAddress (id64) values as the game would
    Name {
        /// id64 values (decimal or 0x-hex)
        #[arg(required = true)]
        id64: Vec<String>,
        /// In-game position of the star (x y z ly); needed to resolve "... Sector" regions
        #[arg(long, num_args = 3, value_names = ["X", "Y", "Z"], allow_negative_numbers = true)]
        at: Option<Vec<f64>>,
    },
    /// Decode an id64 into mass code, boxel, sector and coordinates
    Decode { id64: String },
    /// List authored systems (all, or inside a sphere / box) with their properties
    List {
        #[command(flatten)]
        area: Area,
        #[command(flatten)]
        filter: Filter,
        #[command(flatten)]
        out: OutOpts,
    },
    /// List authored systems around a named system or id64
    Near {
        /// Authored system name (e.g. "Sol") or id64
        center: String,
        /// Radius in ly
        #[arg(short, long, default_value_t = 50.0)]
        radius: f64,
        #[command(flatten)]
        filter: Filter,
        #[command(flatten)]
        out: OutOpts,
    },
    /// Find authored systems by (sub)string of name, Gliese designation, HIP or HD number
    Find {
        pattern: String,
        #[arg(long)]
        exact: bool,
        #[command(flatten)]
        out: OutOpts,
    },
    /// Enumerate procedural boxels (name prefixes) of one mass code inside a sphere
    Boxels {
        #[arg(long, num_args = 4, value_names = ["X", "Y", "Z", "R"], required = true, allow_negative_numbers = true)]
        sphere: Vec<f64>,
        /// Mass code letter a..h
        #[arg(short, long, default_value = "d")]
        masscode: char,
        #[command(flatten)]
        out: OutOpts,
    },
    /// List hand-authored regions ("... Sector"), optionally those touching a sphere
    Regions {
        #[arg(long, num_args = 4, value_names = ["X", "Y", "Z", "R"], allow_negative_numbers = true)]
        sphere: Option<Vec<f64>>,
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        out: OutOpts,
    },
    /// Galaxy density model at a position: per-mass-code boxel mass budget and population weights
    Density {
        #[arg(num_args = 3, value_names = ["X", "Y", "Z"], required = true, allow_negative_numbers = true)]
        pos: Vec<f64>,
        #[command(flatten)]
        out: OutOpts,
    },
    /// Reverse lookup: which 1280-ly sectors carry a given procedural sector name
    Sector {
        /// Procedural sector name, e.g. "Wregoe" or "Eol Prou" (case-insensitive)
        name: String,
        #[command(flatten)]
        out: OutOpts,
    },
    /// List renamed procedural systems (Colonia etc.) with their procedural names and positions
    Overrides {
        #[command(flatten)]
        out: OutOpts,
    },
    /// Summary statistics over the authored systems
    Stats,
    /// Check the naming algorithm against Ardent Insight `nearby` JSON dumps
    Verify {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

#[derive(Args, Clone)]
struct Area {
    /// Sphere: centre x y z (ly) and radius
    #[arg(long, num_args = 4, value_names = ["X", "Y", "Z", "R"], conflicts_with = "r#box", allow_negative_numbers = true)]
    sphere: Option<Vec<f64>>,
    /// Axis-aligned box: min x y z, max x y z (ly)
    #[arg(long = "box", num_args = 6, value_names = ["X0", "Y0", "Z0", "X1", "Y1", "Z1"], allow_negative_numbers = true)]
    r#box: Option<Vec<f64>>,
}

#[derive(Args, Clone, Default)]
struct Filter {
    /// Spectral class (O, B, A, F, G, K, M, L, T, Y, TTS, AeBe, W*, C*, D*, N, H, ...)
    #[arg(long)]
    class: Option<String>,
    /// Record kind byte (5 = star system, 0 = cluster sphere)
    #[arg(long)]
    kind: Option<u8>,
    /// Case-insensitive substring of the authored name
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    min_mass: Option<f64>,
    #[arg(long)]
    max_mass: Option<f64>,
    /// Only records with a HIP / HD / Gliese catalogue id
    #[arg(long)]
    catalogued: bool,
}

#[derive(Args, Clone)]
struct OutOpts {
    #[arg(short, long, value_enum, default_value_t = Format::Table)]
    format: Format,
    /// Maximum rows (0 = unlimited)
    #[arg(short, long, default_value_t = 0)]
    limit: usize,
    #[arg(long, value_enum, default_value_t = Sort::Distance)]
    sort: Sort,
    /// Also print the procedural name each authored system would otherwise have
    #[arg(long)]
    procedural: bool,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum Format {
    Table,
    Json,
    Csv,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum Sort {
    Distance,
    Name,
    Mass,
    Address,
}

#[derive(Serialize)]
struct Row<'a> {
    name: &'a str,
    id64: u64,
    class: String,
    class_bits: u16,
    mass_solar: f64,
    radius_solar: f64,
    temperature_k: u32,
    abs_magnitude: f64,
    age_myr: u16,
    metallicity: i16,
    x: f64,
    y: f64,
    z: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    distance_ly: Option<f64>,
    kind: &'static str,
    hip: u32,
    hd: u32,
    gliese: &'a str,
    cluster_id: i32,
    mass_code: char,
    sector_x: u32,
    sector_y: u32,
    sector_z: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    procedural_name: Option<String>,
}

fn row<'a>(g: &Galaxy, s: &'a Authored, dist: Option<f64>, procedural: bool) -> Row<'a> {
    let b = s.address.boxel();
    let (sx, sy, sz) = b.sector();
    let p = s.position();
    Row {
        name: s.display_name(),
        id64: s.address.0,
        class: s.class().label(),
        class_bits: s.class_bits,
        mass_solar: s.mass_solar(),
        radius_solar: s.radius_solar(),
        temperature_k: s.temperature_k,
        abs_magnitude: s.abs_magnitude(),
        age_myr: s.age_myr,
        metallicity: s.metallicity,
        x: p[0],
        y: p[1],
        z: p[2],
        distance_ly: dist,
        kind: record_kind_name(s.kind),
        hip: s.hip,
        hd: s.hd,
        gliese: s.gliese.as_deref().unwrap_or(""),
        cluster_id: s.cluster_id,
        mass_code: MASS_CODE_LETTERS[b.mc as usize],
        sector_x: sx,
        sector_y: sy,
        sector_z: sz,
        procedural_name: procedural.then(|| g.procedural_name_for(s)),
    }
}

fn matches(f: &Filter, s: &Authored, class_idx: Option<u8>) -> bool {
    if let Some(c) = class_idx {
        if s.class().class != c {
            return false;
        }
    }
    if let Some(k) = f.kind {
        if s.kind != k {
            return false;
        }
    }
    if let Some(n) = &f.name {
        let hay = s.display_name().to_ascii_lowercase();
        if !hay.contains(&n.to_ascii_lowercase()) {
            return false;
        }
    }
    if let Some(m) = f.min_mass {
        if s.mass_solar() < m {
            return false;
        }
    }
    if let Some(m) = f.max_mass {
        if s.mass_solar() > m {
            return false;
        }
    }
    if f.catalogued && s.hip == 0 && s.hd == 0 && s.gliese.is_none() {
        return false;
    }
    true
}

fn class_index(f: &Filter) -> Result<Option<u8>> {
    match &f.class {
        None => Ok(None),
        Some(c) => StarClass::index_of(c)
            .map(Some)
            .with_context(|| format!("unknown class '{}'; known: {}", c, STAR_CLASSES.join(", "))),
    }
}

fn emit_rows(g: &Galaxy, mut items: Vec<(&Authored, Option<f64>)>, out: &OutOpts) -> Result<()> {
    match out.sort {
        Sort::Distance => items.sort_by(|a, b| a.1.unwrap_or(0.0).total_cmp(&b.1.unwrap_or(0.0))),
        Sort::Name => items.sort_by(|a, b| a.0.display_name().cmp(b.0.display_name())),
        Sort::Mass => items.sort_by_key(|a| std::cmp::Reverse(a.0.mass_x256)),
        Sort::Address => items.sort_by_key(|a| a.0.address.0),
    }
    if out.limit > 0 {
        items.truncate(out.limit);
    }
    let rows: Vec<Row> = items.par_iter().map(|(s, d)| row(g, s, *d, out.procedural)).collect();
    let stdout = std::io::stdout();
    let mut w = std::io::BufWriter::new(stdout.lock());
    match out.format {
        Format::Json => {
            serde_json::to_writer(&mut w, &rows)?;
            writeln!(w)?;
        }
        Format::Csv => {
            writeln!(w, "name,id64,class,mass_solar,radius_solar,temperature_k,abs_magnitude,age_myr,metallicity,x,y,z,distance_ly,kind,hip,hd,gliese,cluster_id,procedural_name")?;
            for r in &rows {
                writeln!(
                    w,
                    "{},{},{},{:.4},{:.4},{},{:.3},{},{},{:.5},{:.5},{:.5},{},{},{},{},{},{},{}",
                    csv(r.name),
                    r.id64,
                    r.class,
                    r.mass_solar,
                    r.radius_solar,
                    r.temperature_k,
                    r.abs_magnitude,
                    r.age_myr,
                    r.metallicity,
                    r.x,
                    r.y,
                    r.z,
                    r.distance_ly.map(|d| format!("{d:.3}")).unwrap_or_default(),
                    r.kind,
                    r.hip,
                    r.hd,
                    csv(r.gliese),
                    r.cluster_id,
                    csv(r.procedural_name.as_deref().unwrap_or(""))
                )?;
            }
        }
        Format::Table => {
            let has_dist = rows.iter().any(|r| r.distance_ly.is_some());
            writeln!(
                w,
                "{:<28} {:<16} {:<10} {:>7} {:>7} {:>6} {:>6} {:>6} {:>4} {:>10} {:>10} {:>10}{}{}",
                "name", "id64", "class", "mass", "radius", "T[K]", "Mabs", "age", "met", "x", "y", "z",
                if has_dist { "     dist" } else { "" },
                if out.procedural { "  procedural name" } else { "" }
            )?;
            for r in &rows {
                writeln!(
                    w,
                    "{:<28} {:<16} {:<10} {:>7.3} {:>7.3} {:>6} {:>6.2} {:>6} {:>4} {:>10.3} {:>10.3} {:>10.3}{}{}",
                    trunc(r.name, 28),
                    r.id64,
                    trunc(&r.class, 10),
                    r.mass_solar,
                    r.radius_solar,
                    r.temperature_k,
                    r.abs_magnitude,
                    r.age_myr,
                    r.metallicity,
                    r.x,
                    r.y,
                    r.z,
                    r.distance_ly.map(|d| format!(" {d:>8.2}")).unwrap_or_default(),
                    r.procedural_name.as_deref().map(|p| format!("  {p}")).unwrap_or_default()
                )?;
            }
            writeln!(w, "{} row(s)", rows.len())?;
        }
    }
    Ok(())
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}

fn select<'a>(g: &'a Galaxy, area: &Area, f: &Filter) -> Result<Vec<(&'a Authored, Option<f64>)>> {
    let ci = class_index(f)?;
    let items: Vec<(&Authored, Option<f64>)> = if let Some(s) = &area.sphere {
        let c = [s[0], s[1], s[2]];
        g.index
            .sphere(c, s[3])
            .into_iter()
            .map(|(i, d)| (&g.systems[i as usize], Some(d)))
            .filter(|(s, _)| matches(f, s, ci))
            .collect()
    } else if let Some(b) = &area.r#box {
        let min = [b[0].min(b[3]), b[1].min(b[4]), b[2].min(b[5])];
        let max = [b[0].max(b[3]), b[1].max(b[4]), b[2].max(b[5])];
        g.index
            .aabb(min, max)
            .into_iter()
            .map(|i| (&g.systems[i as usize], None))
            .filter(|(s, _)| matches(f, s, ci))
            .collect()
    } else {
        g.systems.par_iter().filter(|s| matches(f, s, ci)).map(|s| (s, None)).collect()
    };
    Ok(items)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = edgalaxy::galaxy::find_data_dir(cli.data_dir.as_deref())?;
    let t0 = std::time::Instant::now();
    let g = Galaxy::load(&dir)?;
    let load_ms = t0.elapsed().as_millis();

    match cli.cmd {
        Cmd::Name { id64, at } => {
            let pos = at.map(|v| [v[0], v[1], v[2]]);
            for s in id64 {
                let a = parse_address(&s).with_context(|| format!("bad id64 '{s}'"))?;
                println!("{}\t{}", a, g.name_of(a, pos));
            }
        }
        Cmd::Decode { id64 } => {
            let a = parse_address(&id64).context("bad id64")?;
            let b = a.boxel();
            let (sx, sy, sz) = b.sector();
            let o = b.origin_ly();
            println!("id64            {} (0x{:x})", a.0, a.0);
            println!("mass code       {} ({}), boxel edge {} ly", b.mc, MASS_CODE_LETTERS[b.mc as usize], b.size_ly());
            println!("boxel index     x={} y={} z={}   n2={}", b.x, b.y, b.z, a.n2());
            println!("boxel origin    ({:.1}, {:.1}, {:.1}) ly (in-game)", o[0], o[1], o[2]);
            println!("sector          x={sx} y={sy} z={sz}  key=0x{:x}", b.sector_key());
            println!("sector name     {}", g.tables.sector_name(b.sector_key()));
            println!("boxel code      {}", edgalaxy::id64::boxel_body(b.mc, b.index_in_sector(), a.n2()));
            println!("procedural name {}", g.tables.procedural_name(a));
            if let Some(r) = g.regions.find(b.center_ly()) {
                println!("region (boxel centre) {} -> {}", r.name, g.tables.name(a, Some(b.center_ly()), &g.regions));
            }
            if let Some(n) = g.overrides.get(&a.0) {
                println!("override name  {n} (renamed procedural system)");
            }
            if let Some(s) = g.by_address(a) {
                let p = s.position();
                println!("authored        {} at ({:.4}, {:.4}, {:.4}) class {} mass {:.3} Msun", s.display_name(), p[0], p[1], p[2], s.class().label(), s.mass_solar());
            }
        }
        Cmd::List { area, filter, out } => {
            let items = select(&g, &area, &filter)?;
            emit_rows(&g, items, &out)?;
        }
        Cmd::Near { center, radius, filter, out } => {
            let (c, label) = g.resolve_position(&center)?;
            eprintln!("centre {label} at ({:.3}, {:.3}, {:.3}), radius {radius} ly", c[0], c[1], c[2]);
            let area = Area { sphere: Some(vec![c[0], c[1], c[2], radius]), r#box: None };
            let items = select(&g, &area, &filter)?;
            emit_rows(&g, items, &out)?;
        }
        Cmd::Find { pattern, exact, out } => {
            let p = pattern.to_ascii_lowercase();
            let num: Option<u32> = pattern.parse().ok();
            let items: Vec<(&Authored, Option<f64>)> = g
                .systems
                .par_iter()
                .filter(|s| {
                    let n = s.display_name().to_ascii_lowercase();
                    let gl = s.gliese.as_deref().map(|x| x.to_ascii_lowercase()).unwrap_or_default();
                    if exact {
                        n == p || gl == p
                    } else {
                        n.contains(&p) || gl.contains(&p) || num.is_some_and(|v| v != 0 && (s.hip == v || s.hd == v))
                    }
                })
                .map(|s| (s, None))
                .collect();
            emit_rows(&g, items, &out)?;
        }
        Cmd::Boxels { sphere, masscode, out } => {
            let mc = MASS_CODE_LETTERS
                .iter()
                .position(|&c| c == masscode.to_ascii_lowercase())
                .context("mass code must be a..h")? as u8;
            let c = [sphere[0], sphere[1], sphere[2]];
            let list = g.boxels_in_sphere(c, sphere[3], mc);
            #[derive(Serialize)]
            struct BRow {
                name_prefix: String,
                mass_code: char,
                bx: u32,
                by: u32,
                bz: u32,
                origin_x: f64,
                origin_y: f64,
                origin_z: f64,
                size_ly: f64,
                base_id64: u64,
                max_systems: u64,
                authored_count: usize,
            }
            let rows: Vec<BRow> = list
                .iter()
                .map(|(b, name)| {
                    let o = b.origin_ly();
                    let authored = g
                        .index
                        .aabb(o, [o[0] + b.size_ly(), o[1] + b.size_ly(), o[2] + b.size_ly()])
                        .into_iter()
                        .filter(|&i| g.systems[i as usize].address.boxel() == *b)
                        .count();
                    BRow {
                        name_prefix: name.clone(),
                        mass_code: MASS_CODE_LETTERS[b.mc as usize],
                        bx: b.x,
                        by: b.y,
                        bz: b.z,
                        origin_x: o[0],
                        origin_y: o[1],
                        origin_z: o[2],
                        size_ly: b.size_ly(),
                        base_id64: b.address(0).0,
                        max_systems: 1u64 << (3 * b.mc as u32 + 11),
                        authored_count: authored,
                    }
                })
                .collect();
            let rows: Vec<BRow> = if out.limit > 0 { rows.into_iter().take(out.limit).collect() } else { rows };
            match out.format {
                Format::Json => println!("{}", serde_json::to_string(&rows)?),
                Format::Csv => {
                    println!("name_prefix,mass_code,bx,by,bz,origin_x,origin_y,origin_z,size_ly,base_id64,max_systems,authored_count");
                    for r in &rows {
                        println!("{},{},{},{},{},{},{},{},{},{},{},{}", csv(&r.name_prefix), r.mass_code, r.bx, r.by, r.bz, r.origin_x, r.origin_y, r.origin_z, r.size_ly, r.base_id64, r.max_systems, r.authored_count);
                    }
                }
                Format::Table => {
                    println!("{:<34} {:>9} {:>9} {:>9} {:>6} {:>16} {:>9} {:>8}", "name prefix", "origin x", "origin y", "origin z", "size", "base id64", "max n2", "authored");
                    for r in &rows {
                        println!("{:<34} {:>9.1} {:>9.1} {:>9.1} {:>6} {:>16} {:>9} {:>8}", r.name_prefix, r.origin_x, r.origin_y, r.origin_z, r.size_ly, r.base_id64, r.max_systems, r.authored_count);
                    }
                    println!("{} boxel(s)", rows.len());
                }
            }
        }
        Cmd::Regions { sphere, name, out } => {
            let mut regs: Vec<&edgalaxy::regions::Region> = match (&sphere, &name) {
                (Some(s), _) => g.regions.intersecting([s[0], s[1], s[2]], s[3]),
                (None, Some(n)) => g.regions.by_name(n),
                (None, None) => g.regions.list.iter().collect(),
            };
            if let Some(n) = &name {
                let n = n.to_ascii_lowercase();
                regs.retain(|r| r.name.to_ascii_lowercase().contains(&n));
            }
            regs.sort_by(|a, b| a.name.cmp(&b.name));
            #[derive(Serialize)]
            struct RRow<'a> {
                name: &'a str,
                center_x: f64,
                center_y: f64,
                center_z: f64,
                radius_ly: f64,
                origin_x: f64,
                origin_y: f64,
                origin_z: f64,
            }
            let rows: Vec<RRow> = regs
                .iter()
                .map(|r| {
                    let c = r.center_ingame();
                    let o = r.origin_ingame();
                    RRow { name: &r.name, center_x: c[0], center_y: c[1], center_z: c[2], radius_ly: r.radius, origin_x: o[0], origin_y: o[1], origin_z: o[2] }
                })
                .collect();
            match out.format {
                Format::Json => println!("{}", serde_json::to_string(&rows)?),
                Format::Csv => {
                    println!("name,center_x,center_y,center_z,radius_ly,origin_x,origin_y,origin_z");
                    for r in &rows {
                        println!("{},{},{},{},{},{},{},{}", csv(r.name), r.center_x, r.center_y, r.center_z, r.radius_ly, r.origin_x, r.origin_y, r.origin_z);
                    }
                }
                Format::Table => {
                    println!("{:<34} {:>10} {:>10} {:>10} {:>8}  boxel-grid origin", "region", "centre x", "centre y", "centre z", "radius");
                    for r in &rows {
                        println!("{:<34} {:>10.1} {:>10.1} {:>10.1} {:>8.1}  ({:.2}, {:.2}, {:.2})", trunc(r.name, 34), r.center_x, r.center_y, r.center_z, r.radius_ly, r.origin_x, r.origin_y, r.origin_z);
                    }
                    println!("{} region(s)", rows.len());
                }
            }
        }
        Cmd::Density { pos, out } => {
            let d = g.density().context("density tables not found under <data-dir>/density")?;
            let p = [pos[0], pos[1], pos[2]];
            #[derive(Serialize)]
            struct DRow {
                mass_code: char,
                boxel_origin: [f64; 3],
                size_ly: f64,
                mass_budget_raw: u64,
                mass_budget_units: f64,
                cyl_factor: f64,
                population_weights: Option<[f64; 3]>,
            }
            let mut rows = Vec::new();
            for mc in 0..8u8 {
                let Some(b) = Boxel::containing(p, mc) else { bail!("position outside the galaxy volume") };
                let m = d.boxel_mass(&b);
                rows.push(DRow {
                    mass_code: MASS_CODE_LETTERS[mc as usize],
                    boxel_origin: b.origin_ly(),
                    size_ly: b.size_ly(),
                    mass_budget_raw: m,
                    mass_budget_units: m as f64 / 65536.0,
                    cyl_factor: d.cyl_factor(&b) as f64 / 32768.0,
                    population_weights: d.population_weights(&b).map(|w| [w[0] as f64 / 32768.0, w[1] as f64 / 32768.0, w[2] as f64 / 32768.0]),
                });
            }
            match out.format {
                Format::Json => println!("{}", serde_json::to_string(&rows)?),
                _ => {
                    println!("density model at ({:.1}, {:.1}, {:.1}); mips {:?}", p[0], p[1], p[2], d.mip_dims());
                    println!("{:<3} {:>7} {:>14} {:>12} {:>8}  population weights (0,1,2)", "mc", "size", "budget raw", "budget/65536", "cyl");
                    for r in &rows {
                        println!(
                            "{:<3} {:>7} {:>14} {:>12.3} {:>8.4}  {}",
                            r.mass_code,
                            r.size_ly,
                            r.mass_budget_raw,
                            r.mass_budget_units,
                            r.cyl_factor,
                            r.population_weights.map(|w| format!("{:.3} {:.3} {:.3}", w[0], w[1], w[2])).unwrap_or_else(|| "equal".into())
                        );
                    }
                }
            }
        }
        Cmd::Sector { name, out } => {
            let hits = g.find_sectors(&name);
            #[derive(Serialize)]
            struct SRow {
                name: String,
                key: u32,
                sector_x: u32,
                sector_y: u32,
                sector_z: u32,
                origin: [f64; 3],
                center: [f64; 3],
                authored_count: usize,
            }
            let rows: Vec<SRow> = hits
                .into_iter()
                .map(|(key, n)| {
                    let (xs, ys, zs) = (key & 0x7F, (key >> 7) & 0x3F, (key >> 14) & 0x7F);
                    let b = Boxel { mc: 7, x: xs, y: ys, z: zs };
                    let o = b.origin_ly();
                    let authored = g.index.aabb(o, [o[0] + 1280.0, o[1] + 1280.0, o[2] + 1280.0]).len();
                    SRow { name: n, key, sector_x: xs, sector_y: ys, sector_z: zs, origin: o, center: b.center_ly(), authored_count: authored }
                })
                .collect();
            match out.format {
                Format::Json => println!("{}", serde_json::to_string(&rows)?),
                _ => {
                    for r in &rows {
                        println!("{}  sector ({}, {}, {}) key 0x{:x}  origin ({:.0}, {:.0}, {:.0}) ly  centre ({:.0}, {:.0}, {:.0})  authored systems inside: {}", r.name, r.sector_x, r.sector_y, r.sector_z, r.key, r.origin[0], r.origin[1], r.origin[2], r.center[0], r.center[1], r.center[2], r.authored_count);
                    }
                    println!("{} sector(s)", rows.len());
                }
            }
        }
        Cmd::Overrides { out } => {
            #[derive(Serialize)]
            struct ORow<'a> {
                name: &'a str,
                id64: u64,
                procedural_name: String,
                boxel_origin: [f64; 3],
                size_ly: f64,
            }
            let mut rows: Vec<ORow> = g
                .overrides
                .iter()
                .map(|(a, n)| {
                    let addr = SystemAddress(*a);
                    let b = addr.boxel();
                    ORow { name: n, id64: *a, procedural_name: g.tables.name(addr, Some(b.center_ly()), &g.regions), boxel_origin: b.origin_ly(), size_ly: b.size_ly() }
                })
                .collect();
            rows.sort_by(|a, b| a.name.cmp(b.name));
            match out.format {
                Format::Json => println!("{}", serde_json::to_string(&rows)?),
                _ => {
                    for r in &rows {
                        println!("{:<30} {:<16} {:<28} boxel ({:.0}, {:.0}, {:.0}) +{}", r.name, r.id64, r.procedural_name, r.boxel_origin[0], r.boxel_origin[1], r.boxel_origin[2], r.size_ly);
                    }
                    println!("{} override(s)", rows.len());
                }
            }
        }
        Cmd::Stats => {
            let n = g.systems.len();
            let named = g.systems.iter().filter(|s| s.name.is_some()).count();
            let mut by_class = vec![0usize; STAR_CLASSES.len()];
            let mut by_kind = std::collections::BTreeMap::new();
            let mut by_mc = [0usize; 8];
            let (mut hip, mut hd, mut gl) = (0, 0, 0);
            for s in &g.systems {
                by_class[s.class().class as usize] += 1;
                *by_kind.entry(s.kind).or_insert(0usize) += 1;
                by_mc[s.address.mass_code() as usize] += 1;
                hip += (s.hip != 0) as usize;
                hd += (s.hd != 0) as usize;
                gl += s.gliese.is_some() as usize;
            }
            println!("authored records: {n} (named {named}); HIP ids {hip}, HD ids {hd}, Gliese ids {gl}; renamed procedural systems (overrides): {}", g.overrides.len());
            println!("name tables: {} prefixes, {}/{} infixes, {}/{} suffixes; regions: {}", g.tables.prefixes.len(), g.tables.infix_vowel.len(), g.tables.infix_consonant.len(), g.tables.suffix_vowel.len(), g.tables.suffix_consonant.len(), g.regions.list.len());
            println!("by record kind: {:?}", by_kind.iter().map(|(k, v)| format!("{}={}", record_kind_name(*k), v)).collect::<Vec<_>>());
            println!("by mass code:   {}", (0..8).map(|i| format!("{}={}", MASS_CODE_LETTERS[i], by_mc[i])).collect::<Vec<_>>().join(" "));
            println!("by class:");
            for (i, c) in by_class.iter().enumerate() {
                if *c > 0 {
                    println!("  {:<22} {:>7}", STAR_CLASSES[i], c);
                }
            }
            println!("loaded in {load_ms} ms");
        }
        Cmd::Verify { files } => {
            #[derive(serde::Deserialize)]
            struct Sys {
                #[serde(rename = "systemAddress")]
                addr: u64,
                #[serde(rename = "systemName")]
                name: String,
                #[serde(rename = "systemX")]
                x: f64,
                #[serde(rename = "systemY")]
                y: f64,
                #[serde(rename = "systemZ")]
                z: f64,
            }
            let re_proc = |n: &str| -> bool {
                // "<words> AB-C d12-34" or "... h7"
                let parts: Vec<&str> = n.rsplitn(3, ' ').collect();
                parts.len() == 3 && parts[1].len() == 4 && parts[1].as_bytes()[2] == b'-' && parts[0].as_bytes()[0].is_ascii_lowercase()
            };
            let (mut ok, mut total, mut bad_coords) = (0, 0, 0);
            for f in files {
                let rows: Vec<Sys> = serde_json::from_slice(&std::fs::read(&f)?)?;
                let (mut fok, mut ftot) = (0, 0);
                for s in rows.iter().filter(|s| re_proc(&s.name)) {
                    ftot += 1;
                    let a = SystemAddress(s.addr);
                    let got = g.tables.name(a, Some([s.x, s.y, s.z]), &g.regions);
                    if got == s.name {
                        fok += 1;
                    } else {
                        let o = a.boxel().origin_ly();
                        let sz = a.boxel().size_ly();
                        let inside = (0..3).all(|k| [s.x, s.y, s.z][k] >= o[k] && [s.x, s.y, s.z][k] < o[k] + sz);
                        if !inside {
                            bad_coords += 1;
                        }
                        if g.tables.procedural_name(a) != s.name || inside {
                            println!("MISMATCH {}  expected '{}' got '{}' (coords inside boxel: {inside})", f.display(), s.name, got);
                        }
                    }
                }
                println!("{}: {fok}/{ftot} procedural names reproduced", f.display());
                ok += fok;
                total += ftot;
            }
            println!("TOTAL {ok}/{total} ({bad_coords} misses have coordinates outside their own boxel, i.e. bad source data)");
        }
    }
    Ok(())
}
