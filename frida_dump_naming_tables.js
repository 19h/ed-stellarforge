// Read-only dump of StellarForge galaxy naming tables, authored systems and density map.
// No hooks, no writes. Locates the naming-manager sub-object by its vtable pointer.
'use strict';
const OUT = '/tmp/claude-1000/-media-null-ares-Games-star-citizen-drive-c-Program-Files--x86--FrontierFiles/3f541b6b-58d1-4650-85ce-b897377d50af/scratchpad/dump.json';
const IMG = 0x140000000;
const mod = Process.getModuleByName('EliteDangerous64.exe');
function rva(a) { return mod.base.add(a - IMG); }
function hex(buf) { return Array.prototype.map.call(new Uint8Array(buf), b => ('0' + b.toString(16)).slice(-2)).join(''); }
function safeRead(p, n) { try { return p.readByteArray(n); } catch (e) { return null; } }
function readable(p) { try { p.readU8(); return true; } catch (e) { return false; } }
function cstr(p, max) { try { return p.readCString(max || 256); } catch (e) { return null; } }
function isAscii(s) { return s !== null && s.length > 0 && /^[\x20-\x7e]+$/.test(s); }

const result = { base: mod.base.toString(), candidates: [], errors: [] };
// 1. verify build
const exp = { 0x144CA0A80: '40535556574156', 0x143CEE810: '40555356574157' };
result.build_ok = Object.keys(exp).every(a => hex(rva(parseInt(a)).readByteArray(7)) === exp[a]);

// 2. scan for vtable pointer of the naming-manager sub-object (vtable @ 0x14553CA48)
const vt = rva(0x14553CA48);
const pat = Array.from(new Uint8Array(new BigUint64Array([BigInt(vt.toString())]).buffer)).map(b=>('0'+b.toString(16)).slice(-2)).join(' ');
const hits = [];
for (const r of Process.enumerateRanges({protection:'rw-', coalesce:true})) {

  try {
    Memory.scanSync(r.base, r.size, pat).forEach(m => hits.push(m.address));
  } catch (e) { }
}
result.vtable_hits = hits.map(h => h.toString());

function readTable(names, offPtr, offCnt, weighted) {
  const p = names.add(offPtr).readPointer();
  const n = names.add(offCnt).readU32();
  if (n === 0 || n > 20000) throw new Error('bad count ' + n);
  const out = [];
  for (let i = 0; i < n; i++) {
    const e = p.add(16 * i);
    const s = cstr(e.readPointer(), 64);
    const w = e.add(8).readU32();
    out.push(weighted ? [s, w] : s);
  }
  return out;
}

let found = null;
for (const h of hits) {
  try {
    const obj = h;
    const galaxy = obj.add(88).readPointer();
    if (!readable(galaxy)) continue;
    const names = galaxy.add(384).readPointer();
    if (!readable(names)) continue;
    const cnt = names.add(120).readU32();
    if (cnt === 0 || cnt > 20000) continue;
    const first = cstr(names.add(112).readPointer().readPointer(), 32);
    result.first_prefix_dbg = first;
    found = { obj, galaxy, names };
    result.candidates.push({ obj: obj.toString(), galaxy: galaxy.toString(), names: names.toString(), prefix0: first, prefixCount: cnt });
    break;
  } catch (e) { result.errors.push('cand ' + h + ': ' + e); }
}

if (found) {
  const { obj, galaxy, names } = found;
  try {
    result.tables = {
      prefixes: readTable(names, 112, 120, true),
      infix_after_vowel: readTable(names, 128, 136, true),
      infix_after_consonant: readTable(names, 144, 152, true),
      suffix_after_vowel: readTable(names, 160, 168, false),
      suffix_after_consonant: readTable(names, 176, 184, false),
    };
  } catch (e) { result.errors.push('tables: ' + e); }
  try { result.masscode_letters = cstr(obj.add(72).readPointer(), 16); } catch (e) { result.errors.push('mc: ' + e); }
  try { result.namingobj_raw = hex(obj.readByteArray(0x200)); } catch (e) { }
  try { result.galaxy_raw = hex(galaxy.readByteArray(0x200)); } catch (e) { }
  try { result.names_raw = hex(names.readByteArray(0x100)); result.extra192 = (function(){const p=names.add(192).readPointer(), n=names.add(200).readU32(); const o=[]; for(let i=0;i<Math.min(n,40);i++){o.push(hex(p.add(16*i).readByteArray(16)));} return o;})(); } catch (e) { result.errors.push("x192 "+e); }
  // authored systems: names+16 -> 8 x {ptr, u32 nbuckets}; bucket {ptr, u32 count}; 96-byte records
  try {
    const tab = names.add(16).readPointer();
    const recs = [];
    let total = 0;
    for (let mc = 0; mc < 8; mc++) {
      const bptr = tab.add(16 * mc).readPointer();
      const nb = tab.add(16 * mc + 8).readU32();
      for (let b = 0; b < nb && b < 100000; b++) {
        const rp = bptr.add(16 * b).readPointer();
        const rc = bptr.add(16 * b + 8).readU32();
        for (let i = 0; i < rc && i < 100000; i++) {
          const r = rp.add(96 * i);
          total++;
          const raw = hex(r.readByteArray(96));
          let name = null, gl = null;
          try { const np = r.add(8).readPointer(); if (!np.isNull()) name = cstr(np, 64); } catch (e) { }
          try { const gp = r.add(24).readPointer(); if (!gp.isNull()) gl = cstr(gp, 64); } catch (e) { }
          recs.push([raw, name, gl]);
        }
      }
    }
    result.authored_count = total;
    result.authored = recs;
  } catch (e) { result.errors.push('authored: ' + e); }
  // catalog arrays (names+48/+56, +64/+72, +80/+88): pointer arrays to records
  try {
    result.catalog_counts = [names.add(56).readU32(), names.add(72).readU32(), names.add(88).readU32()];
  } catch (e) { }
  // density map object: galaxy+248
  try {
    const dm = galaxy.add(248).readPointer();
    const d = { raw: hex(dm.readByteArray(0x200)), maps: [] };
    for (let mc = 0; mc < 8; mc++) {
      const mp = dm.add(56 + 8 * mc).readPointer();
      const w = mp.readU32(), h = mp.add(4).readU32();
      const e = { mc, ptr: mp.toString(), w, h, head: hex(mp.readByteArray(64)) };
      if (w * h > 0 && w * h <= 300000) e.data = hex(mp.readByteArray(8 + w * h * 4 + 64));
      d.maps.push(e);
    }
    try { const wm = dm.add(312).readPointer(); d.weights = { ptr: wm.toString(), head: hex(wm.readByteArray(0x400)) }; } catch (e) { }
    try { const cy = dm.add(320).readPointer(); d.cyl = { ptr: cy.toString(), head: hex(cy.readByteArray(0x400)) }; } catch (e) { }
    result.density = d;
  } catch (e) { result.errors.push('density: ' + e); }
}

const f = new File(OUT, 'w');
f.write(JSON.stringify(result));
f.close();
send({ done: true, build_ok: result.build_ok, hits: result.vtable_hits.length, found: !!found, errors: result.errors, prefixCount: found ? result.candidates[0].prefixCount : null });
