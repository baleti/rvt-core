//! Port of rvt_mesh.refined_volume: volume / area of a closed shell with curved faces (coarse orientation, then every curved triangle subdivided in (u, v)
//! and mapped through its surface). Same traversal orders as the reference; sums are plain sequential sums, so results agree with the reference to
//! rounding (tests/diff_refvol.py), not bit for bit.
use crate::mesh::*;
use rustc_hash::FxHashMap;

type K3 = (i64, i64, i64);

#[inline] fn key(p: V3, tol: f64) -> K3 { (pyround(p[0] / tol), pyround(p[1] / tol), pyround(p[2] / tol)) }

/// insertion-ordered multimap edge -> [(triangle, a, b)]
struct EdgeMap { idx: FxHashMap<(usize, usize), usize>, lists: Vec<Vec<(usize, usize, usize)>>, keys: Vec<(usize, usize)> }
impl EdgeMap {
    fn get(&self, e: (usize, usize)) -> &[(usize, usize, usize)] { match self.idx.get(&e) { Some(&i) => &self.lists[i], None => &[] } }
}

/// rvt_mesh.refined_volume(tris, meta, levels, tol, centroid=True, fid) -> (volume, closed, area, centroid); Err when a curved triangle's evaluator
/// would raise in the reference (MetaEv::Broken)
pub fn refined_volume(tris: &[Tri], meta: &[Meta], levels: i32, tol: f64, fid: Option<&[i64]>) -> Result<(f64, bool, f64, V3), ()> {
    if tris.is_empty() { return Ok((0.0, false, 0.0, [0.0; 3])); }
    let npts = (tris.len() * 3) as f64;
    let mut s = [0.0; 3];
    for t in tris { for p in t { for k in 0..3 { s[k] += p[k]; } } }
    let off = [s[0] / npts, s[1] / npts, s[2] / npts];
    let mut vid: FxHashMap<K3, usize> = FxHashMap::default();
    let mut tt: Vec<[usize; 3]> = vec![];
    let mut keep: Vec<usize> = vec![];
    for (ti, t) in tris.iter().enumerate() {
        let mut ids = [0usize; 3];
        for i in 0..3 { let n = vid.len(); ids[i] = *vid.entry(key(t[i], tol)).or_insert(n); }
        if ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2] { tt.push(ids); keep.push(ti); }
    }
    let mut em = EdgeMap { idx: FxHashMap::default(), lists: vec![], keys: vec![] };
    for (k, t) in tt.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = (a.min(b), a.max(b));
            let i = *em.idx.entry(e).or_insert_with(|| { em.lists.push(vec![]); em.keys.push(e); em.lists.len() - 1 });
            em.lists[i].push((k, a, b));
        }
    }
    let mut de: FxHashMap<(usize, usize), usize> = FxHashMap::default();
    for t in &tt { for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] { *de.entry((a, b)).or_insert(0) += 1; } }
    let balanced = !tt.is_empty() && de.iter().all(|((a, b), c)| de.get(&(*b, *a)).copied().unwrap_or(0) == *c);
    let n = tt.len();
    let mut flip: Vec<Option<bool>> = if balanced { vec![Some(false); n] } else { vec![None; n] };
    let mut comp: Vec<i64> = vec![0; n];
    if let (Some(fid), false) = (fid, balanced) {
        let fk: Vec<i64> = keep.iter().map(|&ti| fid[ti]).collect();
        let mut fadj: FxHashMap<i64, Vec<(i64, bool)>> = FxHashMap::default();
        for l in &em.lists {
            if l.len() != 2 { continue; }
            let ((k1, a1, _), (k2, a2, _)) = (l[0], l[1]);
            if fk[k1] == fk[k2] { continue; }
            fadj.entry(fk[k1]).or_default().push((fk[k2], a1 == a2));
            fadj.entry(fk[k2]).or_default().push((fk[k1], a1 == a2));
        }
        let mut ff: FxHashMap<i64, bool> = FxHashMap::default();
        let mut fc: FxHashMap<i64, i64> = FxHashMap::default();
        for &f0 in &fk {
            if ff.contains_key(&f0) { continue; }
            ff.insert(f0, false); fc.insert(f0, f0);
            let mut st = vec![f0];
            while let Some(f) = st.pop() {
                let ffv = ff[&f];
                if let Some(adj) = fadj.get(&f) {
                    for &(g, rel) in adj { if !ff.contains_key(&g) { ff.insert(g, ffv ^ rel); fc.insert(g, f0); st.push(g); } }
                }
            }
        }
        flip = fk.iter().map(|f| Some(ff[f])).collect();
        comp = fk.iter().map(|f| fc[f]).collect();
    }
    for s0 in 0..n {
        if flip[s0].is_some() { continue; }
        flip[s0] = Some(false); comp[s0] = s0 as i64;
        let mut stack = vec![s0];
        while let Some(k) = stack.pop() {
            let mut t = tt[k];
            if flip[k] == Some(true) { t = [t[0], t[2], t[1]]; }
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let l = em.get((a.min(b), a.max(b)));
                if l.len() != 2 { continue; }
                for &(k2, a2, b2) in l {
                    if k2 == k || flip[k2].is_some() { continue; }
                    flip[k2] = Some(a2 == a && b2 == b); comp[k2] = s0 as i64; stack.push(k2);
                }
            }
        }
    }
    let mut closed = balanced || em.lists.iter().all(|l| l.len() == 2);
    if !closed { closed = tjunction_closed(tris, tol); }
    // per orientation component: [volume, volume-weighted centroid sum, lo, hi], in first-use order
    let mut corder: Vec<i64> = vec![];
    let mut cv: FxHashMap<i64, (f64, V3, Option<V3>, Option<V3>)> = FxHashMap::default();
    fn acc_(cv: &mut FxHashMap<i64, (f64, V3, Option<V3>, Option<V3>)>, corder: &mut Vec<i64>, c: i64, dv: f64, c3: V3, pts: &[V3]) {
        let e = cv.entry(c).or_insert_with(|| { corder.push(c); (0.0, [0.0; 3], None, None) });
        e.0 += dv; e.1 = add(e.1, c3);
        let (mut lo, mut hi) = (pts[0], pts[0]);
        for p in pts { for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); } }
        e.2 = Some(match e.2 { None => lo, Some(o) => [o[0].min(lo[0]), o[1].min(lo[1]), o[2].min(lo[2])] });
        e.3 = Some(match e.3 { None => hi, Some(o) => [o[0].max(hi[0]), o[1].max(hi[1]), o[2].max(hi[2])] });
    }
    let mut acc = |c: i64, dv: f64, c3: V3, pts: &[V3]| acc_(&mut cv, &mut corder, c, dv, c3, pts);
    let mut area = 0.0;
    for (k, &ti) in keep.iter().enumerate() {
        let m = &meta[ti];
        let fl = flip[k] == Some(true);
        let flat = |area: &mut f64, acc: &mut dyn FnMut(i64, f64, V3, &[V3])| {
            let mut p = [sub(tris[ti][0], off), sub(tris[ti][1], off), sub(tris[ti][2], off)];
            if fl { p = [p[0], p[2], p[1]]; }
            let dv = dot(p[0], cross(p[1], p[2])) / 6.0;
            let s3 = add(add(p[0], p[1]), p[2]);
            acc(comp[k], dv, [dv * s3[0] / 4.0, dv * s3[1] / 4.0, dv * s3[2] / 4.0], &p);
            *area += norm(cross(sub(p[1], p[0]), sub(p[2], p[0]))) / 2.0;
        };
        let ev = match (&m.ev, m.kind) {
            (_, "plane") => None,
            (_, _) if levels <= 0 => None,
            (MetaEv::None, _) => None,
            (MetaEv::Broken, _) => return Err(()),
            (MetaEv::S(s), _) => Some(s.clone()),
        };
        let ev = match ev { None => { flat(&mut area, &mut acc); continue; } Some(e) => e };
        let uv = match &m.uv { Some(u) => *u, None => return Err(()) };
        let kind = m.kind;
        if kind == "cone" || kind == "revolution" {
            let pole = uv.iter().any(|q| norm(sub(ev.eval(q[0], q[1] + 0.0), ev.eval(q[0] + 0.3, q[1]))) < 1e-9);
            if pole { flat(&mut area, &mut acc); continue; }
        }
        let lv: i32 = if kind == "cylinder" || kind == "cone" || kind == "revolution" {
            let mut du = (uv[0][0] - uv[1][0]).abs().max((uv[1][0] - uv[2][0]).abs()).max((uv[0][0] - uv[2][0]).abs());
            if kind == "revolution" {
                let dvv = (uv[0][1] - uv[1][1]).abs().max((uv[1][1] - uv[2][1]).abs()).max((uv[0][1] - uv[2][1]).abs());
                if dvv > 0.1 {
                    let u_ = uv[0][0];
                    let va = uv[0][1].min(uv[1][1]).min(uv[2][1]); let vb = uv[0][1].max(uv[1][1]).max(uv[2][1]);
                    let (pa, pb, pm) = (ev.eval(u_, va), ev.eval(u_, vb), ev.eval(u_, (va + vb) / 2.0));
                    let mid = [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0, (pa[2] + pb[2]) / 2.0];
                    if norm(sub(pm, mid)) > 1e-6 * (1.0f64).max(norm(sub(pb, pa))) { du = du.max(dvv); }
                }
            }
            ((du.max(1e-9) / 0.1).log2().ceil().max(1.0).min(5.0)) as i32
        } else { levels + 1 };
        // subdivision: each level replaces every triangle by its 4 children, child-major order (all first children, then all second, ...)
        let mut cur: Vec<[UV; 3]> = vec![uv];
        let mid = |p: UV, q: UV| -> UV { [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0] };
        for _ in 0..lv {
            let mut nx: Vec<[UV; 3]> = Vec::with_capacity(cur.len() * 4);
            for c in 0..4 {
                for t in &cur {
                    let (a, b, cc) = (t[0], t[1], t[2]);
                    let (ab, bc, ca) = (mid(a, b), mid(b, cc), mid(cc, a));
                    nx.push(match c { 0 => [a, ab, ca], 1 => [ab, b, bc], 2 => [ca, bc, cc], _ => [ab, bc, ca] });
                }
            }
            cur = nx;
        }
        let mut cache: FxHashMap<(i64, i64), V3> = FxHashMap::default();
        let mut q: Vec<V3> = Vec::with_capacity(cur.len() * 3);
        for t in &cur {
            for p in t {
                let kk = ((p[0] * 1e10).round_ties_even() as i64, (p[1] * 1e10).round_ties_even() as i64);
                let pt = *cache.entry(kk).or_insert_with(|| ev.eval(p[0], p[1]));
                q.push(sub(pt, off));
            }
        }
        let (mut vs, mut cs) = (0.0, [0.0; 3]);
        for c in q.chunks_exact(3) {
            let (q0, q1, q2) = if fl { (c[0], c[2], c[1]) } else { (c[0], c[1], c[2]) };
            let dv = dot(q0, cross(q1, q2)) / 6.0;
            vs += dv;
            let s3 = add(add(q0, q1), q2);
            for k3 in 0..3 { cs[k3] += dv * s3[k3] / 4.0; }
            area += norm(cross(sub(q1, q0), sub(q2, q0))) / 2.0;
        }
        acc(comp[k], vs, cs, &q);
    }
    drop(acc);
    let comps: Vec<(f64, V3, V3, V3)> = corder.iter().map(|c| { let e = cv[c]; (e.0, e.1, e.2.unwrap(), e.3.unwrap()) }).collect();
    let mut vol = 0.0; let mut csum = [0.0; 3];
    for (i, (v_i, c_i, lo_i, hi_i)) in comps.iter().enumerate() {
        let sgn = if *v_i >= 0.0 { 1.0 } else { -1.0 };
        let mut inner = false;
        if !balanced {
            for (j, (v_j, _, lo_j, hi_j)) in comps.iter().enumerate() {
                if j != i && v_j.abs() > v_i.abs() && (0..3).all(|k| lo_i[k] > lo_j[k] + 1e-6) && (0..3).all(|k| hi_i[k] < hi_j[k] - 1e-6) { inner = true; break; }
            }
        }
        let w = if inner { -1.0 } else { 1.0 };
        vol += w * v_i.abs();
        csum = add(csum, mul(w * sgn, *c_i));
    }
    if balanced && comps.len() == 1 { vol = comps[0].0; csum = comps[0].1; }
    let cen = if vol != 0.0 { [csum[0] / vol + off[0], csum[1] / vol + off[1], csum[2] / vol + off[2]] } else { off };
    Ok((vol.abs(), closed, area, cen))
}
