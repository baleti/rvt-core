//! Val -> Python objects, identical (==, key order, types incl. rvt_decode.EID) to what rvt_decode.Dec builds.
use crate::dec::{Rec, Schema, Slot, SlotKind, Val, K_ES, K_HANDLE, K_OBJ, K_PTR};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};

pub struct Conv {
    pub keys: Vec<Py<PyString>>,
    pub cnames: Vec<Py<PyString>>,
    pub eid: Py<PyAny>,
    geo: Py<PyString>,
    rk: [Py<PyString>; 8],
}

impl Conv {
    pub fn new(py: Python<'_>, s: &Schema, eid: Py<PyAny>) -> Conv {
        let keys = s.keys.iter().map(|k| PyString::intern_bound(py, k).unbind()).collect();
        let cnames = s.classes.iter().map(|c| PyString::intern_bound(py, &c.name).unbind()).collect();
        let rk = ["id", "tag", "class", "fields", "complete", "error", "bodies", "short"].map(|k| PyString::intern_bound(py, k).unbind());
        Conv { keys, cnames, eid, geo: PyString::new_bound(py, "<geometry not decoded>").unbind(), rk }
    }
    /// keys added to the schema after this converter was built (ES field names)
    pub fn sync(&mut self, py: Python<'_>, s: &Schema) {
        while self.keys.len() < s.keys.len() {
            let k = &s.keys[self.keys.len()];
            self.keys.push(PyString::intern_bound(py, k).unbind());
        }
    }

    pub fn cname<'py>(&self, py: Python<'py>, tag: u32) -> PyObject {
        let i = tag as i64 - 12;
        if i >= 0 && (i as usize) < self.cnames.len() { self.cnames[i as usize].clone_ref(py).into_any() } else { format!("#{}", tag).into_py(py) }
    }

    pub fn val(&self, py: Python<'_>, v: &Val, slots: &[Slot]) -> PyResult<PyObject> {
        Ok(match v {
            Val::None => py.None(),
            Val::I(x) => x.into_py(py),
            Val::U(x) => x.into_py(py),
            Val::F(x) => x.into_py(py),
            Val::S(s) => s.into_py(py),
            Val::B(b) => PyBytes::new_bound(py, b).into_py(py),
            Val::L(l) => {
                let mut items = Vec::with_capacity(l.len());
                for x in l {
                    items.push(self.val(py, x, slots)?);
                }
                PyList::new_bound(py, items).into_py(py)
            }
            Val::T(l) => {
                let mut items = Vec::with_capacity(l.len());
                for x in l {
                    items.push(self.val(py, x, slots)?);
                }
                PyTuple::new_bound(py, items).into_py(py)
            }
            Val::D(d) => {
                let o = PyDict::new_bound(py);
                for (k, x) in d {
                    o.set_item(self.keys[*k as usize].bind(py), self.val(py, x, slots)?)?;
                }
                o.into_py(py)
            }
            Val::EI(x) => self.eid.bind(py).call1((*x,))?.unbind(),
            Val::EU(x) => self.eid.bind(py).call1((*x,))?.unbind(),
            Val::Slot(i) => {
                let s = &slots[*i as usize];
                let o = PyDict::new_bound(py);
                o.set_item(self.keys[K_HANDLE as usize].bind(py), s.handle)?;
                match &s.kind {
                    SlotKind::Ptr(t) => o.set_item(self.keys[K_PTR as usize].bind(py), *t)?,
                    SlotKind::Es(g) => o.set_item(self.keys[K_ES as usize].bind(py), g)?,
                }
                if let Some(ob) = &s.obj {
                    o.set_item(self.keys[K_OBJ as usize].bind(py), self.val(py, ob, slots)?)?;
                }
                o.into_py(py)
            }
            Val::Geo => self.geo.clone_ref(py).into_any(),
            Val::R(..) => return Err(pyo3::exceptions::PyRuntimeError::new_err("lazy string reached the converter")),
        })
    }

    pub fn rec<'py>(&self, py: Python<'py>, r: &Rec) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        let k = &self.rk;
        d.set_item(k[0].bind(py), r.id)?;
        d.set_item(k[1].bind(py), r.tag)?;
        d.set_item(k[2].bind(py), self.cname(py, r.tag))?;
        d.set_item(k[3].bind(py), self.val(py, &r.fields, &r.slots)?)?;
        d.set_item(k[4].bind(py), r.complete)?;
        match &r.error {
            Some(e) => d.set_item(k[5].bind(py), e)?,
            None => d.set_item(k[5].bind(py), py.None())?,
        }
        if let Some(b) = &r.bodies {
            let l: Vec<PyObject> = b.iter().map(|t| self.cname(py, *t)).collect();
            d.set_item(k[6].bind(py), PyList::new_bound(py, l))?;
        }
        d.set_item(k[7].bind(py), r.short)?;
        Ok(d)
    }

    /// slim form {'id','class','fields':{}, 'complete'} used by rvt_entities.extract(slim=...)
    pub fn rec_slim<'py>(&self, py: Python<'py>, r: &Rec) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        let k = &self.rk;
        d.set_item(k[0].bind(py), r.id)?;
        d.set_item(k[2].bind(py), self.cname(py, r.tag))?;
        d.set_item(k[3].bind(py), PyDict::new_bound(py))?;
        d.set_item(k[4].bind(py), r.complete)?;
        Ok(d)
    }
}
