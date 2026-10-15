//! Binary cache of extraction results (no pickle): partition blocks of serialised records, read back (memory-mapped) straight into Python objects
//! with the same first-wins merge across partitions as rvt_entities.extract.
//! File: b"RVTC" u32 format | u32 header length | header (JSON: format, reader version, schema hash, key table, ...) | blocks
//! block: u32 n_recs, n x (u64 eid, u32 canonical tag, u8 slim, rec) | u32 n_eh, n x (u64 eid, val)
use crate::conv::Conv;
use crate::dec::{Rec, Slot, SlotKind, Val};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyFloat, PyList, PyString, PyTuple};

pub const FORMAT: u32 = 1;

pub fn put_val(o: &mut Vec<u8>, v: &Val, slots: &[Slot]) {
    match v {
        Val::None => o.push(0),
        Val::I(x) => { o.push(1); o.extend_from_slice(&x.to_le_bytes()); }
        Val::U(x) => { o.push(2); o.extend_from_slice(&x.to_le_bytes()); }
        Val::F(x) => { o.push(3); o.extend_from_slice(&x.to_le_bytes()); }
        Val::S(s) => { o.push(4); o.extend_from_slice(&(s.len() as u32).to_le_bytes()); o.extend_from_slice(s.as_bytes()); }
        Val::B(b) => { o.push(5); o.extend_from_slice(&(b.len() as u32).to_le_bytes()); o.extend_from_slice(b); }
        Val::L(l) => { o.push(6); o.extend_from_slice(&(l.len() as u32).to_le_bytes()); for x in l { put_val(o, x, slots); } }
        Val::T(l) => { o.push(7); o.extend_from_slice(&(l.len() as u32).to_le_bytes()); for x in l { put_val(o, x, slots); } }
        Val::D(d) => { o.push(8); o.extend_from_slice(&(d.len() as u32).to_le_bytes()); for (k, x) in d { o.extend_from_slice(&k.to_le_bytes()); put_val(o, x, slots); } }
        Val::EI(x) => { o.push(9); o.extend_from_slice(&x.to_le_bytes()); }
        Val::EU(x) => { o.push(10); o.extend_from_slice(&x.to_le_bytes()); }
        Val::Slot(i) => {
            let s = &slots[*i as usize];
            o.push(11);
            o.extend_from_slice(&s.handle.to_le_bytes());
            match &s.kind {
                SlotKind::Ptr(t) => { o.push(0); o.extend_from_slice(&t.to_le_bytes()); }
                SlotKind::Es(g) => { o.push(1); o.extend_from_slice(&(g.len() as u32).to_le_bytes()); o.extend_from_slice(g.as_bytes()); }
            }
            match &s.obj { Some(ob) => { o.push(1); put_val(o, ob, slots); } None => o.push(0) }
        }
        Val::Geo => o.push(12),
        Val::R(..) => o.push(0),
    }
}

pub fn put_rec(o: &mut Vec<u8>, r: &Rec, slim: bool) {
    o.extend_from_slice(&r.id.to_le_bytes());
    o.extend_from_slice(&r.tag.to_le_bytes());
    o.push(r.complete as u8);
    o.push(r.short as u8);
    if slim { o.push(1); return; }
    o.push(0);
    match &r.error { Some(e) => { o.push(1); o.extend_from_slice(&(e.len() as u32).to_le_bytes()); o.extend_from_slice(e.as_bytes()); } None => o.push(0) }
    match &r.bodies { Some(b) => { o.push(1); o.extend_from_slice(&(b.len() as u32).to_le_bytes()); for t in b { o.extend_from_slice(&t.to_le_bytes()); } } None => o.push(0) }
    put_val(o, &r.fields, &r.slots);
}

/// em: one shared rvt_decode.EID instance per value (immutable int subclass: 0.5M ids of a 6 MB file have ~15k distinct values);
/// fm / im / sm: the same for floats (by bits), ints outside CPython's small-int cache and str values (immutable: 40 MB Electrical builds 787k floats with 142k
/// distinct values, 1.2M large ints with 42k, 568k strings with 9.6k; sharing them removes a third of the allocations of a load)
pub struct Rd<'a, 'py> {
    pub b: &'a [u8], pub p: usize, pub em: rustc_hash::FxHashMap<i128, Bound<'py, PyAny>>,
    fm: rustc_hash::FxHashMap<u64, Bound<'py, PyAny>>, im: rustc_hash::FxHashMap<i128, Bound<'py, PyAny>>, sm: rustc_hash::FxHashMap<&'a [u8], Bound<'py, PyAny>>,
}
// values are Bound (GIL-bound): dropping one is a plain decref, no thread-local GIL check as for Py<T> (that check was ~6% of a load)
impl<'a, 'py> Rd<'a, 'py> {
    pub fn new(_py: Python<'py>, b: &'a [u8], p: usize) -> Self { Rd { b, p, em: Default::default(), fm: Default::default(), im: Default::default(), sm: Default::default() } }
    #[inline] fn int(&mut self, py: Python<'py>, x: i128) -> Bound<'py, PyAny> {
        if (-5..=256).contains(&x) { return (x as i64).into_py(py).into_bound(py); }
        if let Some(o) = self.im.get(&x) { return o.clone(); }
        let o = if x < 0 { (x as i64).into_py(py).into_bound(py) } else { (x as u64).into_py(py).into_bound(py) };
        self.im.insert(x, o.clone()); o
    }
    #[inline] fn float(&mut self, py: Python<'py>, bits: u64) -> Bound<'py, PyAny> {
        if let Some(o) = self.fm.get(&bits) { return o.clone(); }
        let o = PyFloat::new_bound(py, f64::from_bits(bits)).into_any(); self.fm.insert(bits, o.clone()); o
    }
    fn strv(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let k = self.u32()? as usize; let raw = self.bytes(k)?;
        if let Some(o) = self.sm.get(raw) { return Ok(o.clone()); }
        let s = std::str::from_utf8(raw).map_err(|_| PyValueError::new_err("bad utf8 in cache"))?;
        let o = PyString::new_bound(py, s).into_any(); self.sm.insert(raw, o.clone()); Ok(o)
    }
    fn eid(&mut self, py: Python<'py>, c: &crate::conv::Conv, x: i128) -> PyResult<Bound<'py, PyAny>> {
        if let Some(o) = self.em.get(&x) { return Ok(o.clone()); }
        let o = if x < 0 { c.eid.bind(py).call1((x as i64,))? } else { c.eid.bind(py).call1((x as u64,))? };
        self.em.insert(x, o.clone());
        Ok(o)
    }
    #[inline] pub fn u8(&mut self) -> PyResult<u8> { let v = *self.b.get(self.p).ok_or_else(|| PyValueError::new_err("truncated cache"))?; self.p += 1; Ok(v) }
    #[inline] fn n<const N: usize>(&mut self) -> PyResult<[u8; N]> {
        if self.p + N > self.b.len() { return Err(PyValueError::new_err("truncated cache")); }
        let a: [u8; N] = self.b[self.p..self.p + N].try_into().unwrap(); self.p += N; Ok(a)
    }
    #[inline] pub fn u32(&mut self) -> PyResult<u32> { Ok(u32::from_le_bytes(self.n::<4>()?)) }
    #[inline] pub fn u64(&mut self) -> PyResult<u64> { Ok(u64::from_le_bytes(self.n::<8>()?)) }
    #[inline] fn bytes(&mut self, k: usize) -> PyResult<&'a [u8]> {
        if self.p + k > self.b.len() { return Err(PyValueError::new_err("truncated cache")); }
        let s = &self.b[self.p..self.p + k]; self.p += k; Ok(s)
    }
    fn str(&mut self) -> PyResult<&'a str> { let k = self.u32()? as usize; std::str::from_utf8(self.bytes(k)?).map_err(|_| PyValueError::new_err("bad utf8 in cache")) }

    pub fn val(&mut self, py: Python<'py>, c: &Conv, keys: &[Py<PyString>], geo: &Bound<'py, PyString>) -> PyResult<Bound<'py, PyAny>> {
        Ok(match self.u8()? {
            0 => py.None().into_bound(py),
            1 => { let x = i64::from_le_bytes(self.n::<8>()?); self.int(py, x as i128) }
            2 => { let x = u64::from_le_bytes(self.n::<8>()?); self.int(py, x as i128) }
            3 => { let x = u64::from_le_bytes(self.n::<8>()?); self.float(py, x) }
            4 => self.strv(py)?,
            5 => { let k = self.u32()? as usize; PyBytes::new_bound(py, self.bytes(k)?).into_any() }
            6 => {
                let n = self.u32()? as usize;
                // filled in place (PyList_SetItem steals the reference): no intermediate Vec
                let l = unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyList_New(n as pyo3::ffi::Py_ssize_t))? };
                for i in 0..n {
                    let v = self.val(py, c, keys, geo)?;
                    if unsafe { pyo3::ffi::PyList_SetItem(l.as_ptr(), i as pyo3::ffi::Py_ssize_t, v.into_ptr()) } != 0 { return Err(PyErr::fetch(py)); }
                }
                l
            }
            7 => {
                let n = self.u32()? as usize;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n { items.push(self.val(py, c, keys, geo)?); }
                PyTuple::new_bound(py, items).into_any()
            }
            8 => {
                let n = self.u32()? as usize;
                let d = PyDict::new_bound(py);
                for _ in 0..n {
                    let k = self.u32()? as usize;
                    let v = self.val(py, c, keys, geo)?;
                    d.set_item(keys.get(k).ok_or_else(|| PyValueError::new_err("bad key"))?.bind(py), v)?;
                }
                d.into_any()
            }
            9 => { let x = i64::from_le_bytes(self.n::<8>()?); self.eid(py, c, x as i128)? }
            10 => { let x = u64::from_le_bytes(self.n::<8>()?); self.eid(py, c, x as i128)? }
            11 => {
                let d = PyDict::new_bound(py);
                d.set_item(keys[crate::dec::K_HANDLE as usize].bind(py), self.u32()?)?;
                if self.u8()? == 0 { d.set_item(keys[crate::dec::K_PTR as usize].bind(py), self.u32()?)?; } else { let g = self.str()?; d.set_item(keys[crate::dec::K_ES as usize].bind(py), g)?; }
                if self.u8()? == 1 { let ob = self.val(py, c, keys, geo)?; d.set_item(keys[crate::dec::K_OBJ as usize].bind(py), ob)?; }
                d.into_any()
            }
            12 => geo.clone().into_any(),
            t => return Err(PyValueError::new_err(format!("bad cache tag {}", t))),
        })
    }

    pub fn rec(&mut self, py: Python<'py>, c: &Conv, keys: &[Py<PyString>], geo: &Bound<'py, PyString>) -> PyResult<Bound<'py, PyDict>> {
        let id = self.u64()?;
        let tag = self.u32()?;
        let complete = self.u8()? != 0;
        let short = self.u8()? != 0;
        let slim = self.u8()? != 0;
        let d = PyDict::new_bound(py);
        d.set_item(pyo3::intern!(py, "id"), id)?;
        if slim {
            d.set_item(pyo3::intern!(py, "class"), c.cname(py, tag))?;
            d.set_item(pyo3::intern!(py, "fields"), PyDict::new_bound(py))?;
            d.set_item(pyo3::intern!(py, "complete"), complete)?;
            return Ok(d);
        }
        let err: Option<&str> = if self.u8()? == 1 { Some(self.str()?) } else { None };
        let bodies: Option<Vec<u32>> = if self.u8()? == 1 { let n = self.u32()? as usize; let mut v = Vec::with_capacity(n); for _ in 0..n { v.push(self.u32()?); } Some(v) } else { None };
        let fields = self.val(py, c, keys, geo)?;
        d.set_item(pyo3::intern!(py, "tag"), tag)?;
        d.set_item(pyo3::intern!(py, "class"), c.cname(py, tag))?;
        d.set_item(pyo3::intern!(py, "fields"), fields)?;
        d.set_item(pyo3::intern!(py, "complete"), complete)?;
        match err { Some(e) => d.set_item(pyo3::intern!(py, "error"), e)?, None => d.set_item(pyo3::intern!(py, "error"), py.None())? }
        if let Some(b) = bodies { d.set_item(pyo3::intern!(py, "bodies"), PyList::new_bound(py, b.iter().map(|t| c.cname(py, *t))))?; }
        d.set_item(pyo3::intern!(py, "short"), short)?;
        Ok(d)
    }
}
