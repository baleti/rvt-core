//! Kernels of the parameter layer, walking the Python record objects directly (works for records from any source, incl. the reader's pickle caches):
//! rvt_ifc.stored, rvt_map2.Src (+ leaves/flat), rvt_map2.resolve. Same results as the Python functions.
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use rustc_hash::FxHashMap;
use std::sync::Mutex;

const IFC_PID: u64 = (-1019000i64) as u64;
const IFC_PID32: u64 = ((-1019000i64) as u64) & 0xffff_ffff;

fn walk_strings<'py>(x: &Bound<'py, PyAny>, out: &mut Vec<Bound<'py, PyAny>>, py: Python<'py>) -> PyResult<()> {
    if let Ok(d) = x.downcast::<PyDict>() {
        if let Some(ps) = d.get_item("m_paramSet")? {
            for e in ps.iter()? {
                let e = e?;
                let ed = match e.downcast::<PyDict>() { Ok(ed) => ed, Err(_) => continue };
                let pid = ed.get_item("m_paramId")?;
                let v = ed.get_item("m_value")?;
                if let Some(pid) = pid {
                    if pid.is_none() { continue; }
                    let iv: i128 = py.get_type_bound::<PyInt>().call1((pid.clone(),))?.extract::<i128>()?;
                    let m = (iv & 0xffff_ffff_ffff_ffff) as u64;
                    if m == IFC_PID || m == IFC_PID32 {
                        if let Some(v) = v {
                            if let Ok(s) = v.downcast::<PyString>() { if s.len()? > 0 { out.push(v.clone()); } }
                        }
                    }
                }
            }
        }
        for b in d.values().iter() { walk_strings(&b, out, py)?; }
    } else if let Ok(l) = x.downcast::<PyList>() {
        for b in l.iter() { walk_strings(&b, out, py)?; }
    } else if let Ok(t) = x.downcast::<PyTuple>() {
        for b in t.iter() { walk_strings(&b, out, py)?; }
    }
    Ok(())
}

/// == rvt_ifc.stored(recs): {eid: last stored IfcGUID string among all records}; recs = {eid: [record, ...]}
#[pyfunction]
pub fn ifc_stored<'py>(py: Python<'py>, recs: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    for (eid, rl) in recs.iter() {
        for r in rl.iter()? {
            let r = r?;
            let f = r.get_item("fields")?;
            let mut s = vec![];
            walk_strings(&f, &mut s, py)?;
            if let Some(last) = s.pop() { out.set_item(&eid, last)?; }
        }
    }
    Ok(out)
}

#[derive(Default)]
struct Flat {
    out: Vec<(String, PyObject)>,
    pos: FxHashMap<String, usize>,
    eids: FxHashMap<String, PyObject>,
    eorder: Vec<String>,
}
impl Flat {
    fn set(&mut self, k: String, v: PyObject) {
        match self.pos.get(&k) { Some(&i) => self.out[i].1 = v, None => { self.pos.insert(k.clone(), self.out.len()); self.out.push((k, v)); } }
    }
    fn set_eid(&mut self, k: String, v: PyObject) {
        if !self.eids.contains_key(&k) { self.eorder.push(k.clone()); }
        self.eids.insert(k, v);
    }
}

fn leaves(py: Python<'_>, v: &Bound<'_, PyAny>, prefix: &str, out: &mut Flat, depth: usize, eid_t: &Bound<'_, PyAny>) -> PyResult<()> {
    if depth > 5 { return Ok(()); }
    if let Ok(d) = v.downcast::<PyDict>() {
        let has = |k: &str| d.contains(k).unwrap_or(false);
        let only_handle_ref = d.iter().all(|(k, _)| k.extract::<String>().map_or(false, |s| s == "handle" || s == "ref"));
        if (has("handle") && (has("ptr") || has("es"))) || only_handle_ref {
            if let Some(sub) = d.get_item("obj")? {
                if sub.is_instance_of::<PyDict>() && prefix.chars().count() < 80 { leaves(py, &sub, &format!("{}*", prefix), out, depth + 1, eid_t)?; }
            }
            return Ok(());
        }
        for (k, x) in d.iter() {
            let ks: String = match k.downcast::<PyString>() { Ok(s) => s.extract::<String>()?, Err(_) => return Err(PyTypeError::new_err("non-str key")) };
            if ks.starts_with('_') { continue; }
            let p = if prefix.is_empty() { ks } else { format!("{}.{}", prefix, ks) };
            leaves(py, &x, &p, out, depth + 1, eid_t)?;
        }
    } else if let Ok(l) = v.downcast::<PyList>() {
        if l.len() <= 6 {
            for (i, x) in l.iter().enumerate() { leaves(py, &x, &format!("{}[{}]", prefix, i), out, depth + 1, eid_t)?; }
        }
    } else if v.is_instance(eid_t)? {
        let iv = py.get_type_bound::<PyInt>().call1((v.clone(),))?.unbind();
        out.set_eid(prefix.to_string(), iv.clone_ref(py));
        out.set(prefix.to_string(), iv);
    } else if v.is_instance_of::<pyo3::types::PyBool>() {
        let iv = py.get_type_bound::<PyInt>().call1((v.clone(),))?.unbind();
        out.set(prefix.to_string(), iv);
    } else if v.is_instance_of::<PyInt>() || v.is_instance_of::<PyFloat>() || v.is_instance_of::<PyString>() {
        out.set(prefix.to_string(), v.clone().unbind());
    }
    Ok(())
}

/// == rvt_map2.Src: per-record flattened view (leaves of the record + 'H'-prefixed leaves of its ElementHeader), cached per id
#[pyclass]
pub struct FlatSrc {
    #[pyo3(get)]
    last: Py<PyDict>,
    #[pyo3(get)]
    hdr: Py<PyDict>,
    eid_t: Py<PyAny>,
    cache: Mutex<FxHashMap<i128, std::sync::Arc<Flat>>>,
    /// compiled rvt_params MAPPING: class -> [(parameter name, candidates)]
    mapping: FxHashMap<String, Vec<(PyObject, Vec<(Slot, Cand)>)>>,
    sent: Vec<i128>,
    pnames: Option<Py<PyDict>>,
}

enum Cand {
    Default(PyObject),
    Res(String, String),
}

/// positional parameter-set slot guard of a candidate: (via, path of the slot's m_paramId, int(pid))
type Slot = Option<(String, String, Option<i128>)>;

fn sid(v: i128) -> i128 { if v >= 1 << 63 { v - (1 << 64) } else if (1 << 31..1 << 32).contains(&v) { v - (1 << 32) } else { v } }

impl FlatSrc {
    fn flat_(&self, py: Python<'_>, rid: &Bound<'_, PyAny>) -> PyResult<std::sync::Arc<Flat>> {
        let key: i128 = rid.extract().unwrap_or(i128::MIN);
        if let Some(f) = self.cache.lock().unwrap().get(&key) { return Ok(f.clone()); }
        let mut fl = Flat::default();
        let eid_t = self.eid_t.bind(py);
        if let Some(r) = self.last.bind(py).get_item(rid)? {
            leaves(py, &r.get_item("fields")?, "", &mut fl, 0, eid_t)?;
            if let Some(h) = self.hdr.bind(py).get_item(rid)? {
                if !h.is_none() { leaves(py, &h.get_item("fields")?, "H", &mut fl, 0, eid_t)?; }
            }
        }
        let a = std::sync::Arc::new(fl);
        self.cache.lock().unwrap().insert(key, a.clone());
        Ok(a)
    }
}

#[pymethods]
impl FlatSrc {
    #[new]
    #[pyo3(signature = (last, hdr=None, eid_type=None))]
    fn new(py: Python<'_>, last: Py<PyDict>, hdr: Option<Py<PyDict>>, eid_type: Option<Py<PyAny>>) -> PyResult<Self> {
        let eid_t = match eid_type { Some(t) => t, None => py.import_bound("rvt_decode")?.getattr("EID")?.unbind() };
        Ok(FlatSrc { last, hdr: hdr.unwrap_or_else(|| PyDict::new_bound(py).unbind()), eid_t, cache: Mutex::new(FxHashMap::default()), mapping: FxHashMap::default(), sent: vec![], pnames: None })
    }

    /// == Src.flat(rid) -> (out, eids) as new dicts
    fn flat<'py>(&self, py: Python<'py>, rid: &Bound<'py, PyAny>) -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyDict>)> {
        let f = self.flat_(py, rid)?;
        let o = PyDict::new_bound(py);
        for (k, v) in &f.out { o.set_item(k, v.bind(py))?; }
        let e = PyDict::new_bound(py);
        for k in &f.eorder { e.set_item(k, f.eids[k].bind(py))?; }
        Ok((o, e))
    }

    /// value of a leaves() path (None if leaves would not produce it); eid_only: the path must hold an EID (Src.flat()[1])
    fn lookup(&self, py: Python<'_>, rid: &Bound<'_, PyAny>, path: &str, eid_only: bool) -> PyResult<Option<PyObject>> {
        let (root, rest): (Bound<'_, PyAny>, &str) = if path == "H" || path.starts_with("H.") || path.starts_with("H*") || path.starts_with("H[") {
            match self.hdr.bind(py).get_item(rid)? { Some(h) if !h.is_none() => (h.get_item("fields")?, &path[1..]), _ => return Ok(None) }
        } else {
            match self.last.bind(py).get_item(rid)? { Some(r) => (r.get_item("fields")?, path), None => return Ok(None) }
        };
        // header leaves are written after the record's: a record path 'H...' is only reachable when no header exists (practically never)
        let mut cur = root;
        let mut depth = 0usize;
        let mut pos = 0usize;
        let b = rest.as_bytes();
        let prefix_len = |p: usize| -> usize { (path.len() - rest.len()) + p };
        loop {
            if depth > 5 { return Ok(None); }
            if pos >= b.len() {
                // leaf
                if cur.is_instance(self.eid_t.bind(py))? { return Ok(Some(py.get_type_bound::<PyInt>().call1((cur,))?.unbind())); }
                if eid_only { return Ok(None); }
                if cur.is_instance_of::<pyo3::types::PyBool>() { return Ok(Some(py.get_type_bound::<PyInt>().call1((cur,))?.unbind())); }
                if cur.is_instance_of::<PyInt>() || cur.is_instance_of::<PyFloat>() || cur.is_instance_of::<PyString>() { return Ok(Some(cur.unbind())); }
                return Ok(None);
            }
            if let Ok(d) = cur.downcast::<PyDict>() {
                let has = |k: &str| d.contains(k).unwrap_or(false);
                let only_handle_ref = d.iter().all(|(k, _)| k.extract::<String>().map_or(false, |s| s == "handle" || s == "ref"));
                if (has("handle") && (has("ptr") || has("es"))) || only_handle_ref {
                    if b[pos] != b'*' { return Ok(None); }
                    let sub = match d.get_item("obj")? { Some(x) if x.is_instance_of::<PyDict>() => x, _ => return Ok(None) };
                    if path[..prefix_len(pos)].chars().count() >= 80 { return Ok(None); }
                    cur = sub; pos += 1; depth += 1; continue;
                }
                // key step: '.name' (or 'name' at the root)
                let start = if pos == 0 && prefix_len(0) == 0 { 0 } else { if b[pos] != b'.' { return Ok(None); } pos + 1 };
                let mut e = start;
                while e < b.len() && b[e] != b'.' && b[e] != b'*' && b[e] != b'[' { e += 1; }
                let k = &rest[start..e];
                if k.starts_with('_') || k.is_empty() { return Ok(None); }
                cur = match d.get_item(k)? { Some(x) => x, None => return Ok(None) };
                pos = e; depth += 1; continue;
            }
            if let Ok(l) = cur.downcast::<PyList>() {
                if l.len() > 6 || b[pos] != b'[' { return Ok(None); }
                let mut e = pos + 1;
                while e < b.len() && b[e] != b']' { e += 1; }
                if e >= b.len() { return Ok(None); }
                let i: usize = match rest[pos + 1..e].parse() { Ok(i) => i, Err(_) => return Ok(None) };
                if i >= l.len() { return Ok(None); }
                cur = l.get_item(i)?; pos = e + 1; depth += 1; continue;
            }
            return Ok(None);
        }
    }

    /// compile rvt_params.MAPPING with PNAMES ('NAME' -> 'Name', unknown ids skipped) and rvt_map2.SENT
    fn set_mapping(&mut self, mapping: &Bound<'_, PyDict>, pnames: &Bound<'_, PyDict>, sent: Vec<i128>) -> PyResult<()> {
        self.mapping.clear();
        for (c, m) in mapping.iter() {
            let c: String = c.extract()?;
            let mut v = vec![];
            for (pid, cands) in m.downcast::<PyDict>()?.iter() {
                let pids: String = pid.extract()?;
                let nm: PyObject = if pids == "NAME" { "Name".into_py(pid.py()) } else {
                    match pnames.get_item(&pid)? { Some(n) if !n.is_none() => n.unbind(), _ => continue }
                };
                let mut cl = vec![];
                for cd in cands.iter()? {
                    let cd = cd?;
                    let cd = cd.downcast::<PyDict>()?;
                    let via: String = match cd.get_item("via")? { Some(x) => x.extract()?, None => String::new() };
                    let path0: String = match cd.get_item("path")? { Some(x) => x.extract()?, None => String::new() };
                    // positional slots of the stored parameter sets shift per element: use a slot only when it holds this parameter
                    let mut slot: Slot = None;
                    if path0.contains("m_pParamValueSet") {
                        if pids == "NAME" || !path0.ends_with(".m_value") { continue; }
                        slot = Some((via.clone(), format!("{}.m_paramId", &path0[..path0.len() - ".m_value".len()]), pids.trim().parse::<i128>().ok()));
                    }
                    if let Some(dv) = cd.get_item("default")? { cl.push((slot, Cand::Default(dv.unbind()))); continue; }
                    if cd.get_item("path")?.is_none() { return Err(pyo3::exceptions::PyKeyError::new_err("path")); }
                    cl.push((slot, Cand::Res(via, path0)));
                }
                v.push((nm, cl));
            }
            self.mapping.insert(c, v);
        }
        self.sent = sent;
        self.pnames = Some(pnames.clone().unbind());
        Ok(())
    }

    /// the MAPPING loop of rvt_params.params for one element -> new dict d
    fn apply<'py>(&self, py: Python<'py>, rid: &Bound<'py, PyAny>, cls: &str) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        let m = match self.mapping.get(cls) { Some(m) => m, None => return Ok(d) };
        for (nm, cands) in m {
            for (slot, cd) in cands {
                if let Some((sv, sp, pid)) = slot {
                    let v = self.resolve(py, rid, sv, sp)?;
                    let vb = v.bind(py);
                    if vb.is_none() { continue; }
                    let iv: i128 = sid(py.get_type_bound::<PyInt>().call1((vb,))?.extract()?);
                    let pid = pid.ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid literal for int()"))?;
                    if iv != pid {
                        let other = match &self.pnames { Some(pn) => pn.bind(py).get_item(iv.to_string())?, None => None };
                        if let Some(o) = other { if !o.is_none() && !o.eq(nm.bind(py))? { continue; } }
                    }
                }
                match cd {
                    Cand::Default(v) => { d.set_item(nm.bind(py), v.bind(py))?; break; }
                    Cand::Res(via, path) => {
                        let v = self.resolve(py, rid, via, path)?;
                        let vb = v.bind(py);
                        if vb.is_none() { continue; }
                        if vb.is_instance_of::<PyInt>() {
                            let iv: i128 = vb.extract().unwrap_or(i128::MIN);
                            if self.sent.contains(&iv) { continue; }
                        }
                        d.set_item(nm.bind(py), vb)?;
                        break;
                    }
                }
            }
        }
        Ok(d)
    }

    /// == rvt_map2.resolve(src, rid, via, path)
    fn resolve(&self, py: Python<'_>, rid: &Bound<'_, PyAny>, via: &str, path: &str) -> PyResult<PyObject> {
        let mut cur = rid.clone().unbind();
        let last = self.last.bind(py);
        for hop in via.split('>').filter(|h| !h.is_empty()) {
            let nxt = match self.lookup(py, cur.bind(py), hop, true)? { Some(n) => n, None => return Ok(py.None()) };
            if !last.contains(nxt.bind(py))? { return Ok(py.None()); }
            cur = nxt;
        }
        Ok(self.lookup(py, cur.bind(py), path, false)?.unwrap_or_else(|| py.None()))
    }

}

/// == ifcguid.from_unique(unique_id)
#[pyfunction]
pub fn ifc_from_unique(u: &str) -> PyResult<String> {
    const C: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
    let chars: Vec<char> = u.chars().collect();
    let g: String = chars.iter().take(36).collect();
    let tail: String = chars.iter().skip(37).collect();
    let eid = u128::from_str_radix(tail.trim(), 16).map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid literal for int() with base 16"))?;
    let h = g.replace("urn:", "").replace("uuid:", "");
    let h = h.trim_matches(|c| c == '{' || c == '}').replace('-', "");
    if h.len() != 32 || !h.chars().all(|c| c.is_ascii_hexdigit()) { return Err(pyo3::exceptions::PyValueError::new_err("badly formed hexadecimal UUID string")); }
    let mut n = u128::from_str_radix(&h, 16).unwrap();
    n ^= (eid & 0xffff_ffff) as u128;
    let mut out = String::with_capacity(22);
    out.push(C[((n >> 126) & 3) as usize] as char);
    for i in 0..21 { out.push(C[((n >> (120 - 6 * i)) & 63) as usize] as char); }
    Ok(out)
}
