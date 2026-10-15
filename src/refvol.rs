//! Port of rvt_mesh.refined_volume: volume / area of a closed shell with curved faces (coarse orientation, then every curved triangle subdivided in (u, v)
//! and mapped through its surface). Same traversal orders as the reference; sums are plain sequential sums, so results agree with the reference to
//! rounding (tests/diff_refvol.py), not bit for bit.
use crate::mesh::*;
use rustc_hash::{FxHashMap, FxHashSet};

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
        // absolute sign of every BFS component from the surfaces' own normals (su x sv points out of the solid): each face votes with up to 3 triangles
        let mut aorder: Vec<i64> = vec![];
        let mut ag: FxHashMap<i64, Vec<i32>> = FxHashMap::default();
        for (k, &ti) in keep.iter().enumerate() {
            let f = fk[k];
            if ag.get(&f).map_or(false, |l| l.len() >= 3) { continue; }
            let m = &meta[ti];
            let ev = match &m.ev { MetaEv::S(s) => s.clone(), _ => continue };
            let uv = match &m.uv { Some(u) => *u, None => continue };
            let t = &tris[ti];
            let nt = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            if norm(nt) < 1e-14 { continue; }
            let (u, v) = ((uv[0][0] + uv[1][0] + uv[2][0]) / 3.0, (uv[0][1] + uv[1][1] + uv[2][1]) / 3.0);
            let h = 1e-5;
            let ns = cross(sub(ev.eval(u + h, v), ev.eval(u - h, v)), sub(ev.eval(u, v + h), ev.eval(u, v - h)));
            if norm(ns) < 1e-14 { continue; }
            let l = ag.entry(f).or_default();
            if l.is_empty() { aorder.push(f); }
            l.push(if dot(nt, ns) > 0.0 { 1 } else { -1 });
        }
        let mut votes: FxHashMap<i64, i64> = FxHashMap::default();
        for f in &aorder {
            let sg: i64 = if ag[f].iter().map(|x| *x as i64).sum::<i64>() >= 0 { 1 } else { -1 };
            *votes.entry(fc[f]).or_insert(0) += if !ff[f] { sg } else { -sg };
        }
        let keys: Vec<i64> = ff.keys().copied().collect();
        for f in keys { if votes.get(&fc[&f]).copied().unwrap_or(0) < 0 { let x = ff[&f]; ff.insert(f, !x); } }
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
        let fan = if kind == "ruled" { fan_uv(&ev, &uv, 1usize << lv) } else { None };
        let fanned = fan.is_some();
        let mut cur: Vec<[UV; 3]> = fan.unwrap_or_else(|| vec![uv]);
        let mid = |p: UV, q: UV| -> UV { [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0] };
        for _ in 0..(if fanned { 0 } else { lv }) {
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

/// rvt_mesh.mesh_volume(tris, tol) -> (volume, closed, n_open_edges, area)
/// == rvt_mesh._fan_uv (2026-10-15): a coarse ruled-surface triangle with exactly one degenerate vertex (ruled surface between a curve and a point) as an n x n grid
/// along straight generators: P(s, t) = (u_b + s (u_c - u_b), v_apex + t (v_b + s (v_c - v_b) - v_apex)), cells [q00, q01, q11], [q00, q11, q10]; None otherwise
fn fan_uv(ev: &std::sync::Arc<Surf>, uv: &[UV; 3], n: usize) -> Option<Vec<[UV; 3]>> {
    let pole: Vec<bool> = uv.iter().map(|q| norm(sub(ev.eval(q[0], q[1]), ev.eval(q[0] + 0.3, q[1]))) < 1e-9).collect();
    if pole.iter().filter(|&&b| b).count() != 1 { return None; }
    let i = pole.iter().position(|&b| b).unwrap(); let (j, k) = ((i + 1) % 3, (i + 2) % 3);
    let (ub, vb) = (uv[j][0], uv[j][1]); let (uc, vc) = (uv[k][0], uv[k][1]); let vp = uv[i][1];
    let s: Vec<f64> = (0..=n).map(|a| if a == n { 1.0 } else { a as f64 * (1.0 / n as f64) }).collect();      // numpy linspace(0, 1, n + 1)
    let p = |a: usize, b: usize| -> UV { [ub + s[a] * (uc - ub), vp + s[b] * (vb + s[a] * (vc - vb) - vp)] };
    let mut out = Vec::with_capacity(2 * n * n);
    for a in 0..n {
        for b in 0..n {
            let (q00, q10, q11, q01) = (p(a, b), p(a + 1, b), p(a + 1, b + 1), p(a, b + 1));
            out.push([q00, q01, q11]); out.push([q00, q11, q10]);
        }
    }
    Some(out)
}

pub fn mesh_volume(tris: &[Tri], tol: f64) -> (f64, bool, usize, f64) {
    if tris.is_empty() { return (0.0, false, 0, 0.0); }
    let mut vid: FxHashMap<K3, usize> = FxHashMap::default();
    let mut vp: Vec<V3> = vec![];
    let mut tt: Vec<[usize; 3]> = vec![];
    for t in tris {
        let mut ids = [0usize; 3];
        for i in 0..3 { let n = vid.len(); ids[i] = *vid.entry(key(t[i], tol)).or_insert_with(|| { vp.push(t[i]); n }); }
        if ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2] { tt.push(ids); }
    }
    for t in tris { for p in t { vp[vid[&key(*p, tol)]] = *p; } }
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
    if !tt.is_empty() && de.iter().all(|((a, b), c)| de.get(&(*b, *a)).copied().unwrap_or(0) == *c) {
        let v: Vec<f64> = tt.iter().map(|t| dot(vp[t[0]], cross(vp[t[1]], vp[t[2]]))).collect();
        let vol = np_sum(&v) / 6.0;
        if vol > 0.0 {
            let ar: Vec<f64> = tt.iter().map(|t| norm(cross(sub(vp[t[1]], vp[t[0]]), sub(vp[t[2]], vp[t[0]])))).collect();
            return (vol, true, 0, np_sum(&ar) / 2.0);
        }
    }
    let n = tt.len();
    let mut flip: Vec<Option<bool>> = vec![None; n];
    for s0 in 0..n {
        if flip[s0].is_some() { continue; }
        flip[s0] = Some(false);
        let mut stack = vec![s0];
        while let Some(k) = stack.pop() {
            let mut t = tt[k];
            if flip[k] == Some(true) { t = [t[0], t[2], t[1]]; }
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let l = em.get((a.min(b), a.max(b)));
                if l.len() != 2 { continue; }
                for &(k2, a2, b2) in l {
                    if k2 == k || flip[k2].is_some() { continue; }
                    flip[k2] = Some(a2 == a && b2 == b); stack.push(k2);
                }
            }
        }
    }
    let (mut vol, mut area) = (0.0, 0.0);
    for (k, t) in tt.iter().enumerate() {
        let (p0, mut p1, mut p2) = (vp[t[0]], vp[t[1]], vp[t[2]]);
        if flip[k] == Some(true) { std::mem::swap(&mut p1, &mut p2); }
        vol += dot(p0, cross(p1, p2)) / 6.0; area += norm(cross(sub(p1, p0), sub(p2, p0))) / 2.0;
    }
    let open = em.lists.iter().filter(|l| l.len() != 2).count();
    (vol.abs(), open == 0, open, area)
}

/// eigenvalues (descending) and eigenvectors of a symmetric 3x3 matrix (Jacobi)
fn eig3(mut a: [[f64; 3]; 3]) -> ([f64; 3], [V3; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..64 {
        let (mut p, mut q, mut big) = (0, 1, a[0][1].abs());
        if a[0][2].abs() > big { p = 0; q = 2; big = a[0][2].abs(); }
        if a[1][2].abs() > big { p = 1; q = 2; big = a[1][2].abs(); }
        if big < 1e-300 { break; }
        let th = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = if th >= 0.0 { 1.0 / (th + (th * th + 1.0).sqrt()) } else { -1.0 / (-th + (th * th + 1.0).sqrt()) };
        let c = 1.0 / (t * t + 1.0).sqrt(); let s = t * c;
        let mut b = a;
        for k in 0..3 { b[p][k] = c * a[p][k] - s * a[q][k]; b[q][k] = s * a[p][k] + c * a[q][k]; }
        let a2 = b;
        for k in 0..3 { b[k][p] = c * a2[k][p] - s * a2[k][q]; b[k][q] = s * a2[k][p] + c * a2[k][q]; }
        a = b;
        for k in 0..3 { let (vp_, vq) = (v[k][p], v[k][q]); v[k][p] = c * vp_ - s * vq; v[k][q] = s * vp_ + c * vq; }
    }
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&i, &j| a[j][j].partial_cmp(&a[i][i]).unwrap_or(std::cmp::Ordering::Equal));
    let ev = [a[idx[0]][idx[0]], a[idx[1]][idx[1]], a[idx[2]][idx[2]]];
    let vec = idx.map(|i| [v[0][i], v[1][i], v[2][i]]);
    (ev, vec)
}

/// singular values (descending) and right singular vectors of the centred point set (np.linalg.svd(P - mean))
fn svd_centred(p: &[V3]) -> ([f64; 3], [V3; 3], V3) {
    let n = p.len() as f64;
    let mut c = [0.0; 3];
    for q in p { for k in 0..3 { c[k] += q[k]; } }
    c = [c[0] / n, c[1] / n, c[2] / n];
    let mut m = [[0.0; 3]; 3];
    for q in p { let d = sub(*q, c); for i in 0..3 { for j in 0..3 { m[i][j] += d[i] * d[j]; } } }
    let (ev, vec) = eig3(m);
    (ev.map(|x| x.max(0.0).sqrt()), vec, c)
}

/// rvt_mesh.heal_open(tris, tol) -> (tris, n_caps, healed_all)
pub fn heal_open(tris: &[Tri], tol: f64, earcut: Earcut) -> (Vec<Tri>, usize, bool) {
    let mut pts: FxHashMap<K3, V3> = FxHashMap::default();
    let mut eorder: Vec<(K3, K3)> = vec![];
    let mut em: FxHashMap<(K3, K3), Vec<(K3, K3)>> = FxHashMap::default();
    for t in tris {
        let ks = [key(t[0], tol), key(t[1], tol), key(t[2], tol)];
        for i in 0..3 { pts.insert(ks[i], t[i]); }
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if ks[a] != ks[b] {
                let e = if ks[a] <= ks[b] { (ks[a], ks[b]) } else { (ks[b], ks[a]) };
                let l = em.entry(e).or_default();
                if l.is_empty() { eorder.push(e); }
                l.push((ks[a], ks[b]));
            }
        }
    }
    let bnd: Vec<(K3, K3)> = eorder.iter().filter(|e| em[e].len() == 1).map(|e| em[e][0]).collect();
    if bnd.is_empty() { return (tris.to_vec(), 0, true); }
    let mut nxt: FxHashMap<K3, Vec<K3>> = FxHashMap::default();
    for (a, b) in &bnd { nxt.entry(*a).or_default().push(*b); }
    let mut used: rustc_hash::FxHashSet<(K3, K3)> = Default::default();
    let mut loops: Vec<Vec<K3>> = vec![];
    for (a, b) in &bnd {
        if used.contains(&(*a, *b)) { continue; }
        let mut lp = vec![*a]; let mut cur = *b; used.insert((*a, *b)); let mut guard = 0;
        while cur != *a && guard < 10000 {
            guard += 1;
            let cand = nxt.get(&cur).and_then(|l| l.iter().find(|c| !used.contains(&(cur, **c))).copied());
            let c = match cand { Some(c) => c, None => break };
            used.insert((cur, c)); lp.push(cur); cur = c;
        }
        if cur == *a && lp.len() >= 3 { loops.push(lp); } else { return (tris.to_vec(), 0, false); }
    }
    let mut out = tris.to_vec(); let mut caps = 0;
    for lp in &loops {
        let p: Vec<V3> = lp.iter().map(|k| pts[k]).collect();
        let (s, vt, c) = svd_centred(&p);
        if s[2] > 1e-4 * s[0].max(1e-9) + 1e-6 { return (tris.to_vec(), 0, false); }
        let uv: Vec<UV> = p.iter().map(|q| { let d = sub(*q, c); [dot(d, vt[0]), dot(d, vt[1])] }).collect();
        let ends = [uv.len() as u32];
        let r = earcut(&[(&uv, &ends)]);
        let idx = match r.into_iter().next().flatten() { Some(i) => i, None => return (tris.to_vec(), 0, false) };
        for k in (0..idx.len() / 3 * 3).step_by(3) { out.push([p[idx[k] as usize], p[idx[k + 1] as usize], p[idx[k + 2] as usize]]); }
        caps += 1;
    }
    (out, caps, true)
}

/// rvt_mesh.volume_by_node(tris, meta, [0] * n, fid=fid)[0]: one body
pub fn volume_one(tris: &[Tri], meta: &[Meta], fid: &[i64], earcut: Earcut) -> f64 {
    if tris.is_empty() { return 0.0; }
    let refinable = meta.len() == tris.len() && meta.iter().all(|m| !matches!(m.ev, MetaEv::None) || m.kind == "plane");
    let (mut v, mut c) = if refinable {
        match refined_volume(tris, meta, 3, 5e-5, Some(fid)) { Ok(r) => (r.0, r.1), Err(_) => { let r = mesh_volume(tris, 5e-5); (r.0, r.1) } }
    } else { let r = mesh_volume(tris, 5e-5); (r.0, r.1) };
    if !c {
        let p: Vec<V3> = tris.iter().flat_map(|t| t.iter().copied()).collect();
        let (s, _, _) = svd_centred(&p);
        if s[2] < 1e-6 * s[0].max(1e-9) { return 0.0; }
        let (t2, ncap, okh) = heal_open(tris, 5e-5, earcut);
        if okh && ncap > 0 { let r = mesh_volume(&t2, 5e-5); if r.1 { v = r.0; c = true; } }
    }
    let _ = c;
    v
}

/// per-body numbers of build.body_summary: vertices welded at 1e-4 (numpy round of x * 1e4, half even), edge parity per body
/// -> (rows (id, triangles, distinct fid count, area, odd edges, sum of cross products, lo, hi), body index per triangle (-1 = beyond the limit))
pub fn body_summary_core(p: &[f64], nid: &[i64], fid: Option<&[i64]>, limit: usize) -> (Vec<(i64, usize, Option<usize>, f64, bool, V3, V3, V3)>, Vec<i8>) {
    let n = p.len() / 9;
    let mut bodies: Vec<i64> = nid.to_vec();
    bodies.sort(); bodies.dedup(); bodies.truncate(limit);
    let tb: Vec<i8> = nid.iter().map(|x| bodies.binary_search(x).map(|i| i as i8).unwrap_or(-1)).collect();
    let mut vid: FxHashMap<(i64, i64, i64), u32> = FxHashMap::default();
    let mut ids: Vec<[u32; 3]> = Vec::with_capacity(n);
    for t in 0..n {
        let mut r = [0u32; 3];
        for k in 0..3 {
            let q = &p[t * 9 + k * 3..t * 9 + k * 3 + 3];
            let key = ((q[0] * 1e4).round_ties_even() as i64, (q[1] * 1e4).round_ties_even() as i64, (q[2] * 1e4).round_ties_even() as i64);
            let nv = vid.len() as u32;
            r[k] = *vid.entry(key).or_insert(nv);
        }
        ids.push(r);
    }
    let nb = bodies.len();
    let mut ec: FxHashMap<(i8, u32, u32), u32> = FxHashMap::default();
    for (t, r) in ids.iter().enumerate() {
        let b = tb[t];
        if b < 0 { continue; }
        for (a, c) in [(r[0], r[1]), (r[1], r[2]), (r[2], r[0])] { *ec.entry((b, a.min(c), a.max(c))).or_insert(0) += 1; }
    }
    let mut odd = vec![false; nb];
    for (k, c) in &ec { if c % 2 == 1 { odd[k.0 as usize] = true; } }
    let mut rows = vec![];
    for (k, &b) in bodies.iter().enumerate() {
        let (mut cnt, mut cs, mut lo, mut hi) = (0usize, [0.0; 3], [f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let mut areas: Vec<f64> = vec![];
        let mut fs: FxHashSet<i64> = FxHashSet::default();
        for t in 0..n {
            if tb[t] as usize != k || tb[t] < 0 { continue; }
            cnt += 1;
            let q = |i: usize| -> V3 { [p[t * 9 + i * 3], p[t * 9 + i * 3 + 1], p[t * 9 + i * 3 + 2]] };
            let c = cross(sub(q(1), q(0)), sub(q(2), q(0)));
            areas.push(norm(c) / 2.0);
            for j in 0..3 { cs[j] += c[j]; }
            for i in 0..3 { let v = q(i); for j in 0..3 { lo[j] = lo[j].min(v[j]); hi[j] = hi[j].max(v[j]); } }
            if let Some(f) = fid { fs.insert(f[t]); }
        }
        rows.push((b, cnt, fid.map(|_| fs.len()), np_sum(&areas), odd[k], cs, lo, hi));
    }
    (rows, tb)
}
