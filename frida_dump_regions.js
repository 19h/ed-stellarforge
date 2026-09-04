const OUT='/tmp/claude-1000/-media-null-ares-Games-star-citizen-drive-c-Program-Files--x86--FrontierFiles/3f541b6b-58d1-4650-85ce-b897377d50af/scratchpad/octree.json';
const mod = Process.getModuleByName('EliteDangerous64.exe');
function rva(a){ return mod.base.add(a - 0x140000000); }
function hex(buf){ return Array.prototype.map.call(new Uint8Array(buf), b=>('0'+b.toString(16)).slice(-2)).join(''); }
function pat(p){ return Array.from(new Uint8Array(new BigUint64Array([BigInt(p.toString())]).buffer)).map(b=>('0'+b.toString(16)).slice(-2)).join(' '); }
function ok(p){ try { p.readU8(); return true; } catch(e){ return false; } }
let hit=null;
for (const r of Process.enumerateRanges({protection:'rw-', coalesce:true})) { try { const m=Memory.scanSync(r.base,r.size,pat(rva(0x14553CA48))); if(m.length){hit=m[0].address;break;} } catch(e){} }
const obj=hit; const root=obj.add(360);
const entries=[]; const nodes=[]; const seen=new Set();
function walk(node, depth){
  if (depth>14 || !ok(node) || seen.has(node.toString())) return; seen.add(node.toString());
  let nd={addr:node+'', depth, raw:hex(node.readByteArray(144))};
  nodes.push(nd);
  // entry list
  let e=node.add(16).readPointer(); let guard=0;
  while(!e.isNull() && ok(e) && guard++<200000){
    const rec={e:e+''};
    try {
      rec.f32 = []; for (let o=32;o<=68;o+=4) rec.f32.push(e.add(o).readFloat());
      rec.raw = hex(e.readByteArray(96));
      const s=e.add(80).readPointer(); rec.struct=s+'';
      if (ok(s)) { rec.sraw=hex(s.readByteArray(64)); const np=s.readPointer(); rec.name = ok(np)? np.readUtf8String() : null; rec.origin=[s.add(24).readS32(), s.add(28).readS32(), s.add(32).readS32()]; }
    } catch(err){ rec.err=''+err; }
    entries.push(rec);
    const nx=e.add(24).readPointer(); if (nx.equals(e)) break; e=nx;
  }
  for (let i=0;i<8;i++){ const c=node.add(24+8*i).readPointer(); if(!c.isNull()) walk(c, depth+1); }
}
walk(root,0);
const f=new File(OUT,'w'); f.write(JSON.stringify({nodes:nodes.length, entries})); f.close();
send({nodes:nodes.length, entries:entries.length, sample: entries.slice(0,3).map(x=>[x.name,x.origin,x.f32])});
