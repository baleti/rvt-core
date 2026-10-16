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
        for yy in ymin.max(y_lo)..=ymax.min(y_hi - 1) {
            let py = yy as f64 + 0.5;
            let row = &mut zb[((yy - y_lo) * w) as usize..((yy - y_lo + 1) * w) as usize];
            for xx in xmin..=xmax {
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
