//! Port of rvt_elemmesh.element_mesh (+ _geos_f, _geos, _polymeshes, _inst_parts) over Rust-held channel-103 records, with a symbol-mesh memo,
//! parallel evaluation over elements (rayon) and conversion to the same Python dict as the reference.
use crate::conv::Conv;
use crate::dec::{Dec, Schema, Val};
use crate::mesh::*;
use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct EMesh {
    /// 'no_saved_graphics' result: only tris/source/flags keys
    pub none: bool,
    pub tris: Vec<Tri>,
    pub fid: Vec<i64>,
    pub nid: Vec<i64>,
    pub meta: Vec<Meta>,
    pub source: &'static str,
    pub flags: Vec<String>,
    pub bbox: Option<Val>,
    /// per triangle: index into mkeys, -1 = unassigned (the reference's `mat` = _material_of(doc, *mkeys[i]) or -1)
    pub mk: Vec<i32>,
    /// (face m_renderStyleId, Geometry node m_GInfo.m_categoryId) material keys
    pub mkeys: Vec<(Val, Val)>,
}

/// rvt_elemmesh._sty: graphics style key for duplicate-body merging (built-in styles below 1000 behave like no style)
fn sty(kk: &K, g: &Val) -> i128 {
    match node_style(kk, g).as_int() { Some(v) => if v < 1000 || v >= 1i128 << 63 || v == 0xFFFF_FFFF { -1 } else { v }, None => -1 }
}

/// rvt_elemmesh._node_style
fn node_style(kk: &K, g: &Val) -> Val {
    match dget(g, kk.k("m_GInfo")) { Some(gi) if truthy(gi) => dget(gi, kk.k("m_categoryId")).cloned().unwrap_or(Val::None), _ => Val::None }
}

/// rvt_elemmesh._tri_mats for one tessellation of node g: per-triangle key indices, keys appended to `keys` (one per face that has triangles)
fn tri_mats(kk: &K, t: &Tess, g: &Val, keys: &mut Vec<(Val, Val)>, face_key: &mut FxHashMap<i64, i32>) -> Vec<i32> {
    let ns = node_style(kk, g);
    face_key.clear();
    t.fidx.iter().map(|&f| {
        *face_key.entry(f).or_insert_with(|| {
            let rs = if f >= 1 { t.frs.get((f - 1) as usize).cloned().unwrap_or(Val::None) } else { Val::None };
            // one entry per distinct key (faces of one node mostly share a style)
            if let Some(i) = keys.iter().position(|k| k.0 == rs && k.1 == ns) { return i as i32; }
            keys.push((rs, ns.clone())); (keys.len() - 1) as i32
        })
    }).collect()
}

pub struct Ctx {
    pub s: Arc<Schema>,
    pub kk: K,
    pub index: FxHashMap<u64, (Arc<Vec<u8>>, usize)>,
    pub cls: FxHashMap<u64, String>,
    pub ws: FxHashMap<u64, i64>,
    recs: Mutex<FxHashMap<u64, Arc<Rs<Option<Val>>>>>,
    memo: Mutex<FxHashMap<(u64, i32, i64), Arc<Rs<EMesh>>>>,
}

fn cond_ok(kk: &K, c: &Val, detail: i64) -> bool {
    let c = ob(c);
    if !is_dict(c) { return true; }
    let num = |n: &str| dget(c, kk.k(n)).and_then(|v| fl(v).ok());
    let comp = num("m_comp");
    let param = num("m_param");
    let value = num("m_value");
    let d = detail as f64;
    if comp == Some(3.0) && param == Some(3.0) { return value == Some(d); }
    if comp == Some(0.0) && param == Some(3.0) { return value != Some(d); }
    if comp == Some(6.0) || comp == Some(7.0) { return dget(c, kk.k("m_negateDirCondition")).map_or(false, truthy); }
    true
}

fn ginfo_flags(kk: &K, g: &Val) -> i128 {
    match dget(g, kk.k("m_GInfo")) {
        Some(gi) if truthy(gi) => match dget(gi, kk.k("m_flags")) { Some(f) if truthy(f) => int_of(f).unwrap_or(0), _ => 0 },
        _ => 0,
    }
}

/// rvt_elemmesh._geos_f
fn geos_f<'a>(kk: &K, fields: &'a Val, detail: i64) -> (Vec<&'a Val>, Vec<&'a Val>) {
    let mut out = vec![];
    let mut inst = vec![];
    fn walk<'a>(kk: &K, n: &'a Val, detail: i64, out: &mut Vec<&'a Val>, inst: &mut Vec<&'a Val>) {
        let n = ob(n);
        if !is_dict(n) { return; }
        if let Some(cs) = dget(n, kk.k("m_oConditions")) {
            if truthy(cs) {
                let ok = match cs { Val::L(l) | Val::T(l) => l.iter().all(|c| cond_ok(kk, c, detail)), _ => true };
                if !ok { return; }
            }
        }
        if dget(n, kk.k("m_pFaces")).map_or(false, truthy) { out.push(n); }
        if has(n, kk.k("m_instanceInfo")) { inst.push(n); }
        if let Some(Val::L(l)) = dget(n, kk.k("m_subNodes")) { for k in l { walk(kk, k, detail, out, inst); } }
    }
    walk(kk, fields, detail, &mut out, &mut inst);
    let hid = |g: &Val| ginfo_flags(kk, g) & 0xE0 != 0;
    if out.iter().any(|g| hid(g)) && out.iter().any(|g| !hid(g)) { out.retain(|g| !hid(g)); }
    (out, inst)
}

/// rvt_elemmesh._geos (stack walk)
fn geos_all<'a>(kk: &K, fields: &'a Val) -> (Vec<&'a Val>, Vec<&'a Val>) {
    let mut out = vec![];
    let mut inst = vec![];
    let mut st: Vec<&'a Val> = vec![fields];
    while let Some(z) = st.pop() {
        match z {
            Val::D(d) => {
                if dget(z, kk.k("m_pFaces")).map_or(false, truthy) { out.push(z); }
                if has(z, kk.k("m_instanceInfo")) { inst.push(z); }
                for (_, b) in d { if !matches!(b, Val::None) { st.push(b); } }
            }
            Val::L(l) => { for b in l { st.push(b); } }
            _ => {}
        }
    }
    (out, inst)
}

fn polymeshes<'a>(kk: &K, fields: &'a Val) -> Vec<&'a Val> {
    let mut out = vec![];
    let k = kk.k("m_pFacetedTopology");
    let mut st: Vec<&'a Val> = vec![fields];
    while let Some(z) = st.pop() {
        match z {
            Val::D(d) => {
                if has(z, k) { out.push(z); }
                for (_, b) in d { if !matches!(b, Val::None) { st.push(b); } }
            }
            Val::L(l) => { for b in l { st.push(b); } }
            _ => {}
        }
    }
    out
}

fn bbox_of(bb: &Val) -> Rs<(V3, V3)> {
    let l = list(bb)?;
    if l.len() < 2 { return Err(Raise); }
    Ok((v3(&l[0])?, v3(&l[1])?))
}

fn round3i(p: V3) -> [i64; 3] { [(p[0] * 1e3).round_ties_even() as i64, (p[1] * 1e3).round_ties_even() as i64, (p[2] * 1e3).round_ties_even() as i64] }
fn sig(t: &Tri) -> [[i64; 3]; 3] { let mut s = [round3i(t[0]), round3i(t[1]), round3i(t[2])]; s.sort(); s }

impl Ctx {
    pub fn new(s: Arc<Schema>, index: FxHashMap<u64, (Arc<Vec<u8>>, usize)>, cls: FxHashMap<u64, String>, ws: FxHashMap<u64, i64>) -> Ctx {
        let kk = K::new(&s);
        Ctx { s, kk, index, cls, ws, recs: Mutex::new(FxHashMap::default()), memo: Mutex::new(FxHashMap::default()) }
    }

    fn cls_of(&self, v: Option<&Val>) -> Option<&str> {
        let id = v?.as_int()?;
        if id < 0 { return None; }
        self.cls.get(&(id as u64)).map(|s| s.as_str())
    }

    /// doc.rec(eid)['fields'] (resolved), None when the element has no channel-103 record; Err when get_record raises
    pub fn rec(&self, eid: u64) -> Arc<Rs<Option<Val>>> {
        if let Some(r) = self.recs.lock().unwrap().get(&eid) { return r.clone(); }
        let r = match self.index.get(&eid) {
            None => Ok(None),
            Some((buf, off)) => {
                let mut d = Dec::new(&self.s);
                d.quiet = true;
                match d.record(buf, *off, false) { Ok((r, _)) => Ok(Some(resolve_rec(r))), Err(_) => Err(Raise) }
            }
        };
        let r = Arc::new(r);
        self.recs.lock().unwrap().insert(eid, r.clone());
        r
    }

    pub fn element_mesh(&self, eid: u64, depth: i32, detail: i64, ec: Earcut) -> Arc<Rs<EMesh>> {
        if depth < 2 {
            if let Some(m) = self.memo.lock().unwrap().get(&(eid, depth, detail)) { return m.clone(); }
        }
        let m = Arc::new(self.element_mesh_(eid, depth, detail, ec));
        if depth < 2 { self.memo.lock().unwrap().insert((eid, depth, detail), m.clone()); }
        m
    }

    fn element_mesh_(&self, eid: u64, depth: i32, detail: i64, ec: Earcut) -> Rs<EMesh> {
        let kk = &self.kk;
        let rr = self.rec(eid);
        let fields = match &*rr { Err(_) => return Err(Raise), Ok(None) => return Ok(EMesh { none: true, source: "none", flags: vec!["no_saved_graphics".into()], ..Default::default() }), Ok(Some(f)) => f };
        let bbv = dget(fields, kk.k("m_bBox")).cloned();
        let bb_truthy = bbv.as_ref().map_or(false, truthy);
        let (mut geos, mut inst) = geos_f(kk, fields, detail);
        if geos.is_empty() && inst.is_empty() { let (g, i) = geos_all(kk, fields); geos = g; inst = i; }
        let mut flags: Vec<String> = vec![];
        let mut pm: Vec<Tri> = vec![];
        for z in polymeshes(kk, fields) { pm.extend(polymesh_tris(kk, z)?); }
        let mut own: Vec<Tri> = vec![]; let mut ownm: Vec<Meta> = vec![]; let mut ownf: Vec<i64> = vec![]; let mut ownn: Vec<i64> = vec![];
        let mut un: usize = 0;
        let mut nface: i64 = 0;
        let mut ownp: FxHashMap<i64, bool> = FxHashMap::default();
        let mut mkeys: Vec<(Val, Val)> = vec![];
        let mut fmat: FxHashMap<i64, i32> = FxHashMap::default();
        let mut fk: FxHashMap<i64, i32> = FxHashMap::default();
        let cat_ok = |g: &Val| -> bool {
            match dget(g, kk.k("m_GInfo")).and_then(|gi| dget(gi, kk.k("m_categoryId"))).and_then(|c| c.as_int()) {
                Some(c) => c != 0xFFFF_FFFF && c != 0xFFFF_FFFF_FFFF_FFFF,
                None => false,
            }
        };
        if geos.iter().any(|g| cat_ok(g)) && matches!(self.cls.get(&eid).map(|s| s.as_str()), Some("ModelText") | Some("Text3dElem")) { geos.retain(|g| cat_ok(g)); }
        if bb_truthy {
            let bb = bbox_of(bbv.as_ref().unwrap())?;
            for (gi, g) in geos.iter().enumerate() {
                let t = faces_in_bbox(kk, g, bb, 0.02, ec)?;
                let n = t.tris.len();
                let tm = tri_mats(kk, &t, g, &mut mkeys, &mut fk);
                for (f, m) in t.fidx.iter().zip(tm) { fmat.insert(nface + f, m); }
                own.extend(t.tris);
                ownm.extend(t.meta);
                ownf.extend(t.fidx.iter().map(|f| nface + f));
                ownn.extend(std::iter::repeat(gi as i64).take(n));
                for (k, v) in &t.fprio { ownp.insert(nface + k, *v); }
                nface += t.faces + 1;
                un += t.unsup.len();
            }
        }
        if !own.is_empty() || !pm.is_empty() {
            if !pm.is_empty() { flags.push("polymesh".into()); }
            if !own.is_empty() && ownf.len() == own.len() {
                let mut groups: Vec<i64> = ownn.clone();
                groups.sort();
                groups.dedup();
                let mut per: Vec<(Vec<Tri>, Vec<Meta>, Vec<i64>, i64)> = vec![];
                if groups.len() > 1 {
                    for g in &groups {
                        let ix: Vec<usize> = (0..ownn.len()).filter(|&i| ownn[i] == *g).collect();
                        let t: Vec<Tri> = ix.iter().map(|&i| own[i]).collect();
                        let m: Vec<Meta> = ix.iter().map(|&i| ownm[i].clone()).collect();
                        let f: Vec<i64> = ix.iter().map(|&i| ownf[i]).collect();
                        let (st, sm, ok, lf) = select_closed_shell(&t, &m, &f, Some(&ownp));
                        // a small closed sub-shell of a large nearly-closed body: the body is the solid when it holds clearly more volume
                        if ok && (st.len() as f64) < 0.5 * ix.len() as f64 && nearly_closed(&t, 0.12, 40) {
                            let vw = crate::refvol::volume_one(&t, &m, &f, ec);
                            let vs = crate::refvol::volume_one(&st, &sm, lf.as_deref().unwrap_or(&[]), ec);
                            if vw > 1.5 * vs { per.push((t, m, f, *g)); continue; }
                        }
                        if ok { per.push((st, sm, lf.unwrap(), *g)); }
                        else if nearly_closed(&t, 0.12, 40) { per.push((t, m, f, *g)); }        // trimmed curved faces whose boundary samples do not weld exactly
                    }
                }
                if !per.is_empty() {
                    // exact duplicate bodies (same corners) are one solid in Revit's geometry
                    let mut seen: rustc_hash::FxHashSet<(usize, [u64; 3], [u64; 3], String, String)> = Default::default();
                    per.retain(|p| {
                        let pts: Vec<V3> = p.0.iter().flat_map(|t| t.iter().copied()).collect();
                        let (mut mn, mut mx) = (pts[0], pts[0]);
                        for q in &pts { for k in 0..3 { mn[k] = mn[k].min(q[k]); mx[k] = mx[k].max(q[k]); } }
                        let r3 = |v: V3| [((v[0] * 1000.0).round_ties_even() / 1000.0 + 0.0).to_bits(), ((v[1] * 1000.0).round_ties_even() / 1000.0 + 0.0).to_bits(), ((v[2] * 1000.0).round_ties_even() / 1000.0 + 0.0).to_bits()];
                        let flat: Vec<f64> = pts.iter().flat_map(|q| [q[0].abs(), q[1].abs(), q[2].abs()]).collect();
                        let sg = (p.0.len(), r3(mn), r3(mx), format!("{:.2}", np_sum(&flat)), sty(kk, geos[p.3 as usize]).to_string());
                        seen.insert(sg)
                    });
                    own = per.iter().flat_map(|p| p.0.iter().copied()).collect();
                    ownm = per.iter().flat_map(|p| p.1.iter().cloned()).collect();
                    ownf = per.iter().flat_map(|p| p.2.iter().copied()).collect();
                    ownn = per.iter().flat_map(|p| std::iter::repeat(p.3).take(p.0.len())).collect();
                    flags.push("closed_shell".into());
                    flags.push("per_node".into());
                } else {
                    let (st, sm, ok, lf) = select_closed_shell(&own, &ownm, &ownf, Some(&ownp));
                    if ok { own = st; ownm = sm; ownn = vec![0; own.len()]; ownf = lf.unwrap(); flags.push("closed_shell".into()); }
                }
            }
            let (mut xt, mut xm, mut xn, mut xf): (Vec<Tri>, Vec<Meta>, Vec<i64>, Vec<i64>) = (vec![], vec![], vec![], vec![]);
            let mut xmat: Vec<i32> = vec![];
            if !inst.is_empty() && depth > 0 {
                let inst_sym: Vec<&Val> = inst.iter().copied().filter(|z| {
                    if !matches!(dget(z, kk.k("m_oEmbeddedSymbolGRep")), None | Some(Val::None)) { return false; }
                    let sid = dget(z, kk.k("m_instanceInfo")).map(ob).and_then(|ii| dget(ii, kk.k("m_symbolId")));
                    let c = self.cls_of(sid);
                    c.unwrap_or("").ends_with("Symbol") || (c == Some("WallSweep") && !self.is_reveal(sid))
                }).collect();
                if !inst_sym.is_empty() {
                    let r = self.inst_parts(&inst_sym, depth, detail, &mut flags, ec, &mut mkeys)?;
                    xt = r.0; xm = r.1; xn = r.2; xf = r.3; xmat = r.4;
                }
                if !xt.is_empty() {
                    let mut have: rustc_hash::FxHashSet<[[i64; 3]; 3]> = Default::default();
                    for t in own.iter().chain(pm.iter()) { have.insert(sig(t)); }
                    let mut gorder: Vec<i64> = vec![];
                    let mut grp: FxHashMap<i64, Vec<usize>> = FxHashMap::default();
                    for (i, n) in xn.iter().enumerate() {
                        let g = n.div_euclid(100000);
                        let e = grp.entry(g).or_default();
                        if e.is_empty() { gorder.push(g); }
                        e.push(i);
                    }
                    let mut keep_i: Vec<usize> = vec![];
                    for g in gorder {
                        let ix = &grp[&g];
                        let hit = ix.iter().filter(|&&i| have.contains(&sig(&xt[i]))).count();
                        if (hit as f64) < 0.5 * ix.len() as f64 { keep_i.extend(ix.iter().copied()); }
                    }
                    xmat = keep_i.iter().map(|&i| xmat[i]).collect();
                    xt = keep_i.iter().map(|&i| xt[i]).collect();
                    xm = keep_i.iter().map(|&i| xm[i].clone()).collect();
                    xn = keep_i.iter().map(|&i| xn[i]).collect();
                    xf = keep_i.iter().map(|&i| xf[i]).collect();
                }
                if !xt.is_empty() { flags.push("plus_instances".into()); }
            }
            let mut mk: Vec<i32> = if ownf.len() == own.len() { ownf.iter().map(|f| *fmat.get(f).unwrap_or(&-1)).collect() } else { vec![-1; own.len()] };
            if xmat.len() != xt.len() { xmat = vec![-1; xt.len()]; }
            mk.extend(std::iter::repeat(-1).take(pm.len()));
            mk.extend(xmat);
            let mut fid: Vec<i64> = if ownf.len() == own.len() { ownf } else { (0..own.len() as i64).collect() };
            fid.extend(std::iter::repeat(10_000_000).take(pm.len()));
            fid.extend(xf.iter().map(|f| 20_000_000 + f));
            let mut nid: Vec<i64> = if ownn.len() == own.len() { ownn } else { vec![0; own.len()] };
            nid.extend(std::iter::repeat(1_000_000_000).take(pm.len()));
            nid.extend(xn.iter().map(|n| 20_000_000 + n));
            let mut meta = ownm;
            meta.extend(std::iter::repeat(Meta { kind: "plane", ev: MetaEv::None, uv: None }).take(pm.len()));
            meta.extend(xm);
            let mut tris = own;
            tris.extend(pm);
            tris.extend(xt);
            if un > 0 { flags.push("unsupported_faces".into()); }
            return Ok(EMesh { none: false, tris, fid, nid, meta, source: "own_geometry_in_bbox", flags, bbox: bbv, mk, mkeys });
        }
        if !inst.is_empty() && depth > 0 {
            let mut ik: Vec<(Val, Val)> = vec![];
            let (t, m, n, f, mk) = self.inst_parts(&inst, depth, detail, &mut flags, ec, &mut ik)?;
            if !t.is_empty() { return Ok(EMesh { none: false, tris: t, fid: f, nid: n, meta: m, source: "symbol_instance", flags, bbox: bbv, mk, mkeys: ik }); }
        }
        let mut allt = vec![]; let mut allm = vec![]; let mut alln = vec![]; let mut allf = vec![];
        let mut allk: Vec<(Val, Val)> = vec![]; let mut allmk: Vec<i32> = vec![];
        for (gi, g) in geos.iter().enumerate() {
            let t = tessellate_geometry(kk, g, ec)?;
            allmk.extend(tri_mats(kk, &t, g, &mut allk, &mut fk));
            alln.extend(std::iter::repeat(gi as i64).take(t.tris.len()));
            allf.extend(t.fidx.iter().map(|f| gi as i64 * 100000 + f));
            allt.extend(t.tris);
            allm.extend(t.meta);
            un += t.unsup.len();
        }
        if un > 0 { flags.push("unsupported_faces".into()); }
        Ok(EMesh { none: false, tris: allt, fid: allf, nid: alln, meta: allm, source: "all_geometry", flags, bbox: bbv, mk: allmk, mkeys: allk })
    }

    fn is_reveal(&self, sid: Option<&Val>) -> bool {
        match sid.and_then(|v| v.as_int()) { Some(i) if i >= 0 => self.ws.get(&(i as u64)) == Some(&1), _ => false }
    }

    /// rvt_elemmesh._inst_parts -> (tris, metas, nids, fids)
    /// (+ per-triangle material key indices into `keys`: the reference's _inst_parts.last_mat)
    fn inst_parts(&self, inst: &[&Val], depth: i32, detail: i64, flags: &mut Vec<String>, ec: Earcut, keys: &mut Vec<(Val, Val)>) -> Rs<(Vec<Tri>, Vec<Meta>, Vec<i64>, Vec<i64>, Vec<i32>)> {
        let kk = &self.kk;
        let forb = |z: &Val| -> bool {
            match dget(z, kk.k("m_forbiddenTarget")) { Some(ft) if is_dict(ft) => dget(ft, kk.k("m_targets")).map_or(false, truthy), _ => false }
        };
        let mut inst: Vec<&Val> = { let f: Vec<&Val> = inst.iter().copied().filter(|z| !forb(z)).collect(); if f.is_empty() { inst.to_vec() } else { f } };
        let mut keep = vec![];
        for z in inst.drain(..) {
            let ii = ob(idx(z, kk.k("m_instanceInfo"))?);
            if self.cls_of(dget(ii, kk.k("m_symbolId"))) != Some("FamilyInstance") { keep.push(z); }
        }
        let (mut tris, mut metas, mut nids, mut fids) = (vec![], vec![], vec![], vec![]);
        let mut mats: Vec<i32> = vec![];
        let mut ninst: i64 = 0;
        for z in keep {
            let ii = ob(idx(z, kk.k("m_instanceInfo"))?);
            let t = dget(ii, kk.k("m_Trf"));
            let sid = dget(ii, kk.k("m_symbolId"));
            let (t, sid) = match (t, sid) { (Some(t), Some(s)) if truthy(t) && !matches!(s, Val::None) => (t, s), _ => continue };
            let sidv = sid.as_int().ok_or(Raise)?;
            let subr = self.element_mesh(sidv as u64, depth - 1, detail, ec);
            let sub = match &*subr { Ok(m) => m, Err(_) => return Err(Raise) };
            if sub.tris.is_empty() { flags.push("symbol_no_graphics".into()); continue; }
            let m3l = list(idx(t, kk.k("m_3x3"))?)?;
            if m3l.len() != 3 { return Err(Raise); }
            let r = [v3(&m3l[0])?, v3(&m3l[1])?, v3(&m3l[2])?];
            let o = v3(idx(t, kk.k("m_or"))?)?;
            ninst += 1;
            let n = sub.tris.len();
            if !sub.fid.is_empty() { fids.extend(sub.fid.iter().map(|f| ninst * 1_000_000 + f)); } else { fids.extend((0..n as i64).map(|f| ninst * 1_000_000 + f)); }
            if !sub.nid.is_empty() { nids.extend(sub.nid.iter().map(|x| ninst * 100000 + x)); } else { nids.extend(std::iter::repeat(ninst * 100000).take(n)); }
            if sub.mk.len() == n {
                let off = keys.len() as i32;
                keys.extend(sub.mkeys.iter().cloned());
                mats.extend(sub.mk.iter().map(|&k| if k < 0 { -1 } else { k + off }));
            } else { mats.extend(std::iter::repeat(-1).take(n)); }
            for tr in &sub.tris { tris.push([add(matvec(&r, tr[0]), o), add(matvec(&r, tr[1]), o), add(matvec(&r, tr[2]), o)]); }
            if !sub.meta.is_empty() && sub.meta.len() == n {
                for m in &sub.meta {
                    let ev = match &m.ev { MetaEv::S(s) => MetaEv::S(Arc::new(Surf::Xf { s: s.clone(), r, o })), _ => MetaEv::Broken };
                    metas.push(Meta { kind: m.kind, ev, uv: m.uv });
                }
            } else {
                metas.extend(std::iter::repeat(Meta { kind: "plane", ev: MetaEv::None, uv: None }).take(n));
            }
        }
        Ok((tris, metas, nids, fids, mats))
    }
}

// ---------------- Python face ----------------
/// a face's surface evaluator: ev(u, v) -> numpy xyz (the reference's closure)
#[pyclass]
pub struct SurfEval {
    s: Option<Arc<Surf>>,
    np_array: Py<PyAny>,
}

#[pymethods]
impl SurfEval {
    fn __call__(&self, py: Python<'_>, u: f64, v: f64) -> PyResult<PyObject> {
        match &self.s {
            Some(s) => { let p = s.eval(u, v); self.np_array.call1(py, (vec![p[0], p[1], p[2]],)) }
            None => Err(PyTypeError::new_err("'NoneType' object is not callable")),
        }
    }
}

pub(crate) struct Py_ {
    np: Py<PyAny>,
    earcut: Py<PyAny>,
}

/// mapbox_earcut.triangulate_float64 through Python (same triangulation as the reference): one GIL acquisition per batch (one Geometry node)
fn earcut_py(p: &Py_, reqs: &[(&[UV], &[u32])]) -> Vec<Option<Vec<u32>>> {
    Python::with_gil(|py| {
        let np = p.np.bind(py);
        let one = |verts: &[UV], ends: &[u32]| -> Option<Vec<u32>> {
            let arr = if verts.is_empty() {
                np.getattr("array").ok()?.call1((PyList::empty_bound(py),)).ok()?.call_method1("astype", ("float64",)).ok()?
            } else {
                let mut b = Vec::with_capacity(verts.len() * 16);
                for q in verts { b.extend_from_slice(&q[0].to_le_bytes()); b.extend_from_slice(&q[1].to_le_bytes()); }
                np.getattr("frombuffer").ok()?.call1((PyBytes::new_bound(py, &b), "<f8")).ok()?.call_method1("reshape", (-1, 2)).ok()?.call_method0("copy").ok()?
            };
            let rings = np.getattr("array").ok()?.call1((ends.to_vec(), "uint32")).ok()?;
            let r = p.earcut.bind(py).call1((arr, rings)).ok()?;
            let bytes: Vec<u8> = r.call_method1("astype", ("<u4",)).ok()?.call_method0("tobytes").ok()?.extract().ok()?;
            Some(bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect())
        };
        reqs.iter().map(|(v, e)| one(v, e)).collect()
    })
}

#[pyclass]
pub struct MeshDoc {
    ctx: Arc<Ctx>,
    conv: Arc<Conv>,
    py_: Arc<Py_>,
}

impl MeshDoc {
    pub fn new(py: Python<'_>, ctx: Ctx, conv: Conv) -> PyResult<MeshDoc> {
        let np = py.import_bound("numpy")?.unbind().into_any();
        let earcut = py.import_bound("mapbox_earcut")?.getattr("triangulate_float64")?.unbind();
        Ok(MeshDoc { ctx: Arc::new(ctx), conv: Arc::new(conv), py_: Arc::new(Py_ { np, earcut }) })
    }

    fn to_py<'py>(&self, py: Python<'py>, m: &EMesh, compact: bool) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        let np = self.py_.np.bind(py);
        // tris: tuples of 3 numpy rows (views into one array)
        let mut b = Vec::with_capacity(m.tris.len() * 72);
        for t in &m.tris { for p in t { for x in p { b.extend_from_slice(&x.to_le_bytes()); } } }
        let arr = np.getattr("frombuffer")?.call1((PyBytes::new_bound(py, &b), "<f8"))?.call_method1("reshape", (-1, 3))?.call_method0("copy")?;
        if compact {
            d.set_item("tris", arr.call_method1("reshape", (-1, 3, 3))?)?;
        } else {
            let rows: Vec<PyObject> = arr.iter()?.map(|r| r.map(|x| x.unbind())).collect::<PyResult<_>>()?;
            let tl: Vec<PyObject> = rows.chunks_exact(3).map(|c| PyTuple::new_bound(py, c.iter().map(|x| x.clone_ref(py))).into_py(py)).collect();
            d.set_item("tris", PyList::new_bound(py, tl))?;
        }
        if !m.none {
            if compact {
                // compact form: numpy arrays (fid/nid int64, surf uint8 codes into surf_names)
                let arr_i64 = |v: &[i64]| -> PyResult<Bound<'py, PyAny>> {
                    let mut b = Vec::with_capacity(v.len() * 8);
                    for x in v { b.extend_from_slice(&x.to_le_bytes()); }
                    np.getattr("frombuffer")?.call1((PyBytes::new_bound(py, &b), "<i8"))?.call_method0("copy")
                };
                d.set_item("fid", arr_i64(&m.fid)?)?;
                d.set_item("nid", arr_i64(&m.nid)?)?;
                let mut names: Vec<&str> = vec![];
                let mut codes: Vec<u8> = Vec::with_capacity(m.meta.len());
                for x in &m.meta {
                    let c = match names.iter().position(|n| *n == x.kind) { Some(i) => i, None => { names.push(x.kind); names.len() - 1 } };
                    codes.push(c as u8);
                }
                d.set_item("surf", np.getattr("frombuffer")?.call1((PyBytes::new_bound(py, &codes), "u1"))?.call_method0("copy")?)?;
                d.set_item("surf_names", PyList::new_bound(py, names))?;
            } else {
                d.set_item("fid", PyList::new_bound(py, &m.fid))?;
                d.set_item("nid", PyList::new_bound(py, &m.nid))?;
            {
                let mut evs: FxHashMap<usize, PyObject> = FxHashMap::default();
                let none_ev: Py<SurfEval> = Py::new(py, SurfEval { s: None, np_array: np.getattr("array")?.unbind() })?;
                let ml = PyList::empty_bound(py);
                for x in &m.meta {
                    let ev: PyObject = match &x.ev {
                        MetaEv::None => py.None(),
                        MetaEv::Broken => none_ev.clone_ref(py).into_py(py),
                        MetaEv::S(s) => {
                            let k = Arc::as_ptr(s) as usize;
                            match evs.get(&k) { Some(o) => o.clone_ref(py), None => { let o = Py::new(py, SurfEval { s: Some(s.clone()), np_array: np.getattr("array")?.unbind() })?.into_py(py); evs.insert(k, o.clone_ref(py)); o } }
                        }
                    };
                    let uv: PyObject = match &x.uv { None => py.None(), Some(u) => PyTuple::new_bound(py, u.iter().map(|q| PyTuple::new_bound(py, [q[0], q[1]]))).into_py(py) };
                    ml.append(PyTuple::new_bound(py, [x.kind.into_py(py), ev, uv]))?;
                }
                d.set_item("meta", ml)?;
            }
        }
            }
        if !m.none {
            let ks = PyList::empty_bound(py);
            for (a, b) in &m.mkeys { ks.append(PyTuple::new_bound(py, [self.conv.val(py, a, &[])?, self.conv.val(py, b, &[])?]))?; }
            d.set_item("mat_keys", ks)?;
            if compact {
                let mut b = Vec::with_capacity(m.mk.len() * 4);
                for x in &m.mk { b.extend_from_slice(&x.to_le_bytes()); }
                d.set_item("mat_idx", np.getattr("frombuffer")?.call1((PyBytes::new_bound(py, &b), "<i4"))?.call_method0("copy")?)?;
            } else { d.set_item("mat_idx", PyList::new_bound(py, &m.mk))?; }
        }
        d.set_item("source", m.source)?;
        d.set_item("flags", PyList::new_bound(py, &m.flags))?;
        if !m.none {
            match &m.bbox { Some(v) => d.set_item("bbox", self.conv.val(py, v, &[])?)?, None => d.set_item("bbox", py.None())? }
        }
        Ok(d)
    }
}

#[pymethods]
impl MeshDoc {
    /// == rvt_elemmesh.element_mesh(doc, eid, symbol_depth, detail); KeyError when the reference would raise
    #[pyo3(signature = (eid, symbol_depth=2, detail=3))]
    fn element_mesh<'py>(&self, py: Python<'py>, eid: u64, symbol_depth: i32, detail: i64) -> PyResult<Bound<'py, PyDict>> {
        let ctx = self.ctx.clone();
        let pp = self.py_.clone();
        let r = py.allow_threads(move || {
            let ec = move |r: &[(&[UV], &[u32])]| earcut_py(&pp, r);
            ctx.element_mesh(eid, symbol_depth, detail, &ec)
        });
        match &*r { Ok(m) => self.to_py(py, m, false), Err(_) => Err(PyKeyError::new_err(format!("element_mesh({}) raised in the reference path", eid))) }
    }

    /// test entry: rvt_mesh.element_faces_in_bbox(geos[gi], m_bBox, 0.02) of element eid -> (fidx, fprio, unsupported, faces)
    #[pyo3(signature = (eid, gi, detail=3))]
    fn faces_in_bbox_dbg(&self, py: Python<'_>, eid: u64, gi: usize, detail: i64) -> PyResult<(Vec<i64>, Vec<(i64, bool)>, Vec<&'static str>, i64, Vec<Tri>)> {
        let ctx = self.ctx.clone();
        let pp = self.py_.clone();
        let r = py.allow_threads(move || -> Rs<(Vec<i64>, Vec<(i64, bool)>, Vec<&'static str>, i64, Vec<Tri>)> {
            let ec = move |r: &[(&[UV], &[u32])]| earcut_py(&pp, r);
            let kk = &ctx.kk;
            let rr = ctx.rec(eid);
            let fields = match &*rr { Ok(Some(f)) => f, _ => return Err(Raise) };
            let (geos, _) = geos_f(kk, fields, detail);
            let bb = bbox_of(dget(fields, kk.k("m_bBox")).ok_or(Raise)?)?;
            let t = faces_in_bbox(kk, geos.get(gi).ok_or(Raise)?, bb, 0.02, &ec)?;
            Ok((t.fidx, t.fprio, t.unsup, t.faces, t.tris))
        });
        r.map_err(|_| PyKeyError::new_err("raised"))
    }

    /// rvt_mesh.refined_volume(element_mesh(eid).tris, .meta, fid=.fid) for many elements in parallel -> list of (volume, closed, area) | None
    /// (None: no triangles, or the reference would raise)
    #[pyo3(signature = (ids, symbol_depth=2, detail=3, levels=3))]
    fn refined_volumes(&self, py: Python<'_>, ids: Vec<u64>, symbol_depth: i32, detail: i64, levels: i32) -> PyResult<Vec<Option<(f64, bool, f64)>>> {
        let ctx = self.ctx.clone();
        let pp = self.py_.clone();
        Ok(py.allow_threads(move || {
            let ec = move |r: &[(&[UV], &[u32])]| earcut_py(&pp, r);
            crate::pool::run(|| ids.par_iter().map(|&e| {
                let m = ctx.element_mesh(e, symbol_depth, detail, &ec);
                match &*m {
                    Ok(m) if !m.tris.is_empty() && m.meta.len() == m.tris.len() => crate::refvol::refined_volume(&m.tris, &m.meta, levels, 5e-5, Some(&m.fid)).ok().map(|r| (r.0, r.1, r.2)),
                    _ => None,
                }
            }).collect())
        }))
    }

    /// element meshes of many elements in parallel -> list of (eid, dict | None on error) in input order.
    /// compact=True: tris as one (n,3,3) array, 'surf' (kind per triangle) instead of 'meta'
    #[pyo3(signature = (ids, symbol_depth=2, detail=3, compact=false))]
    fn element_meshes<'py>(&self, py: Python<'py>, ids: Vec<u64>, symbol_depth: i32, detail: i64, compact: bool) -> PyResult<Bound<'py, PyList>> {
        let ctx = self.ctx.clone();
        let pp = self.py_.clone();
        let res: Vec<Arc<Rs<EMesh>>> = py.allow_threads(move || {
            let ec = move |r: &[(&[UV], &[u32])]| earcut_py(&pp, r);
            crate::pool::run(|| ids.par_iter().map(|&e| ctx.element_mesh(e, symbol_depth, detail, &ec)).collect())
        });
        let out = PyList::empty_bound(py);
        for r in res.iter() {
            match &**r { Ok(m) => out.append(self.to_py(py, m, compact)?)?, Err(_) => out.append(py.None())? }
        }
        Ok(out)
    }
}

/// rvt_mesh.refined_volume(tris, meta, levels, tol, centroid, fid) for meshes whose evaluators are this extension's SurfEval objects (element_mesh results);
/// raises NotImplementedError when an evaluator is a Python callable (the caller then runs the reference), ValueError where the reference would raise
#[pyfunction]
#[pyo3(signature = (tris, meta, levels=3, tol=5e-5, centroid=false, fid=None))]
pub fn refined_volume(py: Python<'_>, tris: Vec<[[f64; 3]; 3]>, meta: &Bound<'_, PyList>, levels: i32, tol: f64, centroid: bool, fid: Option<Vec<i64>>) -> PyResult<PyObject> {
    const KINDS: [&str; 10] = ["plane", "cylinder", "cone", "revolution", "ruled", "hermite", "offset", "nurbs", "spline", "other"];
    let mut ms: Vec<Meta> = Vec::with_capacity(meta.len());
    for x in meta.iter() {
        let t = x.downcast::<PyTuple>()?;
        let kind: String = t.get_item(0)?.extract()?;
        let kind: &'static str = KINDS.iter().copied().find(|k| *k == kind).unwrap_or_else(|| if kind == "plane" { "plane" } else { "other" });
        let evo = t.get_item(1)?;
        let ev = if evo.is_none() { MetaEv::None } else {
            match evo.downcast::<SurfEval>() {
                Ok(se) => match &se.borrow().s { Some(s) => MetaEv::S(s.clone()), None => MetaEv::Broken },
                Err(_) => return Err(pyo3::exceptions::PyNotImplementedError::new_err("evaluator is not a rvt_core surface")),
            }
        };
        let uvo = t.get_item(2)?;
        let uv = if uvo.is_none() { None } else {
            let l: Vec<(f64, f64)> = uvo.extract()?;
            if l.len() != 3 { return Err(pyo3::exceptions::PyValueError::new_err("uv")); }
            Some([[l[0].0, l[0].1], [l[1].0, l[1].1], [l[2].0, l[2].1]])
        };
        ms.push(Meta { kind, ev, uv });
    }
    if ms.len() != tris.len() { return Err(pyo3::exceptions::PyValueError::new_err("meta length")); }
    if let Some(f) = &fid { if f.len() != tris.len() { return Err(pyo3::exceptions::PyValueError::new_err("fid length")); } }
    let r = py.allow_threads(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::refvol::refined_volume(&tris, &ms, levels, tol, fid.as_deref()))));
    let r = r.map_err(|_| pyo3::exceptions::PyValueError::new_err("internal error (panic) in refined_volume: use the reference"))?;
    let (v, c, a, cen) = r.map_err(|_| pyo3::exceptions::PyValueError::new_err("evaluator raised in the reference path"))?;
    if centroid {
        let np = py.import_bound("numpy")?;
        Ok((v, c, a, np.getattr("array")?.call1((cen.to_vec(),))?).into_py(py))
    } else { Ok((v, c, a).into_py(py)) }
}

/// test entry: rvt_mesh.refine_curved(verts, idx, ev) with ev a SurfEval -> (verts, triangles)
#[pyfunction]
#[pyo3(signature = (verts, idx, ev, rel_tol=5e-3, abs_tol=0.0033, max_tris=2000))]
pub fn refine_curved_dbg(verts: Vec<(f64, f64)>, idx: Vec<u32>, ev: &Bound<'_, SurfEval>, rel_tol: f64, abs_tol: f64, max_tris: usize) -> PyResult<(Vec<(f64, f64)>, Vec<[usize; 3]>)> {
    let s = ev.borrow().s.clone().ok_or_else(|| pyo3::exceptions::PyValueError::new_err("no surface"))?;
    let v: Vec<UV> = verts.iter().map(|p| [p.0, p.1]).collect();
    let (nv, t) = refine_curved(&v, &idx, &s, rel_tol, abs_tol, max_tris);
    Ok((nv.iter().map(|p| (p[0], p[1])).collect(), t))
}

/// build.body_summary numbers in Rust: (rows, body index per triangle as bytes); P = float64 little-endian bytes of n * 9 values (P.tobytes())
#[pyfunction]
#[pyo3(signature = (p, nid, fid=None, limit=12))]
pub fn body_summary_core(py: Python<'_>, p: &[u8], nid: Vec<i64>, fid: Option<Vec<i64>>, limit: usize) -> PyResult<(Vec<(i64, usize, Option<usize>, f64, bool, [f64; 3], [f64; 3], [f64; 3])>, Py<PyBytes>)> {
    if p.len() % 8 != 0 { return Err(pyo3::exceptions::PyValueError::new_err("P bytes")); }
    let v: Vec<f64> = p.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect();
    if v.len() != nid.len() * 9 { return Err(pyo3::exceptions::PyValueError::new_err("P and nid lengths differ")); }
    if let Some(f) = &fid { if f.len() != nid.len() { return Err(pyo3::exceptions::PyValueError::new_err("fid length")); } }
    let (rows, tb) = py.allow_threads(|| crate::refvol::body_summary_core(&v, &nid, fid.as_deref(), limit));
    let bytes: Vec<u8> = tb.iter().map(|x| *x as u8).collect();
    Ok((rows, PyBytes::new_bound(py, &bytes).unbind()))
}


// ---------------- rvt_mesh.tessellate_geometry on a Python dict (rvt-sheets, any caller holding decoded Geometry nodes) ----------------

fn py_to_val(o: &Bound<'_, PyAny>, kk: &K, depth: u32) -> PyResult<Val> {
    use pyo3::types::{PyBool, PyDict as D, PyFloat, PyInt, PyList as L, PyString, PyTuple as T, PyBytes as B};
    if depth > 200 { return Err(pyo3::exceptions::PyRecursionError::new_err("geometry too deep")); }
    if o.is_none() { return Ok(Val::None); }
    if o.is_instance_of::<PyBool>() { return Ok(Val::I(o.extract::<bool>()? as i64)); }
    if o.is_instance_of::<PyInt>() {
        return Ok(match o.extract::<i64>() { Ok(v) => Val::I(v), Err(_) => Val::U(o.extract::<u64>()?) });
    }
    if o.is_instance_of::<PyFloat>() { return Ok(Val::F(o.extract::<f64>()?)); }
    if o.is_instance_of::<PyString>() {
        let s: String = o.extract()?;
        return Ok(if s == "<geometry not decoded>" { Val::Geo } else { Val::S(s) });
    }
    if let Ok(b) = o.downcast::<B>() { return Ok(Val::B(b.as_bytes().to_vec())); }
    if let Ok(l) = o.downcast::<L>() { return Ok(Val::L(l.iter().map(|x| py_to_val(&x, kk, depth + 1)).collect::<PyResult<_>>()?)); }
    if let Ok(t) = o.downcast::<T>() { return Ok(Val::T(t.iter().map(|x| py_to_val(&x, kk, depth + 1)).collect::<PyResult<_>>()?)); }
    if let Ok(d) = o.downcast::<D>() {
        let mut out = Vec::with_capacity(d.len());
        for (k, v) in d.iter() {
            let ks: String = match k.extract() { Ok(s) => s, Err(_) => continue };
            if let Some(id) = kk.m.get(ks.as_str()) { if *id != u32::MAX { out.push((*id, py_to_val(&v, kk, depth + 1)?)); } }
        }
        return Ok(Val::D(out));
    }
    // numpy scalars and other number-likes
    if let Ok(v) = o.extract::<f64>() { return Ok(Val::F(v)); }
    Err(pyo3::exceptions::PyTypeError::new_err(format!("unsupported value in geometry: {}", o.get_type().name()?)))
}

fn val_to_py(py: Python<'_>, v: &Val) -> PyObject {
    match v {
        Val::None => py.None(),
        Val::I(x) | Val::EI(x) => x.into_py(py),
        Val::U(x) | Val::EU(x) => x.into_py(py),
        Val::F(x) => x.into_py(py),
        Val::S(s) => s.into_py(py),
        _ => py.None(),
    }
}

/// rvt_mesh.tessellate_geometry(geo) for a decoded Geometry node (Python dicts, pointer slots already resolved as {'handle','ptr','obj'}):
/// dict(tris = float64 array (n, 3, 3), unsupported = [kinds], faces = n, fidx = [face number per triangle], fprio = {face number: bool}, frs = [m_renderStyleId per triangle]).
/// `meta` (surface evaluators) is not returned. Raises KeyError where the reference raises.
#[pyfunction]
#[pyo3(name = "tessellate_geometry")]
pub fn tessellate_geometry_py<'py>(py: Python<'py>, geo: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let kk = K::standalone();
    let g = py_to_val(geo, &kk, 0)?;
    let pp = Arc::new(Py_ { np: py.import_bound("numpy")?.unbind().into_any(), earcut: py.import_bound("mapbox_earcut")?.getattr("triangulate_float64")?.unbind() });
    let t = py.allow_threads(|| {
        let ec = |r: &[(&[UV], &[u32])]| earcut_py(&pp, r);
        crate::mesh::tessellate_geometry(&kk, &g, &ec)
    });
    let t = t.map_err(|_| PyKeyError::new_err("tessellate_geometry raised in the reference path"))?;
    let d = PyDict::new_bound(py);
    let np = pp.np.bind(py);
    let mut b = Vec::with_capacity(t.tris.len() * 72);
    for tr in &t.tris { for p in tr { for x in p { b.extend_from_slice(&x.to_le_bytes()); } } }
    let arr = np.getattr("frombuffer")?.call1((PyBytes::new_bound(py, &b), "<f8"))?.call_method1("reshape", (-1, 3, 3))?.call_method0("copy")?;
    d.set_item("tris", arr)?;
    d.set_item("unsupported", PyList::new_bound(py, &t.unsup))?;
    d.set_item("faces", t.faces)?;
    d.set_item("fidx", PyList::new_bound(py, &t.fidx))?;
    let fp = PyDict::new_bound(py);
    for (k, v) in &t.fprio { fp.set_item(*k, *v)?; }
    d.set_item("fprio", fp)?;
    let frs: Vec<PyObject> = t.fidx.iter().map(|f| if *f >= 1 && (*f as usize) <= t.frs.len() { val_to_py(py, &t.frs[*f as usize - 1]) } else { py.None() }).collect();
    d.set_item("frs", PyList::new_bound(py, frs))?;
    Ok(d)
}

/// rvtsheets.gfx.Collector._index: {handle: node dict} over every dict {'handle': int != 0xFFFFFFFF, 'obj': ...} found by a pre-order walk of dicts (values in order) and
/// lists (tuples are not entered); a later node with the same handle replaces the earlier one. Keys and values are the original objects.
#[pyfunction]
pub fn index_handles<'py>(py: Python<'py>, x: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    use pyo3::types::PyInt;
    let out = PyDict::new_bound(py);
    let (kh, ko) = (pyo3::intern!(py, "handle"), pyo3::intern!(py, "obj"));
    let mut st: Vec<Bound<'py, PyAny>> = vec![x.clone()];
    while let Some(n) = st.pop() {
        if let Ok(d) = n.downcast::<PyDict>() {
            if let (Some(h), true) = (d.get_item(kh)?, d.contains(ko)?) {
                if h.is_instance_of::<PyInt>() && h.ne(0xFFFF_FFFFu64)? { out.set_item(&h, d)?; }
            }
            let vals: Vec<Bound<'py, PyAny>> = d.values().iter().collect();
            for v in vals.into_iter().rev() { if v.is_instance_of::<PyDict>() || v.is_instance_of::<PyList>() { st.push(v); } }
        } else if let Ok(l) = n.downcast::<PyList>() {
            let items: Vec<Bound<'py, PyAny>> = l.iter().collect();
            for v in items.into_iter().rev() { if v.is_instance_of::<PyDict>() || v.is_instance_of::<PyList>() { st.push(v); } }
        }
    }
    Ok(out)
}
