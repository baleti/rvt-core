//! Saved-graphics B-rep tessellation: port of rvt_mesh.py (surface_eval, curves, face loops, tessellate_geometry, select_closed_shell, tjunction_closed,
//! element_faces_in_bbox, polymesh_tris). Works on decoded record trees (Val with pointer slots resolved into dicts, see `resolve`).
//! Arithmetic follows the Python expression order; triangulation uses the same mapbox_earcut (called with the GIL) so results match the oracle.
use crate::dec::{Rec, Schema, Slot, SlotKind, Val, K_ES, K_HANDLE, K_OBJ, K_PTR, K_REF};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub type V3 = [f64; 3];
pub type UV = [f64; 2];

#[inline] pub fn add(a: V3, b: V3) -> V3 { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
#[inline] pub fn sub(a: V3, b: V3) -> V3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
#[inline] pub fn mul(s: f64, a: V3) -> V3 { [s * a[0], s * a[1], s * a[2]] }
#[inline] pub fn dot(a: V3, b: V3) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
#[inline] pub fn cross(a: V3, b: V3) -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
#[inline] pub fn norm(a: V3) -> f64 { dot(a, a).sqrt() }
/// Python round(x) -> int (ties to even)
#[inline] pub fn pyround(x: f64) -> i64 { x.round_ties_even() as i64 }

/// error = the Python reference raises (KeyError, TypeError, IndexError ...)
#[derive(Debug, Clone)]
pub struct Raise;
pub type Rs<T> = Result<T, Raise>;

/// resolve pointer slots into plain dicts {'handle', 'ptr'|'es', ['obj']} (each slot is referenced once in a record tree)
pub fn resolve(v: Val, slots: &mut Vec<Option<Slot>>) -> Val {
    match v {
        Val::Slot(i) => {
            let s = slots[i as usize].take().expect("slot used twice");
            let mut d = vec![(K_HANDLE, Val::I(s.handle as i64))];
            match s.kind {
                SlotKind::Ptr(t) => d.push((K_PTR, Val::I(t as i64))),
                SlotKind::Es(g) => d.push((K_ES, Val::S(g))),
            }
            if let Some(o) = s.obj { d.push((K_OBJ, resolve(o, slots))); }
            Val::D(d)
        }
        Val::D(d) => Val::D(d.into_iter().map(|(k, x)| (k, resolve(x, slots))).collect()),
        Val::L(l) => Val::L(l.into_iter().map(|x| resolve(x, slots)).collect()),
        Val::T(l) => Val::T(l.into_iter().map(|x| resolve(x, slots)).collect()),
        o => o,
    }
}

pub fn resolve_rec(r: Rec) -> Val {
    let mut slots: Vec<Option<Slot>> = r.slots.into_iter().map(Some).collect();
    resolve(r.fields, &mut slots)
}

pub const K_NAMES: &[&str] = &[
            "m_bBox", "m_pFacetedTopology", "m_oConditions", "m_comp", "m_param", "m_value", "m_negateDirCondition", "m_pFaces", "m_instanceInfo", "m_subNodes", "m_GInfo",
            "m_flags", "m_categoryId", "m_forbiddenTarget", "m_targets", "m_symbolId", "m_Trf", "m_3x3", "m_or", "m_oEmbeddedSymbolGRep", "m_radius", "m_center", "m_zVec",
            "m_xVec", "m_yVec", "m_NodeArray", "m_uParams", "m_vParams", "m_iPoint", "m_iTangent", "m_iMixedDer", "m_halfAngle", "m_pProfileCurve", "m_pProfileCurve1",
            "m_pProfileCurve2", "m_Point1", "m_Point2", "m_origin", "m_edgeMap", "m_firstAndLastEdgePnts", "m_pFace", "uv", "m_interiorEdgePnts", "m_next", "m_prev",
            "m_pSurf", "m_pFirstLoop", "m_Envelope", "m_corners", "m_pointsArr", "m_facetsArr", "m_Knots", "m_pitchOver2PI", "m_oBaseCurve", "m_xLen", "m_yLen", "m_dirVec",
            "m_endParams", "m_iParametr", "m_Degree", "m_ctrlPoints", "m_xyz", "m_pnt", "m_point", "m_pt", "m_weight", "m_w", "m_basePnt", "m_referenceVector",
            "m_signedOffsetDistance", "m_pEdges", "m_renderStyleId",
        ];
/// key ids of every field name the mesh code reads (u32::MAX = not in this schema)
#[allow(non_snake_case)]
pub struct K {
    pub m: FxHashMap<&'static str, u32>,
}
impl K {
    pub fn new(s: &Schema) -> K { K::build(|n| s.key_of.get(n).copied().unwrap_or(u32::MAX)) }
    /// key ids without a file schema (dicts handed in from Python): names numbered after the reserved K_* ids
    pub fn standalone() -> K {
        let ix: FxHashMap<&str, u32> = K_NAMES.iter().enumerate().map(|(i, n)| (*n, 16 + i as u32)).collect();
        K::build(|n| ix.get(n).copied().unwrap_or(u32::MAX))
    }
    fn build(f: impl Fn(&str) -> u32) -> K {
        let names = K_NAMES;
        let mut m = FxHashMap::default();
        for n in names.iter().copied() { m.insert(n, f(n)); }
        m.insert("handle", K_HANDLE);
        m.insert("obj", K_OBJ);
        m.insert("ref", K_REF);
        m.insert("ptr", K_PTR);
        K { m }
    }
    #[inline] pub fn k(&self, n: &str) -> u32 { debug_assert!(self.m.contains_key(n), "field name {} not registered in K::new", n); *self.m.get(n).unwrap_or(&u32::MAX) }
}

// ---------------- dict helpers mirroring Python semantics ----------------
#[inline] pub fn is_dict(v: &Val) -> bool { matches!(v, Val::D(_)) }
#[inline] pub fn is_list(v: &Val) -> bool { matches!(v, Val::L(_)) }
#[inline] pub fn dget(v: &Val, k: u32) -> Option<&Val> { if let Val::D(d) = v { d.iter().find(|(kk, _)| *kk == k).map(|(_, x)| x) } else { None } }
#[inline] pub fn has(v: &Val, k: u32) -> bool { dget(v, k).is_some() }
/// _ob(o): o.get('obj', o) for dicts
#[inline] pub fn ob(v: &Val) -> &Val { if let Val::D(_) = v { dget(v, K_OBJ).unwrap_or(v) } else { v } }
/// d[k] (KeyError -> Raise); non-dict -> Raise
#[inline] pub fn idx<'a>(v: &'a Val, k: u32) -> Rs<&'a Val> { dget(v, k).ok_or(Raise) }
/// Python truthiness
pub fn truthy(v: &Val) -> bool {
    match v {
        Val::None => false,
        Val::I(x) | Val::EI(x) => *x != 0,
        Val::U(x) | Val::EU(x) => *x != 0,
        Val::F(x) => *x != 0.0,
        Val::S(s) => !s.is_empty(),
        Val::B(b) => !b.is_empty(),
        Val::L(l) | Val::T(l) => !l.is_empty(),
        Val::D(d) => !d.is_empty(),
        _ => true,
    }
}
/// float(x)
pub fn fl(v: &Val) -> Rs<f64> {
    match v {
        Val::F(x) => Ok(*x),
        Val::I(x) | Val::EI(x) => Ok(*x as f64),
        Val::U(x) | Val::EU(x) => Ok(*x as f64),
        _ => Err(Raise),
    }
}
pub fn int_of(v: &Val) -> Option<i128> { v.as_int() }
pub fn list(v: &Val) -> Rs<&[Val]> { match v { Val::L(l) | Val::T(l) => Ok(l), _ => Err(Raise) } }
/// [float(a) for a in x] as a 3-vector (np.array(_v(x)))
pub fn v3(v: &Val) -> Rs<V3> {
    let l = list(v)?;
    if l.len() != 3 { return Err(Raise); }
    Ok([fl(&l[0])?, fl(&l[1])?, fl(&l[2])?])
}
pub fn uv2(v: &Val) -> Rs<UV> {
    let l = list(v)?;
    if l.len() != 2 { return Err(Raise); }
    Ok([fl(&l[0])?, fl(&l[1])?])
}
pub fn fvec(v: &Val) -> Rs<Vec<f64>> { list(v)?.iter().map(fl).collect() }

// ---------------- curves (rvt_curves.py) ----------------
#[derive(Clone, Debug)]
pub enum Curve {
    Line { o: V3, d: V3 },
    Arc { c: V3, x: V3, y: V3, r: f64 },
    Ellipse { c: V3, x: V3, y: V3, a: f64, b: f64 },
    Hermite { t: Vec<f64>, p: Vec<V3>, m: Vec<V3> },
    Nurbs { deg: usize, k: Vec<f64>, x: Vec<V3>, w: Vec<f64> },
    Helix { bp: V3, x: V3, y: V3, z: V3, r: f64, p: f64 },
    Offset { base: Box<Curve>, rf: V3, d: f64 },
    /// constant point (ruled surface apex)
    Point(V3),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CKind { Hermite, Nurbs, Helix, Offset, Ellipse, Arc, Line }

pub fn curve_kind(kk: &K, c: &Val) -> Option<CKind> {
    let c = ob(c);
    if !is_dict(c) { return None; }
    if has(c, kk.k("m_NodeArray")) { return Some(CKind::Hermite); }
    if has(c, kk.k("m_Knots")) { return Some(CKind::Nurbs); }
    if has(c, kk.k("m_pitchOver2PI")) { return Some(CKind::Helix); }
    if has(c, kk.k("m_oBaseCurve")) { return Some(CKind::Offset); }
    if has(c, kk.k("m_xLen")) { return Some(CKind::Ellipse); }
    if has(c, kk.k("m_radius")) && has(c, kk.k("m_center")) { return Some(CKind::Arc); }
    if has(c, kk.k("m_dirVec")) { return Some(CKind::Line); }
    None
}

fn searchsorted_right(a: &[f64], x: f64) -> usize { a.partition_point(|&v| v <= x) }

impl Curve {
    pub fn eval(&self, t: f64) -> V3 {
        match self {
            Curve::Line { o, d } => add(*o, mul(t, *d)),
            Curve::Arc { c, x, y, r } => add(add(*c, mul(r * t.cos(), *x)), mul(r * t.sin(), *y)),
            Curve::Ellipse { c, x, y, a, b } => add(add(*c, mul(a * t.cos(), *x)), mul(b * t.sin(), *y)),
            Curve::Hermite { t: tt, p, m } => {
                let n = tt.len();
                let i = (searchsorted_right(tt, t) as i64 - 1).clamp(0, n as i64 - 2) as usize;
                let h = tt[i + 1] - tt[i];
                let s = (t - tt[i]) / h;
                let s2 = s.powf(2.0);
                let s3 = s.powf(3.0);
                let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
                let h10 = s3 - 2.0 * s2 + s;
                let h01 = -2.0 * s3 + 3.0 * s2;
                let h11 = s3 - s2;
                add(add(add(mul(h00, p[i]), mul(h10 * h, m[i])), mul(h01, p[i + 1])), mul(h11 * h, m[i + 1]))
            }
            Curve::Nurbs { deg, k, x, w } => {
                let n: Vec<f64> = (0..x.len()).map(|i| basis(k, i, *deg, t) * w[i]).collect();
                let s: f64 = np_sum(&n);
                if s != 0.0 {
                    let mut acc = [0.0; 3];
                    for (i, p) in x.iter().enumerate() { acc = add(acc, mul(n[i], *p)); }
                    [acc[0] / s, acc[1] / s, acc[2] / s]
                } else { x[0] }
            }
            Curve::Helix { bp, x, y, z, r, p } => add(add(add(*bp, mul(r * t.cos(), *x)), mul(r * t.sin(), *y)), mul(p * t, *z)),
            Curve::Offset { base, rf, d } => {
                let h = 1e-5;
                let tan = sub(base.eval(t + h), base.eval(t - h));
                let mut n = cross(tan, *rf);
                let l = norm(n);
                let l = if l != 0.0 { l } else { 1.0 };
                n = [n[0] / l, n[1] / l, n[2] / l];
                add(base.eval(t), mul(*d, n))
            }
            Curve::Point(p) => *p,
        }
    }
}

fn basis(k: &[f64], i: usize, d: usize, t: f64) -> f64 {
    if d == 0 {
        let last = *k.last().unwrap();
        return if (k[i] <= t && t < k[i + 1]) || (t == last && k[i] < k[i + 1] && k[i + 1] == last) { 1.0 } else { 0.0 };
    }
    let mut a = 0.0;
    let mut b = 0.0;
    if k[i + d] > k[i] { a = (t - k[i]) / (k[i + d] - k[i]) * basis(k, i, d - 1, t); }
    if k[i + d + 1] > k[i + 1] { b = (k[i + d + 1] - t) / (k[i + d + 1] - k[i + 1]) * basis(k, i + 1, d - 1, t); }
    a + b
}

/// rvt_curves.curve_fn -> (curve, t0, t1); Ok(None) = None; Err = raises
pub fn curve_fn(kk: &K, c: &Val) -> Rs<Option<(Curve, f64, f64)>> {
    let c = ob(c);
    let kind = match curve_kind(kk, c) { Some(k) => k, None => return Ok(None) };
    let (t0, t1) = match dget(c, kk.k("m_endParams")) {
        Some(e) => { let l = list(e)?; if l.len() != 2 { return Err(Raise); } (fl(&l[0])?, fl(&l[1])?) }
        None => (0.0, 1.0),
    };
    let ep = has(c, kk.k("m_endParams"));
    let g = |n: &str| idx(c, kk.k(n));
    Ok(Some(match kind {
        CKind::Line => (Curve::Line { o: v3(g("m_origin")?)?, d: v3(g("m_dirVec")?)? }, t0, t1),
        CKind::Arc => (Curve::Arc { c: v3(g("m_center")?)?, x: v3(g("m_xVec")?)?, y: v3(g("m_yVec")?)?, r: fl(g("m_radius")?)? }, t0, t1),
        CKind::Ellipse => (Curve::Ellipse { c: v3(g("m_center")?)?, x: v3(g("m_xVec")?)?, y: v3(g("m_yVec")?)?, a: fl(g("m_xLen")?)?, b: fl(g("m_yLen")?)? }, t0, t1),
        CKind::Hermite => {
            let mut tt = vec![]; let mut p = vec![]; let mut m = vec![];
            for n in list(dget(c, kk.k("m_NodeArray")).unwrap_or(&Val::None)).unwrap_or(&[]) {
                let n = ob(n);
                if !is_dict(n) { continue; }
                tt.push(fl(idx(n, kk.k("m_iParametr"))?)?);
                p.push(v3(idx(n, kk.k("m_iPoint"))?)?);
                m.push(v3(idx(n, kk.k("m_iTangent"))?)?);
            }
            if tt.is_empty() { return Err(Raise); }
            let (a, b) = (tt[0], *tt.last().unwrap());
            if tt.len() < 2 { return Err(Raise); }
            (Curve::Hermite { t: tt, p, m }, if ep { t0 } else { a }, if ep { t1 } else { b })
        }
        CKind::Nurbs => {
            let deg = int_of(g("m_Degree")?).ok_or(Raise)? as usize;
            let kn = fvec(g("m_Knots")?)?;
            let mut x = vec![]; let mut w = vec![];
            for p in list(g("m_ctrlPoints")?)? {
                let p = ob(p);
                if let Val::D(dd) = p {
                    let mut xyz = None;
                    for kn_ in ["m_xyz", "m_pnt", "m_point", "m_pt"] { if let Some(v) = dget(p, kk.k(kn_)) { xyz = Some(v3(v)?); break; } }
                    let wv = match dget(p, kk.k("m_weight")) { Some(v) => fl(v)?, None => match dget(p, kk.k("m_w")) { Some(v) => fl(v)?, None => 1.0 } };
                    let xyz = match xyz {
                        Some(v) => v,
                        None => match dd.iter().find(|(_, v)| matches!(v, Val::L(l) if l.len() == 3)) { Some((_, v)) => v3(v)?, None => [0.0; 3] },
                    };
                    x.push(xyz); w.push(wv);
                } else { x.push(v3(p)?); w.push(1.0); }
            }
            if kn.len() < x.len() + 1 || deg >= kn.len() { return Err(Raise); }
            let (a, b) = (kn[deg], kn[x.len()]);
            (Curve::Nurbs { deg, k: kn, x, w }, if ep { t0 } else { a }, if ep { t1 } else { b })
        }
        CKind::Helix => {
            let bpv = g("m_basePnt")?;
            let bp = if is_list(bpv) { v3(bpv)? } else { v3(ob(bpv))? };
            let vec_ = |n: &str| -> Rs<V3> {
                let v = ob(g(n)?);
                if is_list(v) { v3(v) } else if let Val::D(d) = v { v3(&d.first().ok_or(Raise)?.1) } else { Err(Raise) }
            };
            (Curve::Helix { bp, x: vec_("m_xVec")?, y: vec_("m_yVec")?, z: vec_("m_zVec")?, r: fl(g("m_radius")?)?, p: fl(g("m_pitchOver2PI")?)? }, t0, t1)
        }
        CKind::Offset => {
            let base = curve_fn(kk, g("m_oBaseCurve")?)?;
            let rf = match dget(c, kk.k("m_referenceVector")) { Some(v) if is_list(v) => Some(v3(v)?), _ => None };
            let d = fl(g("m_signedOffsetDistance")?)?;
            match (base, rf) {
                (Some((b, a, bb)), Some(rf)) => (Curve::Offset { base: Box::new(b), rf, d }, a, bb),
                _ => return Ok(None),
            }
        }
    }))
}

// ---------------- surfaces ----------------
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SKind { Plane, Cylinder, Cone, Revolution, Ruled, Hermite }
impl SKind {
    pub fn name(&self) -> &'static str {
        match self { SKind::Plane => "plane", SKind::Cylinder => "cylinder", SKind::Cone => "cone", SKind::Revolution => "revolution", SKind::Ruled => "ruled", SKind::Hermite => "hermite" }
    }
}

#[derive(Clone, Debug)]
pub enum Surf {
    Plane { o: V3, x: V3, y: V3 },
    Cyl { c: V3, x: V3, y: V3, z: V3, r: f64 },
    Cone { c: V3, x: V3, y: V3, z: V3, sa: f64, ca: f64 },
    Rev { f: Curve, c: V3, x: V3, y: V3, z: V3 },
    Ruled { fa: Curve, a0: f64, a1: f64, fb: Curve, b0: f64, b1: f64 },
    Herm { u: Vec<f64>, v: Vec<f64>, p: Vec<V3>, tu: Vec<V3>, tv: Vec<V3>, w: Vec<V3> },
    /// symbol surface seen through an instance transform: R @ ev(u, v) + o
    Xf { s: Arc<Surf>, r: [[f64; 3]; 3], o: V3 },
}

fn hb(t: f64) -> [f64; 4] {
    let t2 = t.powf(2.0);
    let t3 = t.powf(3.0);
    [2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, -2.0 * t3 + 3.0 * t2, t3 - t2]
}

pub fn matvec(r: &[[f64; 3]; 3], p: V3) -> V3 {
    [r[0][0] * p[0] + r[0][1] * p[1] + r[0][2] * p[2], r[1][0] * p[0] + r[1][1] * p[1] + r[1][2] * p[2], r[2][0] * p[0] + r[2][1] * p[1] + r[2][2] * p[2]]
}

impl Surf {
    pub fn eval(&self, u: f64, v: f64) -> V3 {
        match self {
            Surf::Plane { o, x, y } => add(add(*o, mul(u, *x)), mul(v, *y)),
            Surf::Cyl { c, x, y, z, r } => add(add(add(*c, mul(r * u.cos(), *x)), mul(r * u.sin(), *y)), mul(v, *z)),
            Surf::Cone { c, x, y, z, sa, ca } => add(add(*c, mul(v * sa, add(mul(u.cos(), *x), mul(u.sin(), *y)))), mul(v * ca, *z)),
            Surf::Rev { f, c, x, y, z } => {
                let l = f.eval(v);
                let (cu, su) = (u.cos(), u.sin());
                add(add(add(*c, mul(l[0] * cu - l[1] * su, *x)), mul(l[0] * su + l[1] * cu, *y)), mul(l[2], *z))
            }
            Surf::Ruled { fa, a0, a1, fb, b0, b1 } => add(mul(1.0 - v, fa.eval(a0 + (a1 - a0) * u)), mul(v, fb.eval(b0 + (b1 - b0) * u))),
            Surf::Herm { u: uu, v: vv, p, tu, tv, w } => {
                let (nu, nv) = (uu.len(), vv.len());
                let i = (searchsorted_right(uu, u) as i64 - 1).clamp(0, nu as i64 - 2) as usize;
                let j = (searchsorted_right(vv, v) as i64 - 1).clamp(0, nv as i64 - 2) as usize;
                let hu = uu[i + 1] - uu[i];
                let hv = vv[j + 1] - vv[j];
                let bu = hb((u - uu[i]) / hu);
                let bv = hb((v - vv[j]) / hv);
                let mut out = [0.0; 3];
                for (a, ia) in [(0, i), (1, i + 1)] {
                    for (b, jb) in [(0, j), (1, j + 1)] {
                        let fu = if a == 0 { (bu[0], bu[1]) } else { (bu[2], bu[3]) };
                        let fv = if b == 0 { (bv[0], bv[1]) } else { (bv[2], bv[3]) };
                        let k = ia + nu * jb;
                        let term = add(add(add(mul(fu.0 * fv.0, p[k]), mul(fu.1 * hu * fv.0, tu[k])), mul(fu.0 * fv.1 * hv, tv[k])), mul(fu.1 * hu * fv.1 * hv, w[k]));
                        out = add(out, term);
                    }
                }
                out
            }
            Surf::Xf { s, r, o } => add(matvec(r, s.eval(u, v)), *o),
        }
    }
}

/// rvt_mesh.surface_eval -> Some((kind, surface)) or None
pub fn surface_eval(kk: &K, s: &Val) -> Rs<Option<(SKind, Surf)>> {
    let s = ob(s);
    if !is_dict(s) { return Ok(None); }
    let g = |n: &str| idx(s, kk.k(n));
    let h = |n: &str| has(s, kk.k(n));
    if h("m_radius") && h("m_center") && h("m_zVec") {
        return Ok(Some((SKind::Cylinder, Surf::Cyl { c: v3(g("m_center")?)?, x: v3(g("m_xVec")?)?, y: v3(g("m_yVec")?)?, z: v3(g("m_zVec")?)?, r: fl(g("m_radius")?)? })));
    }
    if h("m_NodeArray") && h("m_uParams") {
        let u = fvec(g("m_uParams")?)?;
        let v = fvec(g("m_vParams")?)?;
        let nodes = list(g("m_NodeArray")?)?;
        let (nu, nv) = (u.len(), v.len());
        if nodes.len() != nu * nv || nu < 2 || nv < 2 { return Ok(None); }
        let mut p = vec![[0.0; 3]; nu * nv]; let mut tu = p.clone(); let mut tv = p.clone(); let mut w = p.clone();
        for (k, n) in nodes.iter().enumerate() {
            let n = ob(n);
            let (i, j) = (k % nu, k / nu);
            let q = i + nu * j;
            p[q] = v3(idx(n, kk.k("m_iPoint"))?)?;
            let t = list(idx(n, kk.k("m_iTangent"))?)?;
            if t.len() < 2 { return Err(Raise); }
            tu[q] = v3(&t[0])?; tv[q] = v3(&t[1])?;
            w[q] = v3(idx(n, kk.k("m_iMixedDer"))?)?;
        }
        return Ok(Some((SKind::Hermite, Surf::Herm { u, v, p, tu, tv, w })));
    }
    if h("m_halfAngle") && h("m_center") {
        let al = fl(g("m_halfAngle")?)?;
        return Ok(Some((SKind::Cone, Surf::Cone { c: v3(g("m_center")?)?, x: v3(g("m_xVec")?)?, y: v3(g("m_yVec")?)?, z: v3(g("m_zVec")?)?, sa: al.sin(), ca: al.cos() })));
    }
    if h("m_pProfileCurve") && h("m_center") {
        return Ok(match curve_fn(kk, g("m_pProfileCurve")?)? {
            Some((f, _, _)) => Some((SKind::Revolution, Surf::Rev { f, c: v3(g("m_center")?)?, x: v3(g("m_xVec")?)?, y: v3(g("m_yVec")?)?, z: v3(g("m_zVec")?)? })),
            None => None,
        });
    }
    if h("m_pProfileCurve1") && h("m_pProfileCurve2") {
        let c1 = g("m_pProfileCurve1")?;
        let c2 = g("m_pProfileCurve2")?;
        let a = if truthy(c1) { curve_fn(kk, c1)? } else { None };
        let b = if truthy(c2) { curve_fn(kk, c2)? } else { None };
        if let (Some(a), Some(b)) = (&a, &b) {
            return Ok(Some((SKind::Ruled, Surf::Ruled { fa: a.0.clone(), a0: a.1, a1: a.2, fb: b.0.clone(), b0: b.1, b1: b.2 })));
        }
        if (a.is_some() || matches!(c1, Val::None)) && (b.is_some() || matches!(c2, Val::None)) && h("m_Point1") && h("m_Point2") && (a.is_some() || b.is_some()) {
            let p1 = v3(g("m_Point1")?)?;
            let p2 = v3(g("m_Point2")?)?;
            let (fa, a0, a1) = a.unwrap_or((Curve::Point(p1), 0.0, 1.0));
            let (fb, b0, b1) = b.unwrap_or((Curve::Point(p2), 0.0, 1.0));
            return Ok(Some((SKind::Ruled, Surf::Ruled { fa, a0, a1, fb, b0, b1 })));
        }
        return Ok(None);
    }
    if h("m_origin") && h("m_xVec") && h("m_yVec") {
        return Ok(Some((SKind::Plane, Surf::Plane { o: v3(g("m_origin")?)?, x: v3(g("m_xVec")?)?, y: v3(g("m_yVec")?)? })));
    }
    Ok(None)
}

// ---------------- edges and face loops ----------------
/// handle -> edge object, in the reference's DFS insertion order (later definitions of a handle replace the value, keep the position)
pub struct EdgeMap<'a> {
    pub order: Vec<i128>,
    pub map: FxHashMap<i128, &'a Val>,
    /// face handle -> edges bordering it, in `order` (the reference's _face_edges index; the old per-face scan of all edges was O(faces x edges))
    pub by_face: FxHashMap<i128, Vec<i128>>,
}
impl<'a> EdgeMap<'a> {
    /// edges to look at for face h (callers still test the face position inside the edge): the index entry, or every edge for a face without a handle
    #[inline] pub fn face_edges(&self, h: Option<i128>) -> &[i128] {
        match h { Some(hv) => self.by_face.get(&hv).map_or(&[][..], |v| &v[..]), None => &self.order }
    }
}

/// rvt_mesh._edge_map: stack walk (pop from the end, push dict values / list items in order)
pub fn edge_map<'a>(kk: &K, geo: &'a Val) -> EdgeMap<'a> {
    let mut em = EdgeMap { order: vec![], map: FxHashMap::default(), by_face: FxHashMap::default() };
    let kfl = kk.k("m_firstAndLastEdgePnts");
    let mut st: Vec<&'a Val> = vec![geo];
    while let Some(z) = st.pop() {
        match z {
            Val::D(d) => {
                if let (Some(h), Some(o)) = (dget(z, K_HANDLE), dget(z, K_OBJ)) {
                    if is_dict(o) && has(o, kfl) && !matches!(h, Val::None) && h.as_int() != Some(0xffff_ffff) {
                        if let Some(hv) = h.as_int() {
                            if em.map.insert(hv, o).is_none() { em.order.push(hv); }
                        }
                    }
                }
                for (_, b) in d { if !matches!(b, Val::None) { st.push(b); } }
            }
            Val::L(l) => { for b in l { st.push(b); } }
            _ => {}
        }
    }
    for &eid in &em.order {
        for r in refs_of(kk, em.map[&eid]).into_iter().flatten() {
            let l = em.by_face.entry(r).or_default();
            if l.last() != Some(&eid) { l.push(eid); }
        }
    }
    em
}

#[inline] pub fn c2(a: UV, b: UV) -> bool { (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5 }

pub fn area2(p: &[UV]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = (p[i][0], p[i][1]);
        let (x1, y1) = (p[(i + 1) % n][0], p[(i + 1) % n][1]);
        a += x0 * y1 - x1 * y0;
    }
    a / 2.0
}

/// a face's surface as the face-loop code needs it: (kind, evaluator, plane frame when planar)
pub struct FaceSurf<'a> {
    pub kind: SKind,
    pub ev: Surf,
    pub raw: &'a Val,
}

fn snap_to_plane(oev: &Surf, plane: Option<(V3, V3, V3)>, u: f64, v: f64) -> UV {
    let (o, xv, yv) = match plane { Some(p) => p, None => return [u, v] };
    let mut n = cross(xv, yv);
    let l = norm(n);
    let l = if l != 0.0 { l } else { 1.0 };
    n = [n[0] / l, n[1] / l, n[2] / l];
    let f = |vv: f64| dot(n, sub(oev.eval(u, vv), o));
    let mut v0 = v;
    let mut f0 = f(v0);
    if f0.abs() < 1e-12 { return [u, v0]; }
    let h = 1e-4;
    let mut v1 = v0 + h;
    let mut f1 = f(v1);
    for _ in 0..8 {
        if (f1 - f0).abs() < 1e-18 { return [u, v]; }
        let v2 = v1 - f1 * (v1 - v0) / (f1 - f0);
        v0 = v1; f0 = f1; v1 = v2; f1 = f(v1);
        if f1.abs() < 1e-11 { break; }
    }
    if f1.abs() < 1e-9 && (v1 - v).abs() < 2.0 { return [u, v1]; }
    [u, v]
}

fn plane_frame(kk: &K, sd: &Val) -> Option<(V3, V3, V3)> {
    let sd = ob(sd);
    Some((v3(dget(sd, kk.k("m_origin"))?).ok()?, v3(dget(sd, kk.k("m_xVec"))?).ok()?, v3(dget(sd, kk.k("m_yVec"))?).ok()?))
}

/// one edge of a face: (a, b, interior samples, next handle, prev handle)
#[derive(Clone, Debug)]
pub struct Mine {
    pub a: UV,
    pub b: UV,
    pub inter: Vec<UV>,
    pub nref: Option<i128>,
    pub pref: Option<i128>,
}

fn refs_of(kk: &K, e: &Val) -> Vec<Option<i128>> {
    match dget(e, kk.k("m_pFace")) {
        Some(Val::L(l)) => l.iter().map(|r| if is_dict(r) { dget(r, K_REF).and_then(|x| x.as_int()) } else { None }).collect(),
        _ => vec![],
    }
}

fn pt_uv(kk: &K, p: &Val, side: usize) -> Rs<UV> {
    let l = list(idx(ob(p), kk.k("uv"))?)?;
    uv2(l.get(side).ok_or(Raise)?)
}

/// rvt_mesh.face_loops_uv; returns (loops, mine in edge-map order)
pub fn face_loops_uv(kk: &K, h: Option<i128>, edges: &EdgeMap, surfs: Option<&FxHashMap<Option<i128>, FaceSurf>>, allow_gaps: bool) -> Rs<(Vec<Vec<UV>>, Vec<(i128, Mine)>)> {
    let me = surfs.and_then(|s| s.get(&h));
    let plane = match me { Some(m) if m.kind == SKind::Plane => plane_frame(kk, m.raw), _ => None };
    let plane_inv = |p: V3| -> UV {
        let (o, xv, yv) = plane.unwrap();
        let d = sub(p, o);
        [dot(d, xv), dot(d, yv)]
    };
    let kfl = kk.k("m_firstAndLastEdgePnts");
    let kint = kk.k("m_interiorEdgePnts");
    let mut mine: Vec<(i128, Mine)> = vec![];
    for &eid in edges.face_edges(h) {
        let e = edges.map[&eid];
        let refs = refs_of(kk, e);
        let side = match refs.iter().position(|r| *r == h) { Some(s) => s, None => continue };
        let flv = list(idx(e, kfl)?)?;
        if flv.len() < 2 { continue; }
        let a = pt_uv(kk, &flv[0], side)?;
        let b = pt_uv(kk, &flv[1], side)?;
        let ints: &[Val] = match dget(e, kint) { Some(x) if truthy(x) => list(x)?, _ => &[] };
        let mut inter: Vec<UV> = vec![];
        for p in ints { if is_dict(ob(p)) { inter.push(pt_uv(kk, p, side)?); } }
        let other_surf = if refs.len() == 2 { surfs.and_then(|s| s.get(&refs[1 - side])) } else { None };
        if plane.is_some() && refs.len() == 2 {
            if let Some(os) = other_surf {
                if matches!(os.kind, SKind::Cylinder | SKind::Cone | SKind::Revolution | SKind::Ruled) {
                    let ok_ = os.kind;
                    let mut other: Vec<UV> = vec![pt_uv(kk, &flv[0], 1 - side)?];
                    for p in ints { if is_dict(ob(p)) { other.push(pt_uv(kk, p, 1 - side)?); } }
                    other.push(pt_uv(kk, &flv[1], 1 - side)?);
                    let mut dense: Vec<Option<UV>> = vec![];
                    for w in other.windows(2) {
                        let ([u0, v0], [u1, v1]) = (w[0], w[1]);
                        let du = if ok_ != SKind::Ruled { (u1 - u0).abs() } else { 0.0 };
                        let kdiv = (64.0f64).min((1.0f64).max((du.max(0.0) / 0.1).ceil())) as usize;
                        for i in 1..kdiv {
                            let t = i as f64 / kdiv as f64;
                            let [uu, vv] = snap_to_plane(&os.ev, plane, u0 + (u1 - u0) * t, v0 + (v1 - v0) * t);
                            dense.push(Some(plane_inv(os.ev.eval(uu, vv))));
                        }
                        dense.push(None);
                    }
                    if dense.iter().any(|d| d.is_some()) {
                        let mut orig = vec![a];
                        orig.extend(inter.iter().copied());
                        orig.push(b);
                        let mut seq = vec![];
                        let mut oi = 1;
                        for d in dense {
                            match d { None => { seq.push(*orig.get(oi).ok_or(Raise)?); oi += 1; } Some(p) => seq.push(p) }
                        }
                        seq.pop();
                        inter = seq;
                    }
                }
            }
        }
        if let (Some(m), Some(os)) = (me, other_surf) {
            if matches!(m.kind, SKind::Cylinder | SKind::Cone | SKind::Revolution) && os.kind == SKind::Plane {
                let pf = plane_frame(kk, os.raw);
                let mut orig = vec![a];
                orig.extend(inter.iter().copied());
                orig.push(b);
                let mut seq = vec![];
                for w in orig.windows(2) {
                    let ([u0, v0], [u1, v1]) = (w[0], w[1]);
                    let kdiv = (64.0f64).min((1.0f64).max(((u1 - u0).abs() / 0.1).ceil())) as usize;
                    seq.push([u0, v0]);
                    for i in 1..kdiv {
                        let t = i as f64 / kdiv as f64;
                        seq.push(snap_to_plane(&m.ev, pf, u0 + (u1 - u0) * t, v0 + (v1 - v0) * t));
                    }
                }
                seq.remove(0);
                inter = seq;
            }
        }
        let nx = dget(e, kk.k("m_next")).filter(|x| truthy(x));
        let pv = dget(e, kk.k("m_prev")).filter(|x| truthy(x));
        let sref = |x: Option<&Val>| -> Option<i128> {
            let l = match x { Some(Val::L(l)) => l, _ => return None };
            let r = l.get(side)?;
            if is_dict(r) { dget(r, K_REF).and_then(|v| v.as_int()) } else { None }
        };
        let m = Mine { a, b, inter, nref: sref(nx), pref: sref(pv) };
        match mine.iter_mut().find(|(k, _)| *k == eid) { Some(x) => x.1 = m, None => mine.push((eid, m)) }
    }
    let loops = loops_from_mine(&mine, allow_gaps);
    Ok((loops, mine))
}

fn loops_from_mine(mine: &[(i128, Mine)], allow_gaps: bool) -> Vec<Vec<UV>> {
    let pos: FxHashMap<i128, usize> = mine.iter().enumerate().map(|(i, (k, _))| (*k, i)).collect();
    let get = |k: i128| &mine[pos[&k]].1;
    let inm = |k: Option<i128>| k.map_or(false, |k| pos.contains_key(&k));
    let mut seen: FxHashSet<i128> = FxHashSet::default();
    let mut loops: Vec<Option<Vec<UV>>> = vec![];
    let mut keys: Vec<i128> = mine.iter().map(|(k, _)| *k).collect();
    keys.sort();
    for e0 in keys {
        if seen.contains(&e0) { continue; }
        // seq_from(e0)
        let mut cur = e0;
        let mut guard = 0;
        while inm(get(cur).pref) && get(cur).pref != Some(e0) && guard < 100000 && !seen.contains(&get(cur).pref.unwrap()) {
            cur = get(cur).pref.unwrap();
            guard += 1;
        }
        let first = cur;
        let mut seq = vec![cur];
        guard = 0;
        while inm(get(cur).nref) && get(cur).nref != Some(first) && !seq.contains(&get(cur).nref.unwrap()) && guard < 100000 {
            cur = get(cur).nref.unwrap();
            seq.push(cur);
            guard += 1;
        }
        for s in &seq { seen.insert(*s); }
        let segs: Vec<&Mine> = seq.iter().map(|k| get(*k)).collect();
        if segs.len() == 1 {
            let s = segs[0];
            let mut pts = vec![s.a];
            pts.extend(s.inter.iter().copied());
            if c2(s.a, s.b) && pts.len() >= 3 { loops.push(Some(pts)); }
            continue;
        }
        let (mut a0, mut b0, mut i0) = (segs[0].a, segs[0].b, segs[0].inter.clone());
        let (a1, b1) = (segs[1].a, segs[1].b);
        if c2(a0, a1) || c2(a0, b1) {
            std::mem::swap(&mut a0, &mut b0);
            i0.reverse();
        }
        let mut pts = vec![a0];
        pts.extend(i0);
        let mut cur = b0;
        let mut gaps: Vec<(UV, UV)> = vec![];
        for s in &segs[1..] {
            if c2(s.a, cur) { pts.push(s.a); pts.extend(s.inter.iter().copied()); cur = s.b; }
            else if c2(s.b, cur) { pts.push(s.b); pts.extend(s.inter.iter().rev().copied()); cur = s.a; }
            else { gaps.push((cur, s.a)); pts.push(s.a); pts.extend(s.inter.iter().copied()); cur = s.b; }
        }
        if !c2(cur, a0) { gaps.push((cur, a0)); }
        let axis_gap = gaps.iter().all(|(p, q)| (p[0] - q[0]).abs() < 1e-9 || (p[1] - q[1]).abs() < 1e-9);
        if pts.len() >= 3 && (c2(cur, a0) || (allow_gaps && !gaps.is_empty() && gaps.len() <= 2 && axis_gap)) { loops.push(Some(pts)); } else { loops.push(None); }
    }
    if !loops.is_empty() && loops.iter().all(|l| l.as_ref().map_or(false, |l| area2(l).abs() > 1e-12)) {
        return loops.into_iter().flat_map(|l| cancel_slits(&l.unwrap())).collect();
    }
    let polys: Vec<Vec<UV>> = mine.iter().map(|(_, v)| { let mut p = vec![v.a]; p.extend(v.inter.iter().copied()); p.push(v.b); p }).collect();
    let cyc = cycles_from_polylines(&polys, 1e-6);
    if !cyc.is_empty() {
        let mut seen_k: FxHashSet<Vec<(i64, i64)>> = FxHashSet::default();
        let mut out = vec![];
        for c in cyc {
            let mut k: Vec<(i64, i64)> = c.iter().map(|p| (pyround(p[0] / 1e-5), pyround(p[1] / 1e-5))).collect();
            k.sort();
            k.dedup();
            if !seen_k.insert(k) { continue; }
            out.push(c);
        }
        return out;
    }
    let ch = face_loops_chain(mine);
    if !ch.is_empty() || std::env::var_os("RVT_NOSNAP").map_or(false, |x| !x.is_empty()) { return ch; }
    // last resort: edge endpoints that miss each other by < 2e-3 (uv) are snapped together
    let mut ends: Vec<UV> = vec![];
    let mut snap = |p: UV| -> UV {
        for q in &ends { if (p[0] - q[0]).abs() < 2e-3 && (p[1] - q[1]).abs() < 2e-3 { return *q; } }
        ends.push(p); p
    };
    let polys2: Vec<Vec<UV>> = mine.iter().map(|(_, v)| {
        let mut pl = vec![v.a]; pl.extend(v.inter.iter().copied()); pl.push(v.b);
        let n = pl.len(); pl[0] = snap(pl[0]); pl[n - 1] = snap(pl[n - 1]); pl
    }).collect();
    cycles_from_polylines(&polys2, 1e-6)
}

fn face_loops_chain(mine: &[(i128, Mine)]) -> Vec<Vec<UV>> {
    let segs: Vec<&Mine> = mine.iter().map(|(_, v)| v).collect();
    let mut used = vec![false; segs.len()];
    let mut loops = vec![];
    for i in 0..segs.len() {
        if used[i] { continue; }
        used[i] = true;
        let s = segs[i];
        let mut pts = vec![s.a];
        pts.extend(s.inter.iter().copied());
        let mut cur = s.b;
        let start = s.a;
        let mut guard = 0;
        while !c2(cur, start) && guard < segs.len() + 2 {
            guard += 1;
            let mut found = false;
            for j in 0..segs.len() {
                if used[j] { continue; }
                let t = segs[j];
                if c2(t.a, cur) { used[j] = true; pts.push(t.a); pts.extend(t.inter.iter().copied()); cur = t.b; found = true; break; }
                if c2(t.b, cur) { used[j] = true; pts.push(t.b); pts.extend(t.inter.iter().rev().copied()); cur = t.a; found = true; break; }
            }
            if !found { break; }
        }
        if c2(cur, start) && pts.len() >= 3 { loops.push(pts); }
    }
    loops
}

/// rvt_mesh._cycles_from_polylines. The reference iterates Python sets here (CPython hash-table order); this port iterates in sorted order,
/// which finds the same cycles but may start them at another vertex / list them in another order (documented deviation, compared geometrically).
pub fn cycles_from_polylines(polys: &[Vec<UV>], tol: f64) -> Vec<Vec<UV>> {
    let mut vid: FxHashMap<(i64, i64), usize> = FxHashMap::default();
    let mut vv: Vec<UV> = vec![];
    let vertex = |p: UV, vid: &mut FxHashMap<(i64, i64), usize>, vv: &mut Vec<UV>| -> usize {
        let k = (pyround(p[0] / tol), pyround(p[1] / tol));
        *vid.entry(k).or_insert_with(|| { vv.push(p); vv.len() - 1 })
    };
    let mut segs: std::collections::BTreeSet<(usize, usize)> = Default::default();
    for pl in polys {
        for w in pl.windows(2) {
            let ia = vertex(w[0], &mut vid, &mut vv);
            let ib = vertex(w[1], &mut vid, &mut vv);
            if ia != ib { segs.insert((ia.min(ib), ia.max(ib))); }
        }
    }
    let mut changed = true;
    let mut guard = 0;
    while changed && guard < 20 {
        changed = false;
        guard += 1;
        let mut new: std::collections::BTreeSet<(usize, usize)> = Default::default();
        for &(a, b) in &segs {
            let (pa, pb) = (vv[a], vv[b]);
            let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
            let l2 = dx * dx + dy * dy;
            let mut cut = None;
            for c in 0..vv.len() {
                if c == a || c == b { continue; }
                let pc = vv[c];
                let t = if l2 != 0.0 { ((pc[0] - pa[0]) * dx + (pc[1] - pa[1]) * dy) / l2 } else { -1.0 };
                if 1e-9 < t && t < 1.0 - 1e-9 {
                    let (ex, ey) = (pa[0] + t * dx - pc[0], pa[1] + t * dy - pc[1]);
                    if ex * ex + ey * ey < (10.0 * tol).powf(2.0) { cut = Some(c); break; }
                }
            }
            match cut {
                None => { new.insert((a, b)); }
                Some(c) => { new.insert((a.min(c), a.max(c))); new.insert((c.min(b), c.max(b))); changed = true; }
            }
        }
        segs = new;
    }
    let mut adj: std::collections::BTreeMap<usize, std::collections::BTreeSet<usize>> = Default::default();
    for &(a, b) in &segs { adj.entry(a).or_default().insert(b); adj.entry(b).or_default().insert(a); }
    let mut stack: Vec<usize> = adj.iter().filter(|(_, s)| s.len() < 2).map(|(v, _)| *v).collect();
    while let Some(v) = stack.pop() {
        let ws: Vec<usize> = adj.get(&v).map(|s| s.iter().copied().collect()).unwrap_or_default();
        for w in ws {
            if let Some(s) = adj.get_mut(&w) {
                s.remove(&v);
                if s.len() < 2 { stack.push(w); }
            }
        }
        adj.remove(&v);
    }
    let angle = |u: usize, v: usize, w: usize| -> f64 {
        let a1 = (vv[u][1] - vv[v][1]).atan2(vv[u][0] - vv[v][0]);
        let a2 = (vv[w][1] - vv[v][1]).atan2(vv[w][0] - vv[v][0]);
        let d = (a1 - a2).rem_euclid(2.0 * std::f64::consts::PI);
        if d > 1e-12 { d } else { 2.0 * std::f64::consts::PI }
    };
    let mut used: FxHashSet<(usize, usize)> = FxHashSet::default();
    let mut cycles = vec![];
    let keys: Vec<usize> = adj.keys().copied().collect();
    for a in keys {
        let nbrs: Vec<usize> = adj[&a].iter().copied().collect();
        for b in nbrs {
            if used.contains(&(a, b)) { continue; }
            let mut cyc = vec![a];
            let (mut u, mut v) = (a, b);
            used.insert((a, b));
            let mut guard = 0;
            while v != a && guard < 100000 {
                guard += 1;
                cyc.push(v);
                let mut cands: Vec<usize> = adj.get(&v).map(|s| s.iter().copied().filter(|&w| w != u).collect()).unwrap_or_default();
                if cands.is_empty() { cands = vec![u]; }
                let mut w = cands[0];
                let mut best = angle(u, v, w);
                for &c in &cands[1..] { let an = angle(u, v, c); if an < best { best = an; w = c; } }
                if used.contains(&(v, w)) { break; }
                used.insert((v, w));
                u = v;
                v = w;
            }
            if v == a && cyc.len() >= 3 { cycles.push(cyc.iter().map(|&i| vv[i]).collect()); }
        }
    }
    cycles
}

/// rvt_mesh._loop_via_poles: chain the face's edge polylines end to end, jumping along poles (both ends map to one 3D point); closed loop or None
pub fn loop_via_poles(mine: &[(i128, Mine)], ev: &Surf) -> Option<Vec<UV>> {
    let tol = 1e-6;
    let segs: Vec<Vec<UV>> = mine.iter().map(|(_, m)| { let mut v = vec![m.a]; v.extend(m.inter.iter().copied()); v.push(m.b); v }).collect();
    if segs.is_empty() { return None; }
    let close = |p: UV, q: UV| (p[0] - q[0]).abs() < tol && (p[1] - q[1]).abs() < tol;
    let mut used = vec![false; segs.len()];
    used[0] = true;
    let mut pts: Vec<UV> = segs[0].clone();
    let mut poles = 0;
    for _ in 0..segs.len() {
        let cur = *pts.last().unwrap();
        let mut nxt: Option<(usize, Vec<UV>)> = None;
        for (j, sg) in segs.iter().enumerate() {
            if used[j] { continue; }
            if close(sg[0], cur) { nxt = Some((j, sg.clone())); break; }
            if close(*sg.last().unwrap(), cur) { let mut r = sg.clone(); r.reverse(); nxt = Some((j, r)); break; }
        }
        if nxt.is_none() {
            'outer: for (j, sg) in segs.iter().enumerate() {
                if used[j] { continue; }
                let mut rv = sg.clone();
                rv.reverse();
                for cand in [sg.clone(), rv] {
                    if norm(sub(ev.eval(cand[0][0], cand[0][1]), ev.eval(cur[0], cur[1]))) < 1e-7 && ((cand[0][0] - cur[0]).abs() > tol || (cand[0][1] - cur[1]).abs() > tol) {
                        nxt = Some((j, cand));
                        poles += 1;
                        break 'outer;
                    }
                }
            }
        }
        let (j, sg) = match nxt { Some(x) => x, None => break };
        used[j] = true;
        pts.extend(sg);
    }
    if !used.iter().all(|u| *u) { return None; }
    let (f0, l0) = (pts[0], *pts.last().unwrap());
    if !close(l0, f0) {
        if norm(sub(ev.eval(l0[0], l0[1]), ev.eval(f0[0], f0[1]))) < 1e-7 { poles += 1; } else { return None; }
    }
    let _ = poles;
    if pts.len() >= 3 { Some(pts) } else { None }
}

/// rvt_mesh._cancel_slits
pub fn cancel_slits(ring: &[UV]) -> Vec<Vec<UV>> {
    let tol = 1e-6;
    let n = ring.len();
    if n < 4 { return vec![ring.to_vec()]; }
    let key = |p: UV| (pyround(p[0] / tol), pyround(p[1] / tol));
    let mut uniq_k: Vec<(i64, i64)> = vec![];
    let mut uniq_p: Vec<UV> = vec![];
    let mut uidx: FxHashMap<(i64, i64), usize> = FxHashMap::default();
    for &p in ring {
        let k = key(p);
        if !uidx.contains_key(&k) { uidx.insert(k, uniq_k.len()); uniq_k.push(k); uniq_p.push(p); }
    }
    let mut edges: Vec<((i64, i64), (i64, i64))> = vec![];
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let d = [b[0] - a[0], b[1] - a[1]];
        let l2 = d[0] * d[0] + d[1] * d[1];
        if l2 < 1e-18 { continue; }
        let mut sel: Vec<(f64, usize)> = vec![];
        for (j, u) in uniq_p.iter().enumerate() {
            let t = ((u[0] - a[0]) * d[0] + (u[1] - a[1]) * d[1]) / l2;
            let ex = u[0] - (a[0] + t * d[0]);
            let ey = u[1] - (a[1] + t * d[1]);
            let dist = (ex * ex + ey * ey).sqrt();
            if dist < 10.0 * tol && t > 1e-9 && t < 1.0 - 1e-9 { sel.push((t, j)); }
        }
        sel.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut chain = vec![key(a)];
        chain.extend(sel.iter().map(|(_, j)| uniq_k[*j]));
        chain.push(key(b));
        for w in chain.windows(2) { edges.push((w[0], w[1])); }
    }
    // Counter in insertion order
    let mut corder: Vec<((i64, i64), (i64, i64))> = vec![];
    let mut cnt: FxHashMap<((i64, i64), (i64, i64)), usize> = FxHashMap::default();
    for e in &edges {
        let c = cnt.entry(*e).or_insert(0);
        if *c == 0 { corder.push(*e); }
        *c += 1;
    }
    let mut keep: Vec<((i64, i64), (i64, i64))> = vec![];
    for e in &corder {
        let c = cnt[e] as i64;
        let r = c - *cnt.get(&(e.1, e.0)).unwrap_or(&0) as i64;
        for _ in 0..r.max(0) { keep.push(*e); }
    }
    if keep.len() == edges.len() { return vec![ring.to_vec()]; }
    let coords = |k: (i64, i64)| uniq_p[uidx[&k]];
    // nxt: insertion-ordered dict of lists
    let mut nkeys: Vec<(i64, i64)> = vec![];
    let mut nxt: FxHashMap<(i64, i64), Vec<(i64, i64)>> = FxHashMap::default();
    for (u, v) in &keep {
        if !nxt.contains_key(u) { nkeys.push(*u); }
        nxt.entry(*u).or_default().push(*v);
    }
    let mut loops: Vec<Vec<UV>> = vec![];
    loop {
        nkeys.retain(|k| nxt.get(k).map_or(false, |v| !v.is_empty()));
        if nkeys.is_empty() { break; }
        let start = nkeys[0];
        let mut cur = start;
        let mut lp = vec![];
        let mut guard = 0;
        while guard < 100000 {
            guard += 1;
            lp.push(coords(cur));
            let outs = match nxt.get_mut(&cur) { Some(o) if !o.is_empty() => o, _ => break };
            let nx = outs.pop().unwrap();
            if outs.is_empty() { nxt.remove(&cur); }
            cur = nx;
            if cur == start { break; }
        }
        if lp.len() >= 3 && cur == start { loops.push(lp); }
        // re-insertion order: a key deleted then re-created is appended (only empty ones are re-created, and those are filtered)
        nkeys.retain(|k| nxt.contains_key(k));
    }
    if loops.is_empty() { vec![ring.to_vec()] } else { loops }
}

/// rvt_mesh._nudge_repeats
pub fn nudge_repeats(ring: &[UV]) -> Vec<UV> {
    let eps = 1e-7;
    let mut seen: FxHashMap<(i64, i64), i64> = FxHashMap::default();
    let mut out = ring.to_vec();
    let n = ring.len();
    for (i, p) in ring.iter().enumerate() {
        let k = (pyround(p[0] * 1e6), pyround(p[1] * 1e6));
        if let Some(c) = seen.get_mut(&k) {
            let a = ring[(i + n - 1) % n];
            let b = ring[(i + 1) % n];
            let d = [(a[0] + b[0]) / 2.0 - p[0], (a[1] + b[1]) / 2.0 - p[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if l > 0.0 { out[i] = [p[0] + eps * d[0] / l * (1.0 + *c as f64), p[1] + eps * d[1] / l * (1.0 + *c as f64)]; }
            *c += 1;
        } else { seen.insert(k, 0); }
    }
    out
}

fn pip(pt: UV, poly: &[UV]) -> bool {
    let (x, y) = (pt[0], pt[1]);
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (x0, y0) = (poly[i][0], poly[i][1]);
        let (x1, y1) = (poly[(i + 1) % n][0], poly[(i + 1) % n][1]);
        if (y0 > y) != (y1 > y) && x < (x1 - x0) * (y - y0) / (y1 - y0 + 1e-300) + x0 { inside = !inside; }
    }
    inside
}

fn loop_in_loop(inner: &[UV], outer: &[UV]) -> bool {
    let n = inner.len();
    let (mut tot, mut ok) = (0usize, 0usize);
    let m = outer.len();
    for i in 0..n {
        let p = [(inner[i][0] + inner[(i + 1) % n][0]) / 2.0, (inner[i][1] + inner[(i + 1) % n][1]) / 2.0];
        tot += 1;
        if pip(p, outer) { ok += 1; continue; }
        let mut dmin = f64::INFINITY;
        for k in 0..m {
            let a = outer[k];
            let b = outer[(k + 1) % m];
            let d = [b[0] - a[0], b[1] - a[1]];
            let l2 = (d[0] * d[0] + d[1] * d[1]).max(1e-18);
            let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
            let q = [a[0] + t * d[0] - p[0], a[1] + t * d[1] - p[1]];
            let dist = (q[0] * q[0] + q[1] * q[1]).sqrt();
            if dist < dmin { dmin = dist; }
        }
        if dmin < 1e-6 { ok += 1; }
    }
    tot > 0 && ok as f64 >= 0.6 * tot as f64
}

/// rvt_mesh._group_loops -> [(outer, holes)]
pub fn group_loops(loops: &[Vec<UV>]) -> Vec<(Vec<UV>, Vec<Vec<UV>>)> {
    let mut order: Vec<usize> = (0..loops.len()).collect();
    let ar: Vec<f64> = loops.iter().map(|l| -area2(l).abs()).collect();
    order.sort_by(|&a, &b| ar[a].partial_cmp(&ar[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut parent: Vec<Option<usize>> = vec![None; loops.len()];
    for (pos, &i) in order.iter().enumerate() {
        let mut par = None;
        for &j in order[..pos].iter().rev() {
            if loop_in_loop(&loops[i], &loops[j]) { par = Some(j); break; }
        }
        parent[i] = par;
    }
    let depth = |mut i: usize| { let mut d = 0; while let Some(p) = parent[i] { i = p; d += 1; } d };
    let mut groups: Vec<(usize, Vec<UV>, Vec<Vec<UV>>)> = vec![];
    for &i in &order { if depth(i) % 2 == 0 { groups.push((i, loops[i].clone(), vec![])); } }
    for &i in &order {
        if depth(i) % 2 == 1 {
            if let Some(p) = parent[i] {
                if let Some(g) = groups.iter_mut().find(|g| g.0 == p) { g.2.push(loops[i].clone()); }
            }
        }
    }
    groups.into_iter().map(|(_, o, h)| (o, h)).collect()
}

/// numpy's pairwise summation of a contiguous float64 array (np.sum / ndarray.sum)
pub fn np_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut r = 0.0;
        for &x in a { r += x; }
        r
    } else if n <= 128 {
        let mut r = [a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]];
        let mut i = 8;
        while i < n - (n % 8) {
            for k in 0..8 { r[k] += a[i + k]; }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n { res += a[i]; i += 1; }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        np_sum(&a[..n2]) + np_sum(&a[n2..])
    }
}

// ---------------- tessellation ----------------
/// surface evaluator of a triangle's meta entry
#[derive(Clone, Debug)]
pub enum MetaEv {
    None,
    S(Arc<Surf>),
    /// _xfm around an entry without evaluator: calling it raises in the reference
    Broken,
}

#[derive(Clone, Debug)]
pub struct Meta {
    pub kind: &'static str,
    pub ev: MetaEv,
    pub uv: Option<[UV; 3]>,
}

pub type Tri = [V3; 3];

/// batch triangulator: [(vertices, ring ends)] -> flat triangle indices per polygon, None = exception (mapbox_earcut.triangulate_float64 semantics)
pub type Earcut<'e> = &'e (dyn Fn(&[(&[UV], &[u32])]) -> Vec<Option<Vec<u32>>> + Sync);

/// output of one face (or one outer+holes group of a face), in face order; triangulation deferred so that one Geometry node needs one batch call
enum Job {
    Tris(Vec<(Tri, Meta, i64)>),
    Ear { verts: Vec<UV>, ends: Vec<u32>, nf: i64, kind: SKind, ev: Arc<Surf>, drop_degenerate: bool },
}

#[derive(Default)]
pub struct Tess {
    pub tris: Vec<Tri>,
    pub meta: Vec<Meta>,
    pub fidx: Vec<i64>,
    pub fprio: Vec<(i64, bool)>,
    /// m_renderStyleId of face nf (index nf - 1): the reference's per-triangle `frs` is frs[fidx - 1]
    pub frs: Vec<Val>,
    pub unsup: Vec<&'static str>,
    pub faces: i64,
}

fn handle_of(fc: &Val) -> Rs<Option<i128>> {
    if !is_dict(fc) { return Err(Raise); }
    Ok(dget(fc, K_HANDLE).and_then(|h| h.as_int()))
}

/// corners of the face's first-loop envelope as tessellate_geometry reads them (None when the reference's try/except yields None)
fn env_corners_tess(kk: &K, f: &Val) -> Option<(UV, UV)> {
    let fl_ = ob(dget(f, kk.k("m_pFirstLoop"))?);
    let cs = if has(fl_, kk.k("m_Envelope")) { dget(ob(dget(fl_, kk.k("m_Envelope"))?), kk.k("m_corners"))? } else { dget(dget(fl_, kk.k("m_Envelope"))?, kk.k("m_corners"))? };
    let l = list(cs).ok()?;
    if l.len() != 2 { return None; }
    Some((uv2(&l[0]).ok()?, uv2(&l[1]).ok()?))
}

fn round9(x: f64) -> f64 { format!("{:.9}", x).parse().unwrap_or(x) }

fn tri_ok(p: &Tri) -> bool { norm(sub(p[0], p[1])).min(norm(sub(p[1], p[2]))).min(norm(sub(p[0], p[2]))) >= 1e-9 }

/// rvt_mesh.tessellate_geometry over the given faces (edge map of the whole Geometry node)
pub fn tessellate(kk: &K, faces: &[&Val], edges: &EdgeMap, earcut: Earcut) -> Rs<Tess> {
    let mut out = Tess::default();
    let mut surfs: FxHashMap<Option<i128>, FaceSurf> = FxHashMap::default();
    let ksurf = kk.k("m_pSurf");
    for fc in faces {
        let f_ = ob(fc);
        if is_dict(f_) {
            if let Some(sv) = dget(f_, ksurf) {
                if truthy(sv) {
                    if let Some((k, e)) = surface_eval(kk, sv)? {
                        surfs.insert(handle_of(fc)?, FaceSurf { kind: k, ev: e, raw: sv });
                    }
                }
            }
        }
    }
    let mut nf: i64 = 0;
    let mut jobs: Vec<Job> = vec![];
    for fc in faces {
        let f = ob(fc);
        if !is_dict(f) { continue; }
        let rs = dget(f, kk.k("m_renderStyleId")).cloned().unwrap_or(Val::None);
        let sv = dget(f, ksurf).unwrap_or(&Val::None);
        let (kind, ev) = match surface_eval(kk, sv)? { Some(x) => x, None => { out.unsup.push("surface"); continue; } };
        let ev = Arc::new(ev);
        let pr = match dget(f, kk.k("m_GInfo")) {
            Some(g) if truthy(g) => match dget(g, kk.k("m_flags")) { Some(x) if truthy(x) => (int_of(x).ok_or(Raise)? & 0x80000) != 0, _ => false },
            _ => false,
        };
        let h = handle_of(fc)?;
        let (mut loops, mine0) = face_loops_uv(kk, h, edges, Some(&surfs), false)?;
        if loops.is_empty() {
            let lp = if !mine0.is_empty() { loop_via_poles(&mine0, &ev) } else { None };
            // e0, e1 = _ob(_ob(f['m_pFirstLoop'])['m_Envelope'])['m_corners']
            let env = (|| -> Option<(UV, UV)> {
                let fl_ = ob(dget(f, kk.k("m_pFirstLoop"))?);
                let l = list(dget(ob(dget(fl_, kk.k("m_Envelope"))?), kk.k("m_corners"))?).ok()?;
                if l.len() != 2 { return None; }
                Some((uv2(&l[0]).ok()?, uv2(&l[1]).ok()?))
            })();
            let (rect_a, e0, e1) = match env { Some((e0, e1)) => ((e1[0] - e0[0]) * (e1[1] - e0[1]), e0, e1), None => (0.0, [0.0; 2], [0.0; 2]) };
            let mut off_border = false;
            if rect_a > 0.0 {
                for (_, m) in &mine0 {
                    for q in std::iter::once(&m.a).chain(m.inter.iter()).chain(std::iter::once(&m.b)) {
                        if !((q[0] - e0[0]).abs() < 1e-4 || (q[0] - e1[0]).abs() < 1e-4 || (q[1] - e0[1]).abs() < 1e-4 || (q[1] - e1[1]).abs() < 1e-4) { off_border = true; }
                    }
                }
            }
            if let Some(lp) = lp { if off_border && area2(&lp).abs() > 1e-3 * rect_a { loops = vec![lp]; } }
        }
        if loops.is_empty() {
            let (lg, _) = face_loops_uv(kk, h, edges, Some(&surfs), true)?;
            let cc = env_corners_tess(kk, f);
            let mut first = false;
            if !lg.is_empty() {
                if let Some((cc0, cc1)) = cc {
                    let s: f64 = lg.iter().fold(0.0, |a, l| a + area2(l).abs());
                    if s > 0.0 {
                        let den = (cc1[0] - cc0[0]) * (cc1[1] - cc0[1]);
                        if den == 0.0 { return Err(Raise); }
                        if (s / den - 1.0).abs() < 0.03 { first = true; }
                    }
                }
            }
            if first { loops = lg; }
            else if lg.len() == 1 {
                let lp = &lg[0];
                let nn = lp.len();
                let mut npole = 0;
                for i in 0..nn {
                    let (p_, q_) = (lp[i], lp[(i + 1) % nn]);
                    if (p_[0] - q_[0]).abs() + (p_[1] - q_[1]).abs() > 1e-3 && norm(sub(ev.eval(p_[0], p_[1]), ev.eval(q_[0], q_[1]))) < 1e-7 { npole += 1; }
                }
                if npole >= 1 { loops = lg; }
            }
        }
        if loops.is_empty() {
            let mut c = env_corners_tess(kk, f);
            if let Some((c0, c1)) = c {
                if c1[0] > c0[0] && c1[1] > c0[1] {
                    let mut on_rect = true;
                    for &eid in edges.face_edges(h) {
                        let e_ = edges.map[&eid];
                        let rf = refs_of(kk, e_);
                        let sd = match rf.iter().position(|r| *r == h) { Some(s) => s, None => continue };
                        let fl_: &[Val] = match dget(e_, kk.k("m_firstAndLastEdgePnts")) { Some(x) if truthy(x) => list(x)?, _ => &[] };
                        let it: &[Val] = match dget(e_, kk.k("m_interiorEdgePnts")) { Some(x) if truthy(x) => list(x)?, _ => &[] };
                        for p_ in fl_.iter().chain(it.iter()) {
                            let po = ob(p_);
                            if !is_dict(po) || !has(po, kk.k("uv")) { continue; }
                            let [u_, v_] = pt_uv(kk, p_, sd)?;
                            if !((u_ - c0[0]).abs() < 1e-4 || (u_ - c1[0]).abs() < 1e-4 || (v_ - c0[1]).abs() < 1e-4 || (v_ - c1[1]).abs() < 1e-4) { on_rect = false; }
                        }
                    }
                    if !on_rect { c = None; }
                }
            }
            if let Some((c0, c1)) = c {
                if c1[0] > c0[0] && c1[1] > c0[1] {
                    let (_, mine_) = face_loops_uv(kk, h, edges, Some(&surfs), false)?;
                    // borders: v0, v1, u0, u1
                    let mut bd: [Vec<UV>; 4] = [vec![], vec![], vec![], vec![]];
                    for (_, m) in &mine_ {
                        let mut ep = vec![m.a];
                        ep.extend(m.inter.iter().copied());
                        ep.push(m.b);
                        let tests: [&dyn Fn(UV) -> bool; 4] = [&|q: UV| (q[1] - c0[1]).abs() < 1e-6, &|q: UV| (q[1] - c1[1]).abs() < 1e-6, &|q: UV| (q[0] - c0[0]).abs() < 1e-6, &|q: UV| (q[0] - c1[0]).abs() < 1e-6];
                        for (bi, t) in tests.iter().enumerate() { if ep.iter().all(|&q| t(q)) { bd[bi].extend(ep.iter().copied()); } }
                    }
                    bd[0].extend([[c0[0], c0[1]], [c1[0], c0[1]]]);
                    bd[1].extend([[c0[0], c1[1]], [c1[0], c1[1]]]);
                    bd[2].extend([[c0[0], c0[1]], [c0[0], c1[1]]]);
                    bd[3].extend([[c1[0], c0[1]], [c1[0], c1[1]]]);
                    let nset = |l: &Vec<UV>| { let mut s: Vec<(u64, u64)> = l.iter().map(|p| (p[0].to_bits(), p[1].to_bits())).collect(); s.sort(); s.dedup(); s.len() };
                    let mut has_real = bd.iter().any(|l| nset(l) > 2);
                    for vv_ in [c0[1], c1[1]] {
                        if norm(sub(ev.eval(c0[0], vv_), ev.eval((c0[0] + c1[0]) / 2.0, vv_))) < 1e-9 { has_real = false; }
                    }
                    let ceilmm = |x: f64, lo: f64| (64.0f64).min(lo.max(x.ceil())) as usize;
                    if !has_real {
                        let nu = ceilmm((c1[0] - c0[0]) / 0.1, 2.0);
                        let mut nv = ceilmm((c1[1] - c0[1]) / 0.2, 2.0);
                        {
                            let pa = ev.eval(c0[0], c0[1]); let pb = ev.eval(c0[0], c1[1]); let pm = ev.eval(c0[0], (c0[1] + c1[1]) / 2.0);
                            if norm(sub(pm, half(pa, pb))) < 1e-9 * (1.0f64).max(norm(sub(pb, pa))) { nv = 1; }
                        }
                        let mut us: Vec<f64> = (0..=nu).map(|i| c0[0] + (c1[0] - c0[0]) * i as f64 / nu as f64).collect();
                        let mut vs: Vec<f64> = (0..=nv).map(|j| c0[1] + (c1[1] - c0[1]) * j as f64 / nv as f64).collect();
                        let mut ue: Vec<f64> = vec![];
                        for bi in [0usize, 1] { if nset(&bd[bi]) > 2 { for q in &bd[bi] { ue.push(round9(q[0])); } } }
                        let mut ve: Vec<f64> = vec![];
                        for bi in [2usize, 3] { if nset(&bd[bi]) > 2 { for q in &bd[bi] { ve.push(round9(q[1])); } } }
                        ue.sort_by(|a, b| fcmp(a, b)); ue.dedup();
                        ve.sort_by(|a, b| fcmp(a, b)); ve.dedup();
                        if ue.len() > 2 { us = ue; }
                        if ve.len() > 2 { vs = ve; }
                        let (nu, nv) = (us.len() - 1, vs.len() - 1);
                        nf += 1;
                        out.fprio.push((nf, pr)); out.frs.push(rs.clone());
                        let mut v = vec![];
                        for i in 0..nu {
                            for j in 0..nv {
                                let (a_, b_, c_, d_) = ([us[i], vs[j]], [us[i + 1], vs[j]], [us[i + 1], vs[j + 1]], [us[i], vs[j + 1]]);
                                for tri in [[a_, b_, c_], [a_, c_, d_]] {
                                    let p = [ev.eval(tri[0][0], tri[0][1]), ev.eval(tri[1][0], tri[1][1]), ev.eval(tri[2][0], tri[2][1])];
                                    if !tri_ok(&p) { continue; }
                                    v.push((p, Meta { kind: kind.name(), ev: MetaEv::S(ev.clone()), uv: Some(tri) }, nf));
                                }
                            }
                        }
                        jobs.push(Job::Tris(v));
                        continue;
                    }
                    for bi in [0usize, 1] {
                        if nset(&bd[bi]) <= 2 {
                            let vv = if bi == 0 { c0[1] } else { c1[1] };
                            let n_ = ceilmm((c1[0] - c0[0]) / 0.1, 2.0);
                            for i in 0..=n_ { bd[bi].push([c0[0] + (c1[0] - c0[0]) * i as f64 / n_ as f64, vv]); }
                        }
                    }
                    for bi in [2usize, 3] {
                        if nset(&bd[bi]) <= 2 {
                            let uu = if bi == 2 { c0[0] } else { c1[0] };
                            let mut n_ = 1;
                            let pa = ev.eval(uu, c0[1]); let pb = ev.eval(uu, c1[1]); let pm = ev.eval(uu, (c0[1] + c1[1]) / 2.0);
                            if norm(sub(pm, half(pa, pb))) > 1e-9 * (1.0f64).max(norm(sub(pb, pa))) { n_ = ceilmm((c1[1] - c0[1]) / 0.2, 2.0); }
                            for i in 0..=n_ { bd[bi].push([uu, c0[1] + (c1[1] - c0[1]) * i as f64 / n_ as f64]); }
                        }
                    }
                    let srt = |lst: &Vec<UV>, ax: usize, rev: bool| -> Vec<UV> {
                        let mut s: Vec<UV> = lst.iter().map(|p| [round9(p[0]), round9(p[1])]).collect();
                        s.sort_by(|a, b| fcmp(&a[0], &b[0]).then(fcmp(&a[1], &b[1])));
                        s.dedup_by(|a, b| a[0] == b[0] && a[1] == b[1]);
                        s.sort_by(|a, b| { let o = fcmp(&a[ax], &b[ax]); if rev { o.reverse() } else { o } });
                        let mut o: Vec<UV> = vec![];
                        for q in s { if o.last().map_or(true, |l: &UV| (q[0] - l[0]).abs() + (q[1] - l[1]).abs() > 1e-9) { o.push(q); } }
                        o
                    };
                    let mut ring: Vec<UV> = vec![];
                    for (bi, ax, rev) in [(0usize, 0usize, false), (3, 1, false), (1, 0, true), (2, 1, true)] {
                        let mut s = srt(&bd[bi], ax, rev);
                        s.pop();
                        ring.extend(s);
                    }
                    nf += 1;
                    out.fprio.push((nf, pr)); out.frs.push(rs.clone());
                    let n = ring.len() as u32;
                    jobs.push(Job::Ear { verts: ring, ends: vec![n], nf, kind, ev: ev.clone(), drop_degenerate: true });
                    continue;
                }
            }
        }
        if loops.is_empty() { out.unsup.push("noloops"); continue; }
        nf += 1;
        out.fprio.push((nf, pr)); out.frs.push(rs.clone());
        for (outer, holes) in group_loops(&loops) {
            let mut verts = nudge_repeats(&outer);
            let mut ends = vec![verts.len() as u32];
            for hl in holes { verts.extend(hl); ends.push(verts.len() as u32); }
            jobs.push(Job::Ear { verts, ends, nf, kind, ev: ev.clone(), drop_degenerate: false });
        }
    }
    out.faces = nf;
    // one batch triangulation for the node, then emit in face order
    let reqs: Vec<(&[UV], &[u32])> = jobs.iter().filter_map(|j| match j { Job::Ear { verts, ends, .. } => Some((verts.as_slice(), ends.as_slice())), _ => None }).collect();
    let res = if reqs.is_empty() { vec![] } else { earcut(&reqs) };
    let mut ri = 0;
    for j in jobs {
        match j {
            Job::Tris(v) => { for (t, m, f) in v { out.tris.push(t); out.meta.push(m); out.fidx.push(f); } }
            Job::Ear { mut verts, ends, nf, kind, ev, drop_degenerate } => {
                let r = res.get(ri).cloned().flatten();
                ri += 1;
                let mut idx = match r { Some(i) => i, None => { out.unsup.push("earcut"); continue; } };
                // curved face (general loop path, single loop): the emitted triangles must follow the surface (reference: _rect_grid / refine_curved)
                if !drop_degenerate && kind.name() != "plane" && ends.len() == 1 && std::env::var_os("RVT_NOREFINE").map_or(true, |x| x.is_empty()) {
                    let nocol = std::env::var_os("RVT_NOCOLGRID").map_or(false, |x| !x.is_empty());
                    let kn = kind.name();
                    let mut g = rect_grid(&verts, &ev, 48);
                    if g.is_none() && !nocol && (kn == "ruled" || kn == "hermite") { g = col_grid(&verts, &ev, 1500); }
                    if let Some((gv, gt)) = g.take() {
                        g = Some(if kn == "ruled" && gt.len() <= 2 && !nocol { subdivide_grid(gv, gt, &ev, refine_rel(), 0.0033, 6000, 5) } else { (gv, gt) });
                    }
                    let (nv, tl) = match g { Some(g) => g, None => refine_curved(&verts, &idx, &ev, refine_rel(), 0.0033, 2000) };
                    verts = nv;
                    idx = tl.iter().flat_map(|x| [x[0] as u32, x[1] as u32, x[2] as u32]).collect();
                }
                let p: Vec<V3> = verts.iter().map(|q| ev.eval(q[0], q[1])).collect();
                for k in (0..idx.len() / 3 * 3).step_by(3) {
                    let (i0, i1, i2) = (idx[k] as usize, idx[k + 1] as usize, idx[k + 2] as usize);
                    if i0 >= p.len() || i1 >= p.len() || i2 >= p.len() { return Err(Raise); }
                    let pt = [p[i0], p[i1], p[i2]];
                    if drop_degenerate && !tri_ok(&pt) { continue; }
                    out.tris.push(pt);
                    out.meta.push(Meta { kind: kind.name(), ev: MetaEv::S(ev.clone()), uv: Some([verts[i0], verts[i1], verts[i2]]) });
                    out.fidx.push(nf);
                }
            }
        }
    }
    Ok(out)
}

/// (pa + pb) / 2 as numpy computes it (sum, then divide)
fn half(a: V3, b: V3) -> V3 { let s = add(a, b); [s[0] / 2.0, s[1] / 2.0, s[2] / 2.0] }

/// rvt_mesh.element_faces_in_bbox
pub fn faces_in_bbox(kk: &K, geo: &Val, bb: (V3, V3), eps: f64, earcut: Earcut) -> Rs<Tess> {
    let faces: &[Val] = match dget(geo, kk.k("m_pFaces")) { Some(x) if truthy(x) => list(x)?, _ => &[] };
    let edges = edge_map(kk, geo);
    let lo = [bb.0[0] - eps, bb.0[1] - eps, bb.0[2] - eps];
    let hi = [bb.1[0] + eps, bb.1[1] + eps, bb.1[2] + eps];
    let mut keep: Vec<&Val> = vec![];
    for fc in faces {
        let f = ob(fc);
        if !is_dict(f) { continue; }
        let (_, ev) = match surface_eval(kk, dget(f, kk.k("m_pSurf")).unwrap_or(&Val::None))? { Some(x) => x, None => continue };
        let (loops, _) = face_loops_uv(kk, handle_of(fc)?, &edges, None, false)?;
        if loops.is_empty() {
            // no ordinary loop (edgeless / pole / trimmed faces): judge the face by its own tessellation
            let tt = tessellate(kk, &[fc], &edges, earcut)?;
            if tt.tris.is_empty() { continue; }
            let mut mn = tt.tris[0][0]; let mut mx = tt.tris[0][0];
            for t in &tt.tris { for p in t { for k in 0..3 { mn[k] = mn[k].min(p[k]); mx[k] = mx[k].max(p[k]); } } }
            if (0..3).all(|k| mn[k] >= lo[k]) && (0..3).all(|k| mx[k] <= hi[k]) { keep.push(fc); }
            continue;
        }
        let mut best = 0;
        let mut ba = f64::NEG_INFINITY;
        for (i, l) in loops.iter().enumerate() { let a = area2(l).abs(); if a > ba { ba = a; best = i; } }
        let pts: Vec<V3> = loops[best].iter().map(|q| ev.eval(q[0], q[1])).collect();
        if pts.is_empty() { return Err(Raise); }
        let mut mn = pts[0]; let mut mx = pts[0];
        for p in &pts { for k in 0..3 { mn[k] = mn[k].min(p[k]); mx[k] = mx[k].max(p[k]); } }
        if (0..3).all(|k| mn[k] >= lo[k]) && (0..3).all(|k| mx[k] <= hi[k]) { keep.push(fc); }
    }
    tessellate(kk, &keep, &edges, earcut)
}

/// rvt_mesh.tessellate_geometry(geo) on a whole Geometry node
pub fn tessellate_geometry(kk: &K, geo: &Val, earcut: Earcut) -> Rs<Tess> {
    let faces: Vec<&Val> = match dget(geo, kk.k("m_pFaces")) { Some(x) if truthy(x) => list(x)?.iter().collect(), _ => vec![] };
    let edges = edge_map(kk, geo);
    tessellate(kk, &faces, &edges, earcut)
}

/// rvt_mesh.polymesh_tris
pub fn polymesh_tris(kk: &K, node: &Val) -> Rs<Vec<Tri>> {
    let ft = match dget(node, kk.k("m_pFacetedTopology")) { Some(x) if truthy(x) => ob(x), _ => return Ok(vec![]) };
    if !is_dict(ft) { return Ok(vec![]); }
    let (p, f) = match (dget(ft, kk.k("m_pointsArr")), dget(ft, kk.k("m_facetsArr"))) { (Some(p), Some(f)) if truthy(p) && truthy(f) => (p, f), _ => return Ok(vec![]) };
    let pts: Vec<V3> = list(p)?.iter().map(v3).collect::<Rs<_>>()?;
    let n = pts.len() as i128;
    let at = |i: &Val| -> Rs<V3> { let mut k = int_of(i).ok_or(Raise)?; if k < 0 { k += n; } if k < 0 || k >= n { return Err(Raise); } Ok(pts[k as usize]) };
    let mut out = vec![];
    for fa in list(f)? {
        let fa = list(fa)?;
        if fa.len() == 3 { out.push([at(&fa[0])?, at(&fa[1])?, at(&fa[2])?]); }
        else if fa.len() == 4 { out.push([at(&fa[0])?, at(&fa[1])?, at(&fa[2])?]); out.push([at(&fa[0])?, at(&fa[2])?, at(&fa[3])?]); }
        else if fa.len() > 4 { for k in 1..fa.len() - 1 { out.push([at(&fa[0])?, at(&fa[k])?, at(&fa[k + 1])?]); } }
    }
    Ok(out)
}

// ---------------- closed shell selection ----------------
type Key3 = (i64, i64, i64);
#[inline] fn key3(p: V3, tol: f64) -> Key3 { (pyround(p[0] / tol), pyround(p[1] / tol), pyround(p[2] / tol)) }
#[inline] fn ekey(a: Key3, b: Key3) -> (Key3, Key3) { if a <= b { (a, b) } else { (b, a) } }

/// rvt_mesh.tjunction_closed
pub fn tjunction_closed(tris: &[Tri], tol: f64) -> bool {
    let mut pts: FxHashMap<Key3, V3> = FxHashMap::default();
    let mut ec_order: Vec<(Key3, Key3)> = vec![];
    let mut ec: FxHashMap<(Key3, Key3), usize> = FxHashMap::default();
    for t in tris {
        let ks = [key3(t[0], tol), key3(t[1], tol), key3(t[2], tol)];
        for i in 0..3 { pts.insert(ks[i], t[i]); }
        if ks[0] == ks[1] || ks[1] == ks[2] || ks[0] == ks[2] { continue; }
        for i in 0..3 {
            let e = ekey(ks[i], ks[(i + 1) % 3]);
            let c = ec.entry(e).or_insert(0);
            if *c == 0 { ec_order.push(e); }
            *c += 1;
        }
    }
    let bd: Vec<(Key3, Key3)> = ec_order.iter().filter(|e| ec[e] % 2 == 1).copied().collect();
    if bd.is_empty() { return true; }
    let mut vs: Vec<Key3> = bd.iter().flat_map(|e| [e.0, e.1]).collect();
    vs.sort();
    vs.dedup();
    let p: Vec<V3> = vs.iter().map(|k| pts[k]).collect();
    let vidx: FxHashMap<Key3, usize> = vs.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut o_order: Vec<(usize, usize)> = vec![];
    let mut out: FxHashMap<(usize, usize), usize> = FxHashMap::default();
    let thr = (tol * 4.0).max(1e-6);
    for (a, b) in &bd {
        let (ia, ib) = (vidx[a], vidx[b]);
        let (pa, pb) = (p[ia], p[ib]);
        let d = sub(pb, pa);
        let l2 = dot(d, d);
        if l2 < 1e-18 { continue; }
        let mut sel: Vec<(f64, usize)> = vec![];
        for (i, q) in p.iter().enumerate() {
            let t = dot(sub(*q, pa), d) / l2;
            let qq = add(pa, mul(t, d));
            let dist = norm(sub(*q, qq));
            if dist < thr && t > 1e-6 && t < 1.0 - 1e-6 { sel.push((t, i)); }
        }
        sel.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut chain = vec![ia];
        chain.extend(sel.iter().map(|x| x.1));
        chain.push(ib);
        for w in chain.windows(2) {
            let e = (w[0].min(w[1]), w[0].max(w[1]));
            let c = out.entry(e).or_insert(0);
            if *c == 0 { o_order.push(e); }
            *c += 1;
        }
    }
    if out.values().all(|c| c % 2 == 0) { return true; }
    let near = 4e-3;
    let segs: Vec<(V3, V3)> = o_order.iter().filter(|e| out[e] % 2 == 1).map(|(a, b)| (p[*a], p[*b])).collect();
    if segs.len() > 2000 { return false; }
    for (i, (pp, qq)) in segs.iter().enumerate() {
        let mid = { let s = add(*pp, *qq); [s[0] / 2.0, s[1] / 2.0, s[2] / 2.0] };
        for pt in [*pp, *qq, mid] {
            let mut dmin = f64::INFINITY;
            for (j, (a, b)) in segs.iter().enumerate() {
                let d = sub(*b, *a);
                let l2 = dot(d, d).max(1e-18);
                let t = (dot(sub(pt, *a), d) / l2).clamp(0.0, 1.0);
                let dist = if j == i { 1e9 } else { norm(sub(add(*a, mul(t, d)), pt)) };
                if dist < dmin { dmin = dist; }
            }
            if dmin > near { return false; }
        }
    }
    true
}

type EKey = (Key3, Key3);

/// rvt_mesh.select_closed_shell -> (tris, meta, closed, fidx of the kept triangles when closed)
pub fn select_closed_shell(tris: &[Tri], meta: &[Meta], fidx: &[i64], prio: Option<&FxHashMap<i64, bool>>) -> (Vec<Tri>, Vec<Meta>, bool, Option<Vec<i64>>) {
    let tol = 5e-5;
    if tris.is_empty() { return (tris.to_vec(), meta.to_vec(), false, None); }
    // faces in first-appearance order
    let mut forder: Vec<i64> = vec![];
    let mut faces: FxHashMap<i64, Vec<usize>> = FxHashMap::default();
    for (i, f) in fidx.iter().enumerate().take(tris.len().min(meta.len())) {
        let e = faces.entry(*f).or_default();
        if e.is_empty() { forder.push(*f); }
        e.push(i);
    }
    let mut keep_order: Vec<i64> = vec![];
    let mut seen: FxHashSet<(String, Vec<Key3>)> = FxHashSet::default();
    for f in &forder {
        let lst = &faces[f];
        let mut area = 0.0;
        for &i in lst { let t = &tris[i]; area += norm(cross(sub(t[1], t[0]), sub(t[2], t[0]))) / 2.0; }
        if area < 1e-9 { continue; }
        let mut ks: Vec<Key3> = lst.iter().flat_map(|&i| tris[i].iter().map(|p| key3(*p, tol)).collect::<Vec<_>>()).collect();
        ks.sort();
        ks.dedup();
        ks.truncate(6);
        let sig = (format!("{:.6}", area), ks);
        if !seen.insert(sig) { continue; }
        keep_order.push(*f);
    }
    // face boundary edges (occurring once inside the face's triangulation)
    let mut fedges: FxHashMap<i64, FxHashSet<EKey>> = FxHashMap::default();
    for f in &keep_order {
        let mut cnt: FxHashMap<EKey, usize> = FxHashMap::default();
        for &i in &faces[f] {
            let ks = [key3(tris[i][0], tol), key3(tris[i][1], tol), key3(tris[i][2], tol)];
            for (a, b) in [(0, 1), (1, 2), (2, 0)] { if ks[a] != ks[b] { *cnt.entry(ekey(ks[a], ks[b])).or_insert(0) += 1; } }
        }
        fedges.insert(*f, cnt.into_iter().filter(|(_, n)| *n == 1).map(|(e, _)| e).collect());
    }
    let mut allv: Vec<Key3> = fedges.values().flat_map(|es| es.iter().flat_map(|e| [e.0, e.1])).collect();
    allv.sort();
    allv.dedup();
    let nedges: usize = fedges.values().map(|s| s.len()).sum();
    if !allv.is_empty() && nedges * allv.len() <= 3_000_000 {
        let pv: Vec<V3> = allv.iter().map(|k| [k.0 as f64 * tol, k.1 as f64 * tol, k.2 as f64 * tol]).collect();
        let vi: FxHashMap<Key3, usize> = allv.iter().enumerate().map(|(i, k)| (*k, i)).collect();
        for f in &keep_order {
            let es: Vec<EKey> = fedges[f].iter().copied().collect();
            let mut new: FxHashSet<EKey> = FxHashSet::default();
            for e in es {
                let (a, b) = e;
                let (pa, pb) = (pv[vi[&a]], pv[vi[&b]]);
                let d = sub(pb, pa);
                let l2 = dot(d, d);
                if l2 < 1e-18 { new.insert(e); continue; }
                let mut sel: Vec<(f64, usize)> = vec![];
                for (i, q) in pv.iter().enumerate() {
                    let t = dot(sub(*q, pa), d) / l2;
                    let dist = norm(sub(*q, add(pa, mul(t, d))));
                    if dist < 4.0 * tol && t > 1e-6 && t < 1.0 - 1e-6 { sel.push((t, i)); }
                }
                sel.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
                let mut chain = vec![vi[&a]];
                chain.extend(sel.iter().map(|x| x.1));
                chain.push(vi[&b]);
                for w in chain.windows(2) { new.insert(ekey(allv[w[0]], allv[w[1]])); }
            }
            fedges.insert(*f, new);
        }
    }
    let count = |kp: &[i64]| -> FxHashMap<EKey, usize> {
        let mut c: FxHashMap<EKey, usize> = FxHashMap::default();
        for f in kp { for e in &fedges[f] { *c.entry(*e).or_insert(0) += 1; } }
        c
    };
    let n_open = |kp: &[i64]| -> usize { count(kp).values().filter(|n| **n == 1).count() };
    let peel = |kp: &[i64]| -> Vec<i64> {
        let mut kp = kp.to_vec();
        let mut ch = true;
        while ch && !kp.is_empty() {
            ch = false;
            let cnt = count(&kp);
            let before = kp.len();
            kp.retain(|f| !fedges[f].iter().any(|e| cnt[e] == 1));
            if kp.len() != before { ch = true; }
        }
        kp
    };
    let mut kp0: Option<Vec<i64>> = None;
    if let Some(pr) = prio {
        if !pr.is_empty() {
            let pk: Vec<i64> = keep_order.iter().filter(|f| pr.get(f).copied().unwrap_or(false)).copied().collect();
            let pk_tris = || -> Vec<Tri> { pk.iter().flat_map(|f| faces[f].iter().map(|&i| tris[i])).collect() };
            if !pk.is_empty() && tjunction_closed(&pk_tris(), tol) { kp0 = Some(pk.clone()); }
            else if !pk.is_empty() {
                let mut cur = pk.clone();
                let mut best = n_open(&cur);
                while best > 0 {
                    let cnt = count(&cur);
                    let opn: FxHashSet<EKey> = cnt.iter().filter(|(_, n)| **n == 1).map(|(e, _)| *e).collect();
                    let cands: Vec<i64> = keep_order.iter().filter(|f| !cur.contains(f) && fedges[f].iter().any(|e| opn.contains(e))).copied().collect();
                    if cands.is_empty() { break; }
                    let mut scored: Vec<(usize, i64)> = cands.iter().map(|f| { let mut c2 = cur.clone(); c2.push(*f); (n_open(&c2), *f) }).collect();
                    scored.sort();
                    if scored[0].0 >= best { break; }
                    best = scored[0].0;
                    cur.push(scored[0].1);
                }
                if best == 0 && !cur.is_empty() { kp0 = Some(cur); }
            }
        }
    }
    let n_all = keep_order.len();
    let open_all = n_open(&keep_order);
    let n_edges_all = { let mut s: FxHashSet<EKey> = FxHashSet::default(); for f in &keep_order { s.extend(fedges[f].iter().copied()); } s.len() };
    let has_kp0 = kp0.is_some();
    let keep = match kp0 { Some(k) => k, None => peel(&keep_order) };
    if keep.is_empty() { return (tris.to_vec(), meta.to_vec(), false, None); }
    if !has_kp0 && (keep.len() as f64) < 0.5 * n_all as f64 && open_all as f64 <= 0.05 * (n_edges_all.max(1) as f64) { return (tris.to_vec(), meta.to_vec(), false, None); }
    let cnt = count(&keep);
    let closed = cnt.values().all(|n| *n == 2) || has_kp0;
    if !closed { return (tris.to_vec(), meta.to_vec(), false, None); }
    let mut ot = vec![]; let mut om = vec![]; let mut of = vec![];
    for f in &keep { for &i in &faces[f] { ot.push(tris[i]); om.push(meta[i].clone()); of.push(*f); } }
    (ot, om, true, Some(of))
}

/// rvt_mesh.mesh_volume(tris)[2]: unmatched edges (edge used by != 2 triangles after welding at tol), 0 when the winding is already consistent with a positive volume
pub fn mesh_open_edges(tris: &[Tri], tol: f64) -> usize {
    if tris.is_empty() { return 0; }
    let mut vid: FxHashMap<Key3, usize> = FxHashMap::default();
    let mut vp: Vec<V3> = vec![];
    let mut tt: Vec<[usize; 3]> = vec![];
    for t in tris {
        let mut ids = [0usize; 3];
        for i in 0..3 { let k = key3(t[i], tol); let n = vid.len(); ids[i] = *vid.entry(k).or_insert_with(|| { vp.push(t[i]); n }); }
        if ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2] { tt.push(ids); }
    }
    // V[vid[key(p)]] = p: the last point with that key wins
    for t in tris { for p in t { vp[vid[&key3(*p, tol)]] = *p; } }
    let mut em: FxHashMap<(usize, usize), usize> = FxHashMap::default();
    let mut de: FxHashMap<(usize, usize), usize> = FxHashMap::default();
    for t in &tt {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] { *em.entry((a.min(b), a.max(b))).or_insert(0) += 1; *de.entry((a, b)).or_insert(0) += 1; }
    }
    if !tt.is_empty() && de.iter().all(|((a, b), c)| de.get(&(*b, *a)).copied().unwrap_or(0) == *c) {
        let v: Vec<f64> = tt.iter().map(|t| dot(vp[t[0]], cross(vp[t[1]], vp[t[2]]))).collect();
        if np_sum(&v) / 6.0 > 0.0 { return 0; }
    }
    em.values().filter(|n| **n != 2).count()
}

/// rvt_mesh.nearly_closed(tris, frac, min_tris)
pub fn nearly_closed(tris: &[Tri], frac: f64, min_tris: usize) -> bool {
    if tris.len() < min_tris { return false; }
    if tjunction_closed(tris, 5e-5) { return true; }
    (mesh_open_edges(tris, 5e-5) as f64) <= frac * 1.5 * tris.len() as f64
}

/// total order on f64 for sorts: identical to partial_cmp on non-NaN values, NaN after everything (a NaN in decoded geometry must not panic the sort)
#[inline]
fn fcmp(a: &f64, b: &f64) -> std::cmp::Ordering { a.partial_cmp(b).unwrap_or_else(|| a.is_nan().cmp(&b.is_nan())) }

/// Python round(x, 8) (correctly rounded decimal, parsed back)
fn round8(x: f64) -> f64 { format!("{:.8}", x).parse::<f64>().unwrap_or(x) }

/// rvt_mesh._rect_grid: a curved face whose uv loop is its uv rectangle -> structured grid through the loop's own border samples; None otherwise
pub fn rect_grid(verts: &[UV], ev: &Surf, max_n: usize) -> Option<(Vec<UV>, Vec<[usize; 3]>)> {
    if verts.is_empty() { return None; }
    let (mut u0, mut v0, mut u1, mut v1) = (verts[0][0], verts[0][1], verts[0][0], verts[0][1]);
    for q in verts { u0 = u0.min(q[0]); v0 = v0.min(q[1]); u1 = u1.max(q[0]); v1 = v1.max(q[1]); }
    if u1 - u0 < 1e-9 || v1 - v0 < 1e-9 { return None; }
    let tb = 1e-6 * (1.0f64).max((u1 - u0).abs()).max((v1 - v0).abs());
    if !verts.iter().all(|q| (q[0] - u0).abs() < tb || (q[0] - u1).abs() < tb || (q[1] - v0).abs() < tb || (q[1] - v1).abs() < tb) { return None; }
    let ar = area2(verts);
    let rect = (u1 - u0) * (v1 - v0);
    if (ar.abs() - rect).abs() > 1e-6 * rect + 1e-12 { return None; }
    let set = |it: Vec<f64>| -> Vec<f64> { let mut v = it; v.sort_by(|a, b| fcmp(a, b)); v.dedup_by(|a, b| a.to_bits() == b.to_bits() || *a == *b); v };
    let ub0 = set(verts.iter().filter(|q| (q[1] - v0).abs() < tb).map(|q| round8(q[0])).chain([round8(u0), round8(u1)]).collect());
    let ub1 = set(verts.iter().filter(|q| (q[1] - v1).abs() < tb).map(|q| round8(q[0])).chain([round8(u0), round8(u1)]).collect());
    let vb0 = set(verts.iter().filter(|q| (q[0] - u0).abs() < tb).map(|q| round8(q[1])).chain([round8(v0), round8(v1)]).collect());
    let vb1 = set(verts.iter().filter(|q| (q[0] - u1).abs() < tb).map(|q| round8(q[1])).chain([round8(v0), round8(v1)]).collect());
    if vb0 != vb1 { return None; }
    let thin = |l: &[f64]| -> Vec<f64> { let mut o = vec![l[0]]; for &x in &l[1..] { if x - o[o.len() - 1] > 1e-9 { o.push(x); } } o };
    let (a0, a1, vs) = (thin(&ub0), thin(&ub1), thin(&vb0));
    let nv = vs.len() - 1;
    if nv < 1 || nv > max_n || a0.len() - 1 > 4 * max_n || a1.len() - 1 > 4 * max_n || nv * (a0.len() + a1.len()) > 60000 { return None; }
    let pos = ar > 0.0;
    let mut vv: Vec<UV> = vec![];
    let mut rows: Vec<Vec<usize>> = vec![];
    for (jj, &v) in vs.iter().enumerate() {
        let mut a = if jj * 2 < nv || jj == 0 { &a0 } else { &a1 };
        if jj == nv { a = &a1; }
        rows.push((0..a.len()).map(|k| vv.len() + k).collect());
        for &u in a.iter() { vv.push([u, v]); }
    }
    let mut t: Vec<[usize; 3]> = vec![];
    for jj in 0..nv {
        let (r0, r1) = (&rows[jj], &rows[jj + 1]);
        let (mut i0, mut i1) = (0usize, 0usize);
        while i0 < r0.len() - 1 || i1 < r1.len() - 1 {
            let adv0 = i1 >= r1.len() - 1 || (i0 < r0.len() - 1 && vv[r0[i0 + 1]][0] <= vv[r1[i1 + 1]][0]);
            let tr = if adv0 { let x = [r0[i0], r0[i0 + 1], r1[i1]]; i0 += 1; x } else { let x = [r0[i0], r1[i1 + 1], r1[i1]]; i1 += 1; x };
            t.push(if pos { tr } else { [tr[0], tr[2], tr[1]] });
        }
    }
    let p: Vec<V3> = vv.iter().map(|q| ev.eval(q[0], q[1])).collect();
    t.retain(|x| norm(sub(p[x[0]], p[x[1]])).min(norm(sub(p[x[1]], p[x[2]]))).min(norm(sub(p[x[0]], p[x[2]]))) > 1e-9);
    Some((vv, t))
}

/// rvt_mesh._col_grid (2026-10-17 final rules): rectangle loop whose two u-borders carry different v samples and whose v-borders carry the same samples ->
/// columns zipped between the u-border rows (interior columns: union of both). None otherwise.
pub fn col_grid(verts: &[UV], ev: &Surf, max_rows: usize) -> Option<(Vec<UV>, Vec<[usize; 3]>)> {
    if verts.is_empty() { return None; }
    let (mut u0, mut v0, mut u1, mut v1) = (verts[0][0], verts[0][1], verts[0][0], verts[0][1]);
    for q in verts { u0 = u0.min(q[0]); v0 = v0.min(q[1]); u1 = u1.max(q[0]); v1 = v1.max(q[1]); }
    if u1 - u0 < 1e-9 || v1 - v0 < 1e-9 { return None; }
    let tb = 1e-6 * (1.0f64).max((u1 - u0).abs()).max((v1 - v0).abs());
    if !verts.iter().all(|q| (q[0] - u0).abs() < tb || (q[0] - u1).abs() < tb || (q[1] - v0).abs() < tb || (q[1] - v1).abs() < tb) { return None; }
    let ar = area2(verts);
    let rect = (u1 - u0) * (v1 - v0);
    if (ar.abs() - rect).abs() > 1e-6 * rect + 1e-12 { return None; }
    let set = |it: Vec<f64>| -> Vec<f64> { let mut v = it; v.sort_by(|a, b| fcmp(a, b)); v.dedup_by(|a, b| *a == *b); v };
    let a0 = set(verts.iter().filter(|q| (q[1] - v0).abs() < tb).map(|q| round8(q[0])).chain([round8(u0), round8(u1)]).collect());
    let a1 = set(verts.iter().filter(|q| (q[1] - v1).abs() < tb).map(|q| round8(q[0])).chain([round8(u0), round8(u1)]).collect());
    if a0 != a1 { return None; }
    let l = set(verts.iter().filter(|q| (q[0] - u0).abs() < tb).map(|q| round8(q[1])).chain([round8(v0), round8(v1)]).collect());
    let r = set(verts.iter().filter(|q| (q[0] - u1).abs() < tb).map(|q| round8(q[1])).chain([round8(v0), round8(v1)]).collect());
    let mut mid: Vec<f64> = vec![];
    for x in set(l.iter().chain(r.iter()).copied().collect()) { if mid.is_empty() || x - mid[mid.len() - 1] > 1e-9 { mid.push(x); } }
    if mid.len() > max_rows || a0.len() * mid.len() > 20000 { return None; }
    let n = a0.len();
    let pos = ar > 0.0;
    let mut vv: Vec<UV> = vec![];
    let mut ids: Vec<Vec<usize>> = vec![];
    for (k, &u) in a0.iter().enumerate() {
        let rows: &Vec<f64> = if k == 0 { &l } else if k == n - 1 { &r } else { &mid };
        ids.push((0..rows.len()).map(|q| vv.len() + q).collect());
        for &v in rows.iter() { vv.push([u, v]); }
    }
    let mut t: Vec<[usize; 3]> = vec![];
    for k in 0..n.saturating_sub(1) {
        let (a, b) = (&ids[k], &ids[k + 1]);
        let (mut i, mut j) = (0usize, 0usize);
        while i < a.len() - 1 || j < b.len() - 1 {
            let advl = j >= b.len() - 1 || (i < a.len() - 1 && vv[a[i + 1]][1] <= vv[b[j + 1]][1]);
            let tr = if advl { let x = [a[i], b[j], a[i + 1]]; i += 1; x } else { let x = [a[i], b[j], b[j + 1]]; j += 1; x };
            t.push(if pos { tr } else { [tr[0], tr[2], tr[1]] });
        }
    }
    let p: Vec<V3> = vv.iter().map(|q| ev.eval(q[0], q[1])).collect();
    t.retain(|x| norm(sub(p[x[0]], p[x[1]])).min(norm(sub(p[x[1]], p[x[2]]))).min(norm(sub(p[x[0]], p[x[2]]))) > 1e-9);
    Some((vv, t))
}

/// rvt_mesh._subdivide_grid: 1->4 midpoint split of a (ruled) grid's triangles until the interior edge midpoints are within tol
pub fn subdivide_grid(v0: Vec<UV>, t0: Vec<[usize; 3]>, ev: &Surf, rel_tol: f64, abs_tol: f64, max_tris: usize, max_depth: usize) -> (Vec<UV>, Vec<[usize; 3]>) {
    let mut v = v0;
    let mut t = t0;
    if t.is_empty() { return (v, t); }
    let mut p: Vec<V3> = v.iter().map(|q| ev.eval(q[0], q[1])).collect();
    let (mut lo, mut hi) = (p[0], p[0]);
    for q in &p { for k in 0..3 { lo[k] = lo[k].min(q[k]); hi[k] = hi[k].max(q[k]); } }
    let mut tol = abs_tol.max(rel_tol * norm(sub(hi, lo)));
    let ek = |a: usize, b: usize| -> (usize, usize) { (a.min(b), a.max(b)) };
    let edges_of = |t: &Vec<[usize; 3]>| -> FxHashMap<(usize, usize), usize> {
        let mut em: FxHashMap<(usize, usize), usize> = FxHashMap::default();
        for tr in t { for k in 0..3 { *em.entry(ek(tr[k], tr[(k + 1) % 3])).or_insert(0) += 1; } }
        em
    };
    let dev = |v: &Vec<UV>, p: &Vec<V3>, i: usize, j: usize| -> f64 {
        let m = [(v[i][0] + v[j][0]) / 2.0, (v[i][1] + v[j][1]) / 2.0];
        let c = add(p[i], p[j]);
        norm(sub(ev.eval(m[0], m[1]), [c[0] / 2.0, c[1] / 2.0, c[2] / 2.0]))
    };
    let em = edges_of(&t);
    let mut bdev = 0.0f64;
    for (e, c) in &em { if *c == 1 { bdev = bdev.max(dev(&v, &p, e.0, e.1)); } }
    tol = tol.max(bdev);
    for _ in 0..max_depth {
        let em = edges_of(&t);
        if t.len() * 4 > max_tris { break; }
        let interior: Vec<&(usize, usize)> = em.iter().filter(|(_, c)| **c == 2).map(|(e, _)| e).collect();
        if interior.is_empty() || interior.iter().map(|e| dev(&v, &p, e.0, e.1)).fold(0.0f64, f64::max) <= tol { break; }
        let mut mid: FxHashMap<(usize, usize), usize> = FxHashMap::default();
        let mut mp = |v: &mut Vec<UV>, p: &mut Vec<V3>, i: usize, j: usize| -> usize {
            let k = ek(i, j);
            if let Some(&x) = mid.get(&k) { return x; }
            let m = [(v[i][0] + v[j][0]) / 2.0, (v[i][1] + v[j][1]) / 2.0];
            v.push(m); p.push(ev.eval(m[0], m[1]));
            mid.insert(k, v.len() - 1);
            v.len() - 1
        };
        let mut nt: Vec<[usize; 3]> = Vec::with_capacity(t.len() * 4);
        for &[a, b, c] in &t {
            let (ab, bc, ca) = (mp(&mut v, &mut p, a, b), mp(&mut v, &mut p, b, c), mp(&mut v, &mut p, c, a));
            nt.push([a, ab, ca]); nt.push([ab, b, bc]); nt.push([ca, bc, c]); nt.push([ab, bc, ca]);
        }
        t = nt;
    }
    (v, t)
}

/// rvt_mesh.refine_curved: conforming refinement of an earcut triangulation of a curved face (interior edges bisected where the chord leaves the surface,
/// loop-only triangles get their centroid); loop never changed
pub fn refine_curved(verts: &[UV], idx: &[u32], ev: &Surf, rel_tol: f64, abs_tol: f64, max_tris: usize) -> (Vec<UV>, Vec<[usize; 3]>) {
    let mut v: Vec<UV> = verts.to_vec();
    let mut t: Vec<[usize; 3]> = (0..idx.len() / 3).map(|k| [idx[3 * k] as usize, idx[3 * k + 1] as usize, idx[3 * k + 2] as usize]).collect();
    if t.is_empty() { return (v, t); }
    let mut p: Vec<V3> = v.iter().map(|q| ev.eval(q[0], q[1])).collect();
    let (mut lo, mut hi) = (p[0], p[0]);
    for q in &p { for k in 0..3 { lo[k] = lo[k].min(q[k]); hi[k] = hi[k].max(q[k]); } }
    let mut tol = abs_tol.max(rel_tol * norm(sub(hi, lo)));
    let nloop = v.len();
    // the loop's own chord error: refining the interior beyond it adds triangles, not fidelity
    let mut bdev = 0.0f64;
    for i in 0..nloop {
        let j = (i + 1) % nloop;
        let m = [(v[i][0] + v[j][0]) / 2.0, (v[i][1] + v[j][1]) / 2.0];
        let c = add(p[i], p[j]);
        bdev = bdev.max(norm(sub(ev.eval(m[0], m[1]), [c[0] / 2.0, c[1] / 2.0, c[2] / 2.0])));
    }
    tol = tol.max(bdev);
    let ek = |a: usize, b: usize| -> (usize, usize) { (a.min(b), a.max(b)) };
    let bnd: FxHashSet<(usize, usize)> = (0..nloop).map(|i| ek(i, (i + 1) % nloop)).collect();
    let dev = |v: &Vec<UV>, p: &Vec<V3>, i: usize, j: usize| -> f64 {
        let m = [(v[i][0] + v[j][0]) / 2.0, (v[i][1] + v[j][1]) / 2.0];
        let c = add(p[i], p[j]);
        norm(sub(ev.eval(m[0], m[1]), [c[0] / 2.0, c[1] / 2.0, c[2] / 2.0]))
    };
    // 1) edge flips (no new vertices): an interior diagonal of a uv-convex quad is replaced by the other diagonal when that one deviates less
    let cr = |v: &Vec<UV>, o: usize, a: usize, b: usize| -> f64 { (v[a][0] - v[o][0]) * (v[b][1] - v[o][1]) - (v[a][1] - v[o][1]) * (v[b][0] - v[o][0]) };
    for _fp in 0..8 {
        let mut eorder: Vec<(usize, usize)> = vec![];
        let mut em: FxHashMap<(usize, usize), Vec<usize>> = FxHashMap::default();
        for (ti, tr) in t.iter().enumerate() {
            for k in 0..3 {
                let e = ek(tr[k], tr[(k + 1) % 3]);
                let l = em.entry(e).or_default();
                if l.is_empty() { eorder.push(e); }
                l.push(ti);
            }
        }
        let mut flipped = 0;
        let mut used: FxHashSet<usize> = FxHashSet::default();
        for e in &eorder {
            let ts = &em[e];
            if bnd.contains(e) || ts.len() != 2 || used.contains(&ts[0]) || used.contains(&ts[1]) { continue; }
            let (i, j) = *e;
            let (t1, t2) = (t[ts[0]], t[ts[1]]);
            let cs: Vec<usize> = t1.iter().copied().filter(|q| *q != i && *q != j).collect();
            let ds: Vec<usize> = t2.iter().copied().filter(|q| *q != i && *q != j).collect();
            if cs.len() != 1 || ds.len() != 1 { continue; }
            let (c, d) = (cs[0], ds[0]);
            if em.contains_key(&ek(c, d)) { continue; }
            if cr(&v, i, j, c) * cr(&v, i, j, d) >= 0.0 || cr(&v, c, d, i) * cr(&v, c, d, j) >= 0.0 { continue; }
            if dev(&v, &p, c, d) + 1e-12 >= dev(&v, &p, i, j) { continue; }
            let [mut a, mut b, mut cc] = t1;
            for _ in 0..3 { if (a == i && b == j) || (a == j && b == i) { break; } let o = a; a = b; b = cc; cc = o; }
            t[ts[0]] = [a, d, cc]; t[ts[1]] = [d, b, cc];
            used.insert(ts[0]); used.insert(ts[1]); flipped += 1;
        }
        if flipped == 0 { break; }
    }
    for _pass in 0..12 {
        if t.len() >= max_tris { break; }
        let mut eorder: Vec<(usize, usize)> = vec![];
        let mut em: FxHashMap<(usize, usize), Vec<usize>> = FxHashMap::default();
        for (ti, tr) in t.iter().enumerate() {
            for k in 0..3 {
                let e = ek(tr[k], tr[(k + 1) % 3]);
                let l = em.entry(e).or_default();
                if l.is_empty() { eorder.push(e); }
                l.push(ti);
            }
        }
        let mut changed = false;
        let mut used: FxHashSet<usize> = FxHashSet::default();
        let mut drop: FxHashSet<usize> = FxHashSet::default();
        let mut newt = t.clone();
        let mut cand: Vec<(f64, usize, usize, UV, V3, Vec<usize>)> = vec![];
        for e in &eorder {
            let ts = &em[e];
            if bnd.contains(e) || ts.len() != 2 { continue; }
            let (i, j) = *e;
            let m = [(v[i][0] + v[j][0]) / 2.0, (v[i][1] + v[j][1]) / 2.0];
            let pm = ev.eval(m[0], m[1]);
            let c = add(p[i], p[j]);
            let d = norm(sub(pm, [c[0] / 2.0, c[1] / 2.0, c[2] / 2.0]));
            if d > tol { cand.push((d, i, j, m, pm, ts.clone())); }
        }
        cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, i, j, m, pm, ts) in cand {
            if newt.len() - drop.len() >= max_tris { break; }
            if ts.iter().any(|x| used.contains(x)) { continue; }
            v.push(m); p.push(pm); let mi = v.len() - 1;
            for &tx in &ts {
                let [mut a, mut b, mut c] = t[tx];
                for _ in 0..3 { if (a == i && b == j) || (a == j && b == i) { break; } let o = a; a = b; b = c; c = o; }
                newt.push([a, mi, c]); newt.push([mi, b, c]); drop.insert(tx); used.insert(tx);
            }
            changed = true;
        }
        for (ti, tr) in t.iter().enumerate() {
            if drop.contains(&ti) || newt.len() - drop.len() >= max_tris { continue; }
            if (0..3).all(|k| bnd.contains(&ek(tr[k], tr[(k + 1) % 3]))) {
                let c = [(0.0 + v[tr[0]][0] + v[tr[1]][0] + v[tr[2]][0]) / 3.0, (0.0 + v[tr[0]][1] + v[tr[1]][1] + v[tr[2]][1]) / 3.0];
                let pc = ev.eval(c[0], c[1]);
                let s3 = add(add(p[tr[0]], p[tr[1]]), p[tr[2]]);
                if norm(sub(pc, [s3[0] / 3.0, s3[1] / 3.0, s3[2] / 3.0])) > tol {
                    v.push(c); p.push(pc); let ci = v.len() - 1;
                    let [a, b, cc] = *tr;
                    newt.push([a, b, ci]); newt.push([b, cc, ci]); newt.push([cc, a, ci]); drop.insert(ti); changed = true;
                }
            }
        }
        t = newt.into_iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, x)| x).collect();
        if std::env::var_os("RVT_REFINE_TRACE").is_some() { eprintln!("pass {} tris {} verts {} drop {} tol {}", _pass, t.len(), v.len(), drop.len(), tol); }
        if !changed { break; }
    }
    (v, t)
}

/// rvt_mesh.refine_curved default rel_tol: float(os.environ.get("RVT_REFINE_REL", 5e-3)) (read once, like the reference's default argument at import)
fn refine_rel() -> f64 {
    static R: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *R.get_or_init(|| std::env::var("RVT_REFINE_REL").ok().and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(5e-3))
}
