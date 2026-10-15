//! Compact binary serialisation of plain Python results (params dicts, compact mesh dicts) for the binary cache:
//! None, bool, int (i64 / big as decimal text), float, str, bytes, list, tuple, dict, rvt_decode.EID, numpy arrays (dtype str + shape + raw bytes).
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

fn put(py: Python<'_>, o: &mut Vec<u8>, x: &Bound<'_, PyAny>, eid: &Bound<'_, PyAny>, nd: &Bound<'_, PyAny>) -> PyResult<()> {
    if x.is_none() { o.push(0); return Ok(()); }
    if x.is_instance_of::<PyBool>() { o.push(if x.is_truthy()? { 2 } else { 1 }); return Ok(()); }
    if x.is_instance(eid)? {
        let v: i128 = x.extract()?;
        o.push(10); o.extend_from_slice(&v.to_le_bytes()); return Ok(());
    }
    if x.is_instance_of::<PyInt>() {
        match x.extract::<i64>() {
            Ok(v) => { o.push(3); o.extend_from_slice(&v.to_le_bytes()); }
            Err(_) => { let s: String = x.str()?.extract()?; o.push(4); o.extend_from_slice(&(s.len() as u32).to_le_bytes()); o.extend_from_slice(s.as_bytes()); }
        }
        return Ok(());
    }
    if x.is_instance_of::<PyFloat>() { o.push(5); o.extend_from_slice(&x.extract::<f64>()?.to_le_bytes()); return Ok(()); }
    if let Ok(s) = x.downcast::<PyString>() { let s: String = s.extract()?; o.push(6); o.extend_from_slice(&(s.len() as u32).to_le_bytes()); o.extend_from_slice(s.as_bytes()); return Ok(()); }
    if let Ok(b) = x.downcast::<PyBytes>() { let b = b.as_bytes(); o.push(7); o.extend_from_slice(&(b.len() as u32).to_le_bytes()); o.extend_from_slice(b); return Ok(()); }
    if let Ok(l) = x.downcast::<PyList>() { o.push(8); o.extend_from_slice(&(l.len() as u32).to_le_bytes()); for e in l.iter() { put(py, o, &e, eid, nd)?; } return Ok(()); }
    if let Ok(t) = x.downcast::<PyTuple>() { o.push(9); o.extend_from_slice(&(t.len() as u32).to_le_bytes()); for e in t.iter() { put(py, o, &e, eid, nd)?; } return Ok(()); }
    if let Ok(d) = x.downcast::<PyDict>() { o.push(11); o.extend_from_slice(&(d.len() as u32).to_le_bytes()); for (k, v) in d.iter() { put(py, o, &k, eid, nd)?; put(py, o, &v, eid, nd)?; } return Ok(()); }
    if x.is_instance(nd)? {
        let a = x.call_method0("copy")?; // C-contiguous
        let dt: String = a.getattr("dtype")?.getattr("str")?.extract()?;
        let shape: Vec<u64> = a.getattr("shape")?.extract()?;
        let raw: Vec<u8> = a.call_method0("tobytes")?.extract()?;
        o.push(12);
        o.extend_from_slice(&(dt.len() as u32).to_le_bytes()); o.extend_from_slice(dt.as_bytes());
        o.extend_from_slice(&(shape.len() as u32).to_le_bytes()); for s in shape { o.extend_from_slice(&s.to_le_bytes()); }
        o.extend_from_slice(&(raw.len() as u64).to_le_bytes()); o.extend_from_slice(&raw);
        return Ok(());
    }
    Err(PyTypeError::new_err(format!("pack: unsupported type {}", x.get_type().name()?)))
}

struct R<'a> { b: &'a [u8], p: usize }
impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> PyResult<&'a [u8]> {
        if self.p + n > self.b.len() { return Err(PyValueError::new_err("truncated pack")); }
        let s = &self.b[self.p..self.p + n]; self.p += n; Ok(s)
    }
    fn u32(&mut self) -> PyResult<usize> { Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize) }
    fn s(&mut self) -> PyResult<&'a str> { let n = self.u32()?; std::str::from_utf8(self.take(n)?).map_err(|_| PyValueError::new_err("bad utf8")) }
    fn get(&mut self, py: Python<'_>, eid: &Bound<'_, PyAny>, np: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let t = self.take(1)?[0];
        Ok(match t {
            0 => py.None(),
            1 => false.into_py(py),
            2 => true.into_py(py),
            3 => i64::from_le_bytes(self.take(8)?.try_into().unwrap()).into_py(py),
            4 => py.get_type_bound::<PyInt>().call1((self.s()?,))?.unbind(),
            5 => f64::from_le_bytes(self.take(8)?.try_into().unwrap()).into_py(py),
            6 => self.s()?.into_py(py),
            7 => { let n = self.u32()?; PyBytes::new_bound(py, self.take(n)?).into_py(py) }
            8 | 9 => {
                let n = self.u32()?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n { v.push(self.get(py, eid, np)?); }
                if t == 8 { PyList::new_bound(py, v).into_py(py) } else { PyTuple::new_bound(py, v).into_py(py) }
            }
            10 => eid.call1((i128::from_le_bytes(self.take(16)?.try_into().unwrap()),))?.unbind(),
            11 => {
                let n = self.u32()?;
                let d = PyDict::new_bound(py);
                for _ in 0..n { let k = self.get(py, eid, np)?; let v = self.get(py, eid, np)?; d.set_item(k, v)?; }
                d.into_py(py)
            }
            12 => {
                let dt = self.s()?.to_string();
                let nd = self.u32()?;
                let mut shape = Vec::with_capacity(nd);
                for _ in 0..nd { shape.push(u64::from_le_bytes(self.take(8)?.try_into().unwrap())); }
                let n = u64::from_le_bytes(self.take(8)?.try_into().unwrap()) as usize;
                let raw = self.take(n)?;
                let a = np.getattr("frombuffer")?.call1((pyo3::types::PyByteArray::new_bound(py, raw), dt))?;   // writable, one copy
                a.call_method1("reshape", (PyTuple::new_bound(py, shape),))?.unbind()
            }
            _ => return Err(PyValueError::new_err(format!("bad pack tag {}", t))),
        })
    }
}

/// pack(obj, eid_type) -> bytes
#[pyfunction]
pub fn pack<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>, eid_type: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let nd = py.import_bound("numpy")?.getattr("ndarray")?;
    let mut o = Vec::new();
    put(py, &mut o, obj, eid_type, &nd)?;
    Ok(PyBytes::new_bound(py, &o))
}

/// unpack(data, eid_type) -> obj  (data: bytes or a path to a file, which is memory-mapped)
#[pyfunction]
pub fn unpack<'py>(py: Python<'py>, data: &Bound<'py, PyAny>, eid_type: &Bound<'py, PyAny>) -> PyResult<PyObject> {
    let np = py.import_bound("numpy")?;
    if let Ok(b) = data.downcast::<PyBytes>() {
        let mut r = R { b: b.as_bytes(), p: 0 };
        return r.get(py, eid_type, &np);
    }
    let path: String = data.extract()?;
    let f = std::fs::File::open(&path)?;
    let mm = unsafe { memmap2::Mmap::map(&f)? };
    let mut r = R { b: &mm, p: 0 };
    r.get(py, eid_type, &np)
}
