//! End-to-end check against Ardent Insight neighbourhood dumps (requires the data directory).
use edgalaxy::id64::SystemAddress;
use edgalaxy::Galaxy;

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

fn is_procedural(n: &str) -> bool {
    let parts: Vec<&str> = n.rsplitn(3, ' ').collect();
    parts.len() == 3 && parts[1].len() == 4 && parts[1].as_bytes()[2] == b'-' && parts[0].as_bytes()[0].is_ascii_lowercase()
}

#[test]
fn ardent_neighbourhoods() {
    let Ok(dir) = edgalaxy::galaxy::find_data_dir(None) else { eprintln!("data dir not found; skipping"); return };
    let g = Galaxy::load(&dir).expect("load");
    // Colonia is a renamed procedural system (game "Overrides" resource), not an authored record
    assert_eq!(g.name_of(SystemAddress(10477373803), None), "Sol");
    if !g.overrides.is_empty() {
        assert_eq!(g.name_of(SystemAddress(3238296097059), None), "Colonia");
    }
    assert_eq!(g.tables.procedural_name(SystemAddress(3238296097059)), "Eol Prou RS-T d3-94");
    assert_eq!(g.tables.procedural_name(SystemAddress(81973396946)), "Ceeckia ZQ-L c24-0");
    assert_eq!(g.tables.procedural_name(SystemAddress(20578934)), "Stuemeae AA-A g0");
    assert_eq!(g.tables.name(SystemAddress(1797133453651), Some([-230.28125, 133.21875, -257.625]), &g.regions), "Col 285 Sector ZK-X d1-52");
    let (mut ok, mut total) = (0, 0);
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().map_or(true, |e| e != "json") { continue; }
        let rows: Vec<Sys> = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        for s in rows.iter().filter(|s| is_procedural(&s.name)) {
            let a = SystemAddress(s.addr);
            let o = a.boxel().origin_ly();
            let sz = a.boxel().size_ly();
            let inside = (0..3).all(|k| [s.x, s.y, s.z][k] >= o[k] && [s.x, s.y, s.z][k] < o[k] + sz);
            if !inside { continue; } // Ardent placeholder coordinates
            total += 1;
            if g.tables.name(a, Some([s.x, s.y, s.z]), &g.regions) == s.name { ok += 1; }
        }
    }
    assert!(total > 5000, "expected thousands of validation systems, got {total}");
    assert_eq!(ok, total, "all procedural names must be reproduced");
}
