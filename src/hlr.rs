//! Depth-buffer rasteriser of rvtsheets/hlr.py (_raster, numba): same per-pixel arithmetic in the same operation order, so the max-depth buffer is bit-identical;
//! the buffer is split into row bands rasterised in parallel (a max over triangles is order independent).
use pyo3::prelude::*;
use rayon::prelude::*;

#[inline]
fn raster_band(tris: &[f64], w: i64, h: i64, u0: f64, v0: f64, px: f64, y_lo: i64, y_hi: i64, zb: &mut [f64]) {
    // zb = rows y_lo..y_hi of the buffer (row length w)
    for t in tris.chunks_exact(9) {
        let (x0, y0, z0) = ((t[0] - u0) / px, (t[1] - v0) / px, t[2]);
        let (x1, y1, z1) = ((t[3] - u0) / px, (t[4] - v0) / px, t[5]);
        let (x2, y2, z2) = ((t[6] - u0) / px, (t[7] - v0) / px, t[8]);
        let den = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
        if den.abs() < 1e-12 { continue; }
        let xmin = ((x0.min(x1).min(x2)).floor() as i64).max(0);
        let xmax = ((x0.max(x1).max(x2)).ceil() as i64).min(w - 1);
        let ymin = ((y0.min(y1).min(y2)).floor() as i64).max(0);
        let ymax = ((y0.max(y1).max(y2)).ceil() as i64).min(h - 1);
        if ymax < y_lo || ymin >= y_hi { continue; }
        let gx = ((z0 - z2) * (y1 - y2) - (z1 - z2) * (y0 - y2)) / den;
        let gy = ((z1 - z2) * (x0 - x2) - (z0 - z2) * (x1 - x2)) / den;
        let bias = (2.0 * (gx * gx + gy * gy).sqrt()).min(0.5);
        // per row, the x interval where the three barycentric coordinates can be >= -1e-6 (each is linear in x): pixels outside it (plus a 2 pixel safety margin, far above the
        // float error of the evaluation) cannot pass the exact test below, so skipping them leaves the buffer bit-identical
        let (ka, kb) = ((y1 - y2) / den, (y2 - y0) / den);
        let kc = -(ka + kb);
        for yy in ymin.max(y_lo)..=ymax.min(y_hi - 1) {
            let py = yy as f64 + 0.5;
            let row = &mut zb[((yy - y_lo) * w) as usize..((yy - y_lo + 1) * w) as usize];
            let (mut xa, mut xb) = (xmin, xmax);
            let ca = (x2 - x1) * (py - y2) / den - ka * x2;
            let cb = (x0 - x2) * (py - y2) / den - kb * x2;
            let cc = 1.0 - ca - cb;
            let mut empty = false;
            for (k, c0) in [(ka, ca), (kb, cb), (kc, cc)] {
                if k.abs() < 1e-300 { if c0 < -1e-3 { empty = true; } continue; }
                let bound = (-1e-6 - c0) / k - 0.5;
                if k > 0.0 { xa = xa.max(bound.floor() as i64 - 2); } else { xb = xb.min(bound.ceil() as i64 + 2); }
            }
            if empty || xa > xb { continue; }
            for xx in xa..=xb {
                let pxx = xx as f64 + 0.5;
                let a = ((y1 - y2) * (pxx - x2) + (x2 - x1) * (py - y2)) / den;
                let b = ((y2 - y0) * (pxx - x2) + (x0 - x2) * (py - y2)) / den;
                let c = 1.0 - a - b;
                if a < -1e-6 || b < -1e-6 || c < -1e-6 { continue; }
                let z = a * z0 + b * z1 + c * z2 - bias;
                let cell = &mut row[xx as usize];
                if z > *cell { *cell = z; }
            }
        }
    }
}

/// rvtsheets.hlr._raster(tris (n,3,3) float64 C-contiguous, W, H, u0, v0, px, zb (H, W) float64 C-contiguous, modified in place).
/// Both arrays are passed as raw addresses (abi3 has no buffer protocol before 3.11); the caller guarantees dtype, contiguity and sizes (rvt_fast.hlr_raster does).
#[pyfunction]
pub fn hlr_raster(py: Python<'_>, tris_ptr: usize, n: usize, w: usize, h: usize, u0: f64, v0: f64, px: f64, zb_ptr: usize) -> PyResult<()> {
    if w == 0 || h == 0 { return Ok(()); }
    // SAFETY: the Python wrapper keeps both numpy arrays alive and sized n*9 / h*w float64 for the duration of the call
    let tris: &[f64] = unsafe { std::slice::from_raw_parts(tris_ptr as *const f64, n * 9) };
    let zb: &mut [f64] = unsafe { std::slice::from_raw_parts_mut(zb_ptr as *mut f64, h * w) };
    // small jobs (most calls: a few thousand pixels of triangle area) run on the calling thread: the pool hand-off and the per-band triangle scans cost more than they save
    let mut work = 0.0f64;
    for t in tris.chunks_exact(9) {
        let (xa, xb) = (t[0].min(t[3]).min(t[6]), t[0].max(t[3]).max(t[6]));
        let (ya, yb) = (t[1].min(t[4]).min(t[7]), t[1].max(t[4]).max(t[7]));
        work += ((xb - xa) / px + 1.0).min(w as f64) * ((yb - ya) / px + 1.0).min(h as f64);
        if work > 400_000.0 { break; }
    }
    if work <= 400_000.0 {
        py.allow_threads(|| raster_band(tris, w as i64, h as i64, u0, v0, px, 0, h as i64, zb));
        return Ok(());
    }
    let band = ((h + 31) / 32).max(16);
    py.allow_threads(|| {
        crate::pool::run(|| {
            zb.par_chunks_mut(band * w).enumerate().for_each(|(i, chunk)| {
                let y_lo = (i * band) as i64;
                let rows = (chunk.len() / w) as i64;
                raster_band(tris, w as i64, h as i64, u0, v0, px, y_lo, y_lo + rows, chunk);
            });
        })
    });
    Ok(())
}

/// rvtsheets.hlr._visible: segs (m,2,3) float64, out (m, n_samp) uint8 (1 = visible); zb (H, W) float64. Same arithmetic as the numba kernel; segments run in parallel.
#[pyfunction]
pub fn hlr_visible(py: Python<'_>, segs_ptr: usize, m: usize, n_samp: usize, u0: f64, v0: f64, px: f64, zb_ptr: usize, w: usize, h: usize, eps: f64, out_ptr: usize) -> PyResult<()> {
    // SAFETY: the Python wrapper keeps the arrays alive and sized m*6 / h*w float64 / m*n_samp uint8
    let segs: &[f64] = unsafe { std::slice::from_raw_parts(segs_ptr as *const f64, m * 6) };
    let zb: &[f64] = unsafe { std::slice::from_raw_parts(zb_ptr as *const f64, h * w) };
    let out: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(out_ptr as *mut u8, m * n_samp) };
    let (wi, hi) = (w as i64, h as i64);
    let one = |s: &[f64], o: &mut [u8]| {
        for k in 0..n_samp {
            let t = (k as f64 + 0.5) / n_samp as f64;
            let u = s[0] + t * (s[3] - s[0]);
            let v = s[1] + t * (s[4] - s[1]);
            let wd = s[2] + t * (s[5] - s[2]);
            let xx = ((u - u0) / px) as i64;
            let yy = ((v - v0) / px) as i64;
            let mut vis = true;
            for dy in -1..2i64 {
                for dx in -1..2i64 {
                    let (x, y) = (xx + dx, yy + dy);
                    if x < 0 || y < 0 || x >= wi || y >= hi { continue; }
                    if zb[(y * wi + x) as usize] > wd + eps { vis = false; }
                }
            }
            o[k] = vis as u8;
        }
    };
    py.allow_threads(|| {
        if m * n_samp < 4096 {
            for (s, o) in segs.chunks_exact(6).zip(out.chunks_mut(n_samp.max(1))) { one(s, o); }
        } else {
            crate::pool::run(|| segs.par_chunks_exact(6).zip(out.par_chunks_mut(n_samp.max(1))).for_each(|(s, o)| one(s, o)));
        }
    });
    Ok(())
}

/// rvtsheets.viewdraw._feature_edges(T, None): edges (keys = coordinates rounded to 6 decimals as np.round does) shared by 1 triangle, or by 2 triangles whose unit normals
/// differ by more than the feature angle (|n_i . n_j| < cosmax); output in the dict's first-seen order, as float64 bytes (k, 2, 3). T = (n,3,3) float64 C-contiguous at tris_ptr.
#[pyfunction]
pub fn feature_edges<'py>(py: Python<'py>, tris_ptr: usize, n: usize, cosmax: f64) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    use rustc_hash::FxHashMap;
    // SAFETY: the Python wrapper keeps the array alive and sized n*9 float64
    let t: &[f64] = unsafe { std::slice::from_raw_parts(tris_ptr as *const f64, n * 9) };
    let bytes = py.allow_threads(|| {
        let rnd = |x: f64| (x * 1e6).round_ties_even() / 1e6;
        let rt: Vec<f64> = t.iter().map(|&x| rnd(x)).collect();
        let norm0 = |x: f64| if x == 0.0 { 0.0 } else { x };      // -0.0 == 0.0 (and hashes equal) for a Python tuple key
        let bits = |p: &[f64]| [norm0(p[0]).to_bits(), norm0(p[1]).to_bits(), norm0(p[2]).to_bits()];
        let mut idx: FxHashMap<([u64; 3], [u64; 3]), usize> = FxHashMap::default();
        let mut keys: Vec<([f64; 3], [f64; 3])> = vec![];
        let mut tris_of: Vec<Vec<usize>> = vec![];
        for i in 0..n {
            let v = &rt[i * 9..i * 9 + 9];
            for (a, b) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (pa, pb) = ([v[a * 3], v[a * 3 + 1], v[a * 3 + 2]], [v[b * 3], v[b * 3 + 1], v[b * 3 + 2]]);
                let lt = { let (qa, qb) = (pa.map(norm0), pb.map(norm0)); qa[0] < qb[0] || (qa[0] == qb[0] && (qa[1] < qb[1] || (qa[1] == qb[1] && qa[2] < qb[2]))) };
                let (ka, kb) = if lt { (pa, pb) } else { (pb, pa) };
                let key = (bits(&ka), bits(&kb));
                let e = *idx.entry(key).or_insert_with(|| { keys.push((ka, kb)); tris_of.push(vec![]); keys.len() - 1 });
                tris_of[e].push(i);
            }
        }
        let normal = |i: usize| -> [f64; 3] {
            let v = &t[i * 9..i * 9 + 9];
            let (a, b) = ([v[3] - v[0], v[4] - v[1], v[5] - v[2]], [v[6] - v[0], v[7] - v[1], v[8] - v[2]]);
            let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            let mut nl = ((c[0] * c[0] + c[1] * c[1]) + c[2] * c[2]).sqrt();
            if nl == 0.0 { nl = 1.0; }
            [c[0] / nl, c[1] / nl, c[2] / nl]
        };
        let mut out: Vec<u8> = vec![];
        for (e, ts) in tris_of.iter().enumerate() {
            let keep = match ts.len() {
                1 => true,
                2 => { let (a, b) = (normal(ts[0]), normal(ts[1])); ((a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]).abs() < cosmax }
                _ => false,
            };
            if keep { for x in keys[e].0.iter().chain(keys[e].1.iter()) { out.extend_from_slice(&x.to_le_bytes()); } }
        }
        out
    });
    Ok(pyo3::types::PyBytes::new_bound(py, &bytes))
}

/// rvtviewer.build.smooth_part(pv, pf): weld vertices on a 0.05 mm grid (np.unique over the rounded rows: sorted order, first occurrence), area-weighted vertex normals,
/// corners within the smoothing cone share the vertex normal, the others get their own vertex with the flat face normal. Same operation order as the numpy code.
/// pv (nv,3) float64, pf (mf,3) int64 (C-contiguous, raw addresses) -> (pos bytes, idx bytes, nrm bytes): float64 (np,3), int64 (mf,3), float64 (np,3).
#[pyfunction]
pub fn smooth_part<'py>(py: Python<'py>, pv_ptr: usize, nv: usize, pf_ptr: usize, mf: usize, smooth_cos: f64) -> PyResult<(Bound<'py, pyo3::types::PyBytes>, Bound<'py, pyo3::types::PyBytes>, Bound<'py, pyo3::types::PyBytes>)> {
    use pyo3::types::PyBytes;
    // SAFETY: the Python wrapper keeps both arrays alive and sized nv*3 float64 / mf*3 int64
    let pv: &[f64] = unsafe { std::slice::from_raw_parts(pv_ptr as *const f64, nv * 3) };
    let pf: &[i64] = unsafe { std::slice::from_raw_parts(pf_ptr as *const i64, mf * 3) };
    let (pos, idx, nrm) = py.allow_threads(|| {
        let key = |i: usize| -> [i64; 3] { [(pv[i * 3] * 2e4).round_ties_even() as i64, (pv[i * 3 + 1] * 2e4).round_ties_even() as i64, (pv[i * 3 + 2] * 2e4).round_ties_even() as i64] };
        let keys: Vec<[i64; 3]> = (0..nv).map(key).collect();
        let mut order: Vec<usize> = (0..nv).collect();
        order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));          // stable: the first of equal keys stays first
        let mut inv = vec![0usize; nv];
        let mut first: Vec<usize> = vec![];
        for (r, &i) in order.iter().enumerate() {
            if r == 0 || keys[order[r - 1]] != keys[i] { first.push(i); }
            inv[i] = first.len() - 1;
        }
        let nw = first.len();
        let wf: Vec<[usize; 3]> = (0..mf).map(|f| [inv[pf[f * 3] as usize], inv[pf[f * 3 + 1] as usize], inv[pf[f * 3 + 2] as usize]]).collect();
        let p = |i: i64| -> [f64; 3] { let i = i as usize; [pv[i * 3], pv[i * 3 + 1], pv[i * 3 + 2]] };
        let norm = |v: [f64; 3]| ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt();
        let mut fnv: Vec<[f64; 3]> = Vec::with_capacity(mf);
        let mut fu: Vec<[f64; 3]> = Vec::with_capacity(mf);
        let mut okv: Vec<bool> = Vec::with_capacity(mf);
        for f in 0..mf {
            let (p0, p1, p2) = (p(pf[f * 3]), p(pf[f * 3 + 1]), p(pf[f * 3 + 2]));
            let (a, b) = ([p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]], [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]]);
            let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            let ln = norm(c); let ok = ln > 1e-12;
            fnv.push(c); okv.push(ok); fu.push(if ok { [c[0] / ln, c[1] / ln, c[2] / ln] } else { [0.0; 3] });
        }
        let mut nn = vec![[0.0f64; 3]; nw];
        for k in 0..3 { for f in 0..mf { let t = &mut nn[wf[f][k]]; for j in 0..3 { t[j] += fnv[f][j]; } } }
        for t in nn.iter_mut() { let nl = norm(*t); if nl > 1e-12 { *t = [t[0] / nl, t[1] / nl, t[2] / nl]; } else { *t = [0.0; 3]; } }
        let mut pos: Vec<f64> = Vec::with_capacity((nw + mf) * 3);
        let mut nrm: Vec<f64> = Vec::with_capacity((nw + mf) * 3);
        for &i in &first { pos.extend_from_slice(&pv[i * 3..i * 3 + 3]); }
        for t in &nn { nrm.extend_from_slice(t); }
        let mut idx: Vec<i64> = Vec::with_capacity(mf * 3);
        for f in 0..mf {
            for k in 0..3 {
                let nv_ = nn[wf[f][k]]; let u = fu[f];
                let sm = ((u[0] * nv_[0] + u[1] * nv_[1]) + u[2] * nv_[2]) > smooth_cos;
                if !sm && okv[f] {
                    idx.push((pos.len() / 3) as i64);
                    let i = pf[f * 3 + k] as usize; pos.extend_from_slice(&pv[i * 3..i * 3 + 3]); nrm.extend_from_slice(&u);
                } else { idx.push(wf[f][k] as i64); }
            }
        }
        (pos, idx, nrm)
    });
    let f8 = |v: &Vec<f64>| { let mut b = Vec::with_capacity(v.len() * 8); for x in v { b.extend_from_slice(&x.to_le_bytes()); } b };
    let i8 = |v: &Vec<i64>| { let mut b = Vec::with_capacity(v.len() * 8); for x in v { b.extend_from_slice(&x.to_le_bytes()); } b };
    Ok((PyBytes::new_bound(py, &f8(&pos)), PyBytes::new_bound(py, &i8(&idx)), PyBytes::new_bound(py, &f8(&nrm))))
}
