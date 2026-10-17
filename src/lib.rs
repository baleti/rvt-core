//! rvt_core: hot core of the Revit file reader (own code; Python implementation in ~/rvt-research is the reference oracle).
//! Stage 1: page stripping, gzip-member spans/inflate (parallel), element-header candidate scan.
mod conv;
mod dec;
mod decoder;
mod extract;
mod g103;
mod es;
mod mesh;
mod elemmesh;
mod refvol;
mod pool;
mod params;
mod cache;
mod pack;
mod hlr;
use flate2::bufread::DeflateDecoder;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use rayon::prelude::*;
use std::io::Read;

const PAGE_STORED: usize = 65_249;
const PAGE_PAYLOAD: usize = 64_896;
const GZ: [u8; 4] = [0x1f, 0x8b, 0x08, 0x00];

/// Remove the 353-byte trailer after every FULL stored page (same rule as rvtfmt.strip_pages).
pub fn strip_pages_impl(stored: &[u8]) -> Vec<u8> {
    let full = stored.len() / PAGE_STORED;
    if full == 0 {
        return stored.to_vec();
    }
    let mut out = Vec::with_capacity(stored.len());
    for p in 0..full {
        let s = p * PAGE_STORED;
        out.extend_from_slice(&stored[s..s + PAGE_PAYLOAD]);
    }
    out.extend_from_slice(&stored[full * PAGE_STORED..]);
    out
}

/// test entry: rvt_mesh.select_closed_shell(tris, meta=planes, fidx, prio) -> (kept fidx or None, closed)
#[pyfunction]
fn select_closed_shell_dbg(tris: Vec<[[f64; 3]; 3]>, fidx: Vec<i64>, prio: std::collections::HashMap<i64, bool>) -> (Option<Vec<i64>>, bool) {
    let meta: Vec<mesh::Meta> = tris.iter().map(|_| mesh::Meta { kind: "plane", ev: mesh::MetaEv::None, uv: None }).collect();
    let pr: rustc_hash::FxHashMap<i64, bool> = prio.into_iter().collect();
    let (_, _, ok, lf) = mesh::select_closed_shell(&tris, &meta, &fidx, Some(&pr));
    (lf, ok)
}

#[pyfunction]
fn strip_pages<'py>(py: Python<'py>, stored: &[u8]) -> Bound<'py, PyBytes> {
    PyBytes::new_bound(py, &strip_pages_impl(stored))
}

/// Locate gzip members using the compressed size stored in the 24-byte segment header in front of each member.
/// Returns None when the layout is not the expected one (caller falls back to the Python reader).
fn member_spans(data: &[u8]) -> Option<Vec<(usize, usize)>> {
    let mut spans = Vec::new();
    let mut pos = 0usize;
    loop {
        let i = match memchr::memmem::find(&data[pos..], &GZ) {
            Some(k) => pos + k,
            None => return Some(spans),
        };
        if i >= 24 {
            let size = u32::from_le_bytes(data[i - 16..i - 12].try_into().ok()?) as usize;
            if size >= 18 && i + size - 8 <= data.len() {
                spans.push((i, i + size - 8));
                pos = i + size - 8;
                continue;
            }
        }
        return None;
    }
}

fn inflate_one(data: &[u8], sp: (usize, usize)) -> Option<Vec<u8>> {
    // gzip header is 10 bytes (no optional fields in these files); raw deflate follows, 8-byte trailer at the end
    let body = &data[sp.0 + 10..sp.1];
    let mut dec = DeflateDecoder::new(body);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).ok()?;
    let consumed = body.len() - dec.into_inner().len();
    if body.len() - consumed > 8 {
        return None;
    }
    Some(out)
}

/// strip pages, inflate every member in parallel, concatenate. None if the layout is unexpected.
pub fn joined_vec(stored: &[u8]) -> Option<Vec<u8>> {
    let data = strip_pages_impl(stored);
    let spans = member_spans(&data)?;
    let outs: Vec<Option<Vec<u8>>> = crate::pool::run(|| spans.par_iter().map(|sp| inflate_one(&data, *sp)).collect());
    let mut total = 0usize;
    for o in &outs {
        total += o.as_ref()?.len();
    }
    let mut buf = Vec::with_capacity(total);
    for o in outs {
        buf.extend_from_slice(&o?);
    }
    Some(buf)
}

/// joined(stored): strip pages, inflate every member in parallel, concatenate. None if the layout is unexpected.
#[pyfunction]
fn joined<'py>(py: Python<'py>, stored: &[u8]) -> Option<Bound<'py, PyBytes>> {
    let buf = py.allow_threads(|| joined_vec(stored))?;
    Some(PyBytes::new_bound(py, &buf))
}

/// worker threads: RVT_THREADS, default min(6, cpus)
fn init_pool() {
    let _ = rayon::ThreadPoolBuilder::new().num_threads(pool::threads()).build_global();
}

/// Positions i where chunk[i+4 .. i+4+W] (little endian, any alignment) is in the sorted id list; ascending, i in [0, n-24].
#[pyfunction]
fn header_candidates(py: Python<'_>, chunk: &[u8], ids_sorted: Vec<u64>, w: usize) -> Vec<usize> {
    let n = chunk.len();
    if n < 32 || (w != 4 && w != 8) {
        return vec![];
    }
    let mut out: Vec<usize> = py.allow_threads(|| crate::pool::run(|| {
        (0..n.saturating_sub(w + 4))
            .into_par_iter()
            .filter_map(|p| {
                // value starts at p (so header starts at p-4)
                if p < 4 || p + w > n || p - 4 > n.saturating_sub(24) {
                    return None;
                }
                let v = if w == 4 { u32::from_le_bytes(chunk[p..p + 4].try_into().unwrap()) as u64 } else { u64::from_le_bytes(chunk[p..p + 8].try_into().unwrap()) };
                if ids_sorted.binary_search(&v).is_ok() { Some(p - 4) } else { None }
            })
            .collect()
    }));
    out.sort_unstable();
    out.dedup();
    out
}

#[pymodule]
fn rvt_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    init_pool();
    m.add_function(wrap_pyfunction!(strip_pages, m)?)?;
    m.add_function(wrap_pyfunction!(select_closed_shell_dbg, m)?)?;
    m.add_function(wrap_pyfunction!(elemmesh::refined_volume, m)?)?;
    m.add_function(wrap_pyfunction!(elemmesh::tessellate_geometry_py, m)?)?;
    m.add_function(wrap_pyfunction!(elemmesh::index_handles, m)?)?;
    m.add_function(wrap_pyfunction!(hlr::hlr_raster, m)?)?;
    m.add_function(wrap_pyfunction!(hlr::hlr_visible, m)?)?;
    m.add_function(wrap_pyfunction!(hlr::feature_edges, m)?)?;
    m.add_function(wrap_pyfunction!(hlr::smooth_part, m)?)?;
    m.add_function(wrap_pyfunction!(elemmesh::body_summary_core, m)?)?;
    m.add_function(wrap_pyfunction!(elemmesh::refine_curved_dbg, m)?)?;
    m.add_function(wrap_pyfunction!(joined, m)?)?;
    m.add_function(wrap_pyfunction!(header_candidates, m)?)?;
    m.add_class::<decoder::RecDecoder>()?;
    m.add_class::<params::FlatSrc>()?;
    m.add_function(wrap_pyfunction!(pack::pack, m)?)?;
    m.add_function(wrap_pyfunction!(pack::unpack, m)?)?;
    m.add_function(wrap_pyfunction!(params::ifc_stored, m)?)?;
    m.add_function(wrap_pyfunction!(params::ifc_from_unique, m)?)?;
    Ok(())
}
