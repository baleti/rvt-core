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
}

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
        Ok(FlatSrc { last, hdr: hdr.unwrap_or_else(|| PyDict::new_bound(py).unbind()), eid_t, cache: Mutex::new(FxHashMap::default()) })
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

    /// == rvt_map2.resolve(src, rid, via, path)
    fn resolve(&self, py: Python<'_>, rid: &Bound<'_, PyAny>, via: &str, path: &str) -> PyResult<PyObject> {
        let mut cur = rid.clone().unbind();
        let last = self.last.bind(py);
        for hop in via.split('>').filter(|h| !h.is_empty()) {
            let f = self.flat_(py, cur.bind(py))?;
            let nxt = match f.eids.get(hop) { Some(n) => n.clone_ref(py), None => return Ok(py.None()) };
            if !last.contains(nxt.bind(py))? { return Ok(py.None()); }
            cur = nxt;
        }
        let f = self.flat_(py, cur.bind(py))?;
        Ok(match f.pos.get(path) { Some(&i) => f.out[i].1.clone_ref(py), None => py.None() })
    }
}
