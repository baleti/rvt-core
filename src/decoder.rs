//! Schema-driven record decoder: port of ~/rvt-research/rvt_decode.py (default configuration: no hints, no GG/M experiments).
//! Output objects are built directly as Python dicts/lists so that results compare equal (==) to the Python reference.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone)]
struct Field {
    name: String,
    t0: u32,
    types: Vec<u32>,
    ns: Vec<Option<u32>>,
    reff: Option<u32>,
    ref2: Option<u32>,
}

#[derive(Clone)]
struct Class {
    name: String,
    parent: u32,
    fields: Vec<Field>,
}

#[derive(Clone)]
struct EsField {
    name: String,
    entry: i64,
    container: i64,
    tname: String,
}

enum Err {
    Desync(String),
    Other(String),
}
type R<T> = Result<T, Err>;

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}
fn hex_sp(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect::<Vec<_>>().join(" ")
}

#[pyclass]
pub struct RecDecoder {
    classes: Vec<Class>,
    idsize: usize,
    stop: Vec<String>,
    strcap: usize,
    es: HashMap<String, Vec<EsField>>,
    eid_type: Py<PyAny>,
    desync_type: Py<PyAny>,
    chain_cache: HashMap<usize, Vec<usize>>,
    defined: HashSet<u32>,
    visited: Vec<String>,
    err: Option<String>,
    path: Vec<String>,
}

type Pend<'py> = Vec<(Bound<'py, PyDict>, PendKind)>;
enum PendKind {
    Tag(u32),
    Es(String),
}

#[pymethods]
impl RecDecoder {
    /// classes: list of dicts as produced by fmt_parser.parse; eid_type = rvt_decode.EID; desync_type = rvt_decode.Desync
    #[new]
    fn new(classes: &Bound<'_, PyList>, idsize: usize, stop: Vec<String>, strcap: usize, eid_type: Py<PyAny>, desync_type: Py<PyAny>) -> PyResult<Self> {
        let mut cl = Vec::new();
        for c in classes.iter() {
            let c = c.downcast::<PyDict>()?;
            let name: String = c.get_item("name")?.unwrap().extract()?;
            let parent: u32 = c.get_item("parent")?.unwrap().extract()?;
            let mut fields = Vec::new();
            for f in c.get_item("fields")?.unwrap().downcast::<PyList>()?.iter() {
                let f = f.downcast::<PyDict>()?;
                let fname: String = f.get_item("name")?.unwrap().extract()?;
                let types: Vec<u32> = f.get_item("types")?.unwrap().extract()?;
                let ns: Vec<Option<u32>> = f.get_item("ns")?.map(|x| x.extract()).transpose()?.unwrap_or_else(|| vec![None; types.len()]);
                let reff: Option<u32> = f.get_item("ref")?.map(|x| x.extract()).transpose()?;
                let ref2: Option<u32> = f.get_item("ref2")?.map(|x| x.extract()).transpose()?;
                let t0 = types[0];
                fields.push(Field { name: fname, t0, types, ns, reff, ref2 });
            }
            cl.push(Class { name, parent, fields });
        }
        Ok(RecDecoder { classes: cl, idsize, stop, strcap, es: HashMap::new(), eid_type, desync_type, chain_cache: HashMap::new(), defined: HashSet::new(), visited: vec![], err: None, path: vec![] })
    }

    /// es_schemas: {guid_hex: {'m_fields': [{'m_entryIndex','m_containerType','m_fieldTypeName','m_fieldName'}]}}
    fn set_es(&mut self, es: &Bound<'_, PyDict>) -> PyResult<()> {
        self.es.clear();
        for (k, v) in es.iter() {
            let g: String = k.extract()?;
            let mut fs = Vec::new();
            for f in v.downcast::<PyDict>()?.get_item("m_fields")?.unwrap().downcast::<PyList>()?.iter() {
                let f = f.downcast::<PyDict>()?;
                fs.push(EsField {
                    name: f.get_item("m_fieldName")?.unwrap().extract()?,
                    entry: f.get_item("m_entryIndex")?.unwrap().extract()?,
                    container: f.get_item("m_containerType")?.unwrap().extract()?,
                    tname: f.get_item("m_fieldTypeName")?.unwrap().extract()?,
                });
            }
            fs.sort_by_key(|f| f.entry);
            self.es.insert(g, fs);
        }
        Ok(())
    }

    fn set_strcap(&mut self, n: usize) { self.strcap = n; }

    /// header [u32 A][id][u64 stamp][u16 tag] (short: u32 stamp) -> (record dict, end)
    fn record<'py>(&mut self, py: Python<'py>, b: &Bound<'py, PyBytes>, p: usize, short: bool) -> PyResult<(Bound<'py, PyDict>, usize)> {
        let buf = b.as_bytes();
        let w = self.idsize;
        let sl = if short { 4 } else { 8 };
        if p + 4 + w + sl + 2 > buf.len() {
            return Err(self.other_err("unpack_from requires a buffer of at least"));
        }
        let eid: u64 = if w == 4 { u32::from_le_bytes(buf[p + 4..p + 8].try_into().unwrap()) as u64 } else { u64::from_le_bytes(buf[p + 4..p + 12].try_into().unwrap()) };
        let tag = u16::from_le_bytes(buf[p + 4 + w + sl..p + 6 + w + sl].try_into().unwrap()) as u32;
        self.err = None;
        self.defined.clear();
        self.defined.insert(1);
        self.defined.insert(2);
        self.visited.clear();
        let body0 = p + 4 + w + sl + 2;
        self.path.clear();
        let cname = self.cname(tag);
        self.path.push(cname.clone());
        let part = PyDict::new_bound(py);
        let res = self.body(py, buf, body0, tag, &part);
        let rec = PyDict::new_bound(py);
        match res {
            Ok((obj, q, complete)) => {
                rec.set_item("id", eid)?;
                rec.set_item("tag", tag)?;
                rec.set_item("class", cname)?;
                rec.set_item("fields", obj)?;
                rec.set_item("complete", complete)?;
                match &self.err { Some(e) => rec.set_item("error", e)?, None => rec.set_item("error", py.None())? }
                rec.set_item("bodies", PyList::new_bound(py, self.visited.iter()))?;
                rec.set_item("short", short)?;
                Ok((rec, q))
            }
            Err(Err::Desync(m)) => {
                if part.len() == 0 || !part.contains("m_id")? { return Err(self.desync_err(py, &m)); }
                let m200: String = m.chars().take(200).collect();
                rec.set_item("id", eid)?;
                rec.set_item("tag", tag)?;
                rec.set_item("class", cname)?;
                rec.set_item("fields", part)?;
                rec.set_item("complete", false)?;
                rec.set_item("error", m200)?;
                rec.set_item("short", short)?;
                Ok((rec, p))
            }
            Err(Err::Other(m)) => Err(self.other_err(&m)),
        }
    }
}

impl RecDecoder {
    fn other_err(&self, m: &str) -> PyErr { PyValueError::new_err(m.to_string()) }
    fn desync_err(&self, py: Python<'_>, m: &str) -> PyErr {
        match self.desync_type.bind(py).call1((m,)) { Ok(e) => PyErr::from_value_bound(e), Err(e) => e }
    }
    fn cname(&self, tag: u32) -> String {
        let i = tag as i64 - 12;
        if i >= 0 && (i as usize) < self.classes.len() { self.classes[i as usize].name.clone() } else { format!("#{}", tag) }
    }
    fn chain(&mut self, ci: usize) -> Vec<usize> {
        if let Some(c) = self.chain_cache.get(&ci) { return c.clone(); }
        let mut out = vec![];
        let mut i = ci;
        loop {
            out.push(i);
            let pt = (self.classes[i].parent & 0x7fff) as i64;
            if pt == 0 { break; }
            i = (pt - 12) as usize;
        }
        out.reverse();
        self.chain_cache.insert(ci, out.clone());
        out
    }

    fn u32at(&self, b: &[u8], p: usize) -> R<u32> {
        if p + 4 > b.len() { return Err(Err::Other("unpack_from requires a buffer of at least 4 bytes".into())); }
        Ok(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()))
    }

    fn prim<'py>(&self, py: Python<'py>, b: &[u8], p: usize, kind: u32) -> R<(PyObject, usize)> {
        if kind == 9 {
            let e = (p + 16).min(b.len());
            let s = if p <= b.len() { hex(&b[p..e]) } else { String::new() };
            return Ok((s.into_py(py), p + 16));
        }
        if kind == 8 {
            let n = self.u32at(b, p)? as usize;
            if n > self.strcap || p + 4 + 2 * n > b.len() { return Err(Err::Desync(format!("string length {} at {}", n, p))); }
            let u: Vec<u16> = b[p + 4..p + 4 + 2 * n].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            return Ok((String::from_utf16_lossy(&u).into_py(py), p + 4 + 2 * n));
        }
        let sz = match kind { 1 | 2 => 1, 3 | 0x0a => 2, 4 | 5 | 6 => 4, 7 | 0x0b => 8, _ => return Err(Err::Desync(format!("unknown kind {:#x}", kind))) };
        if p + sz > b.len() { return Err(Err::Desync("past end".into())); }
        let o: PyObject = match kind {
            1 | 2 => b[p].into_py(py),
            3 => i16::from_le_bytes(b[p..p + 2].try_into().unwrap()).into_py(py),
            0x0a => u16::from_le_bytes(b[p..p + 2].try_into().unwrap()).into_py(py),
            4 | 5 => u32::from_le_bytes(b[p..p + 4].try_into().unwrap()).into_py(py),
            6 => (f32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as f64).into_py(py),
            7 => f64::from_le_bytes(b[p..p + 8].try_into().unwrap()).into_py(py),
            _ => u64::from_le_bytes(b[p..p + 8].try_into().unwrap()).into_py(py),
        };
        Ok((o, p + sz))
    }

    fn value<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, f: &Field, pend: &mut Pend<'py>, lvl: usize) -> R<(PyObject, usize)> {
        let t = f.types[lvl];
        let kind = t & 0xff;
        let fl = (t >> 8) & 0xff;
        let n = if lvl < f.ns.len() { f.ns[lvl] } else { None };
        let last = lvl == f.types.len() - 1;
        let reff = if (lvl == 0 || last) && f.reff.is_some() { f.reff } else { f.ref2 };
        let mut p = p;
        if kind == 0x0d {
            let cnt: usize;
            if fl & 0x10 != 0 && fl & 0x40 == 0 {
                cnt = n.unwrap_or(0) as usize;
            } else {
                cnt = self.u32at(b, p)? as usize;
                p += 4;
                if cnt > 1_000_000 { return Err(Err::Desync(format!("array count {}", cnt))); }
            }
            let out = PyList::empty_bound(py);
            for _ in 0..cnt {
                let (v, np) = self.value(py, b, p, f, pend, lvl + 1)?;
                p = np;
                out.append(v).map_err(|e| Err::Other(e.to_string()))?;
            }
            return Ok((out.into_py(py), p));
        }
        if fl & 0x10 != 0 && fl & 0x40 == 0 {
            let out = PyList::empty_bound(py);
            for _ in 0..n.unwrap_or(0) {
                let (v, np) = self.elem(py, b, p, kind, fl & !0x10, reff, pend)?;
                p = np;
                out.append(v).map_err(|e| Err::Other(e.to_string()))?;
            }
            return Ok((out.into_py(py), p));
        }
        self.elem(py, b, p, kind, fl, reff, pend)
    }

    fn elem<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, kind: u32, fl: u32, reff: Option<u32>, pend: &mut Pend<'py>) -> R<(PyObject, usize)> {
        let mut p = p;
        let pe = |e: PyErr| Err::Other(e.to_string());
        if kind == 0x0e {
            if (fl & 0xf0) == 0x50 && (fl & 0x0f) != 0 {
                let n = self.u32at(b, p)?;
                p += 4;
                if n == 0xffff_ffff { return Ok((py.None(), p)); }
                if n > 1_000_000 { return Err(Err::Desync(format!("set count {} at {} ({})", n, p - 4, self.path.join(".")))); }
                let stars = if matches!(fl & 0x0f, 1 | 2 | 3) { fl & 0x0f } else { 1 };
                let out = PyList::empty_bound(py);
                for _ in 0..n {
                    let (v, np) = self.slot(py, b, p, stars, pend)?;
                    p = np;
                    out.append(v).map_err(pe)?;
                }
                return Ok((out.into_py(py), p));
            }
            if matches!(fl, 1 | 2 | 3) { return self.slot(py, b, p, fl, pend); }
            let rf = (reff.unwrap_or(0) & 0x7fff) as i64;
            if fl == 0x50 {
                let n = self.u32at(b, p)?;
                p += 4;
                if n == 0xffff_ffff { return Ok((py.None(), p)); }
                if n > 1_000_000 { return Err(Err::Desync(format!("vector count {} at {} ({})", n, p - 4, self.path.join(".")))); }
                let out = PyList::empty_bound(py);
                for _ in 0..n {
                    let (v, np) = self.struct_(py, b, p, rf - 12, pend, None)?;
                    p = np;
                    out.append(v).map_err(pe)?;
                }
                return Ok((out.into_py(py), p));
            }
            if fl == 0 { return self.struct_(py, b, p, rf - 12, pend, None); }
            return Err(Err::Desync(format!("flags {:#x} kind 0e", fl)));
        }
        if fl == 0x50 || fl == 0x51 {
            let n = self.u32at(b, p)? as usize;
            p += 4;
            if kind == 1 || kind == 2 {
                if p + n > b.len() { return Err(Err::Desync(format!("byte vector count {} past end", n))); }
                return Ok((PyBytes::new_bound(py, &b[p..p + n]).into_py(py), p + n));
            }
            if n > 1_000_000 { return Err(Err::Desync(format!("prim vector count {}", n))); }
            let out = PyList::empty_bound(py);
            for _ in 0..n {
                let (v, np) = self.prim(py, b, p, kind)?;
                p = np;
                out.append(v).map_err(pe)?;
            }
            return Ok((out.into_py(py), p));
        }
        self.prim(py, b, p, kind)
    }

    fn es_slot<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, pend: &mut Pend<'py>) -> R<(PyObject, usize)> {
        let v = self.u32at(b, p)?;
        if v == 0 { return Ok((py.None(), p + 4)); }
        let e = (p + 20).min(b.len());
        let g = hex(&b[(p + 4).min(b.len())..e]);
        let slot = PyDict::new_bound(py);
        slot.set_item("handle", v).map_err(|e| Err::Other(e.to_string()))?;
        slot.set_item("es", &g).map_err(|e| Err::Other(e.to_string()))?;
        pend.push((slot.clone(), PendKind::Es(g)));
        Ok((slot.into_py(py), p + 20))
    }

    fn es_value<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, tn: &str, pend: &mut Pend<'py>) -> R<(PyObject, usize)> {
        let ck = |n: usize| -> R<()> { if p + n > b.len() { Err(Err::Other("unpack_from requires a buffer".into())) } else { Ok(()) } };
        match tn {
            "TCHAR" | "AString" => self.prim(py, b, p, 8),
            "int" => { ck(4)?; Ok((i32::from_le_bytes(b[p..p + 4].try_into().unwrap()).into_py(py), p + 4)) }
            "short" => { ck(2)?; Ok((i16::from_le_bytes(b[p..p + 2].try_into().unwrap()).into_py(py), p + 2)) }
            "char" | "bool" => { ck(1)?; Ok((b[p].into_py(py), p + 1)) }
            "double" => { ck(8)?; Ok((f64::from_le_bytes(b[p..p + 8].try_into().unwrap()).into_py(py), p + 8)) }
            "float" => { ck(4)?; Ok(((f32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as f64).into_py(py), p + 4)) }
            "GUIDvalue" => { let e = (p + 16).min(b.len()); Ok((hex(&b[p.min(b.len())..e]).into_py(py), p + 16)) }
            "ElementId" => {
                let w = self.idsize;
                ck(w)?;
                let v: u64 = if w == 4 { u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as u64 } else { u64::from_le_bytes(b[p..p + 8].try_into().unwrap()) };
                let o = self.eid_type.bind(py).call1((v,)).map_err(|e| Err::Other(e.to_string()))?;
                Ok((o.into_py(py), p + w))
            }
            "XYZ" => { ck(24)?; let l = PyList::new_bound(py, (0..3).map(|i| f64::from_le_bytes(b[p + 8 * i..p + 8 * i + 8].try_into().unwrap()))); Ok((l.into_py(py), p + 24)) }
            "UV" => { ck(16)?; let l = PyList::new_bound(py, (0..2).map(|i| f64::from_le_bytes(b[p + 8 * i..p + 8 * i + 8].try_into().unwrap()))); Ok((l.into_py(py), p + 16)) }
            "ESEntity" => self.es_slot(py, b, p, pend),
            _ => Err(Err::Desync(format!("ES type {}", tn))),
        }
    }

    fn es_body<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, guid: &str, pend: &mut Pend<'py>) -> R<(PyObject, usize)> {
        let fields = match self.es.get(guid) { Some(f) => f.clone(), None => return Err(Err::Desync(format!("unknown ES schema {}", guid))) };
        let out = PyDict::new_bound(py);
        let mut p = p;
        let pe = |e: PyErr| Err::Other(e.to_string());
        for f in fields.iter() {
            match f.container {
                0 => { let (v, np) = self.es_value(py, b, p, &f.tname, pend)?; p = np; out.set_item(&f.name, v).map_err(pe)?; }
                1 => {
                    let n = self.u32at(b, p)? as usize; p += 4;
                    if n > 1_000_000 { return Err(Err::Desync("ES array count".into())); }
                    let arr = PyList::empty_bound(py);
                    for _ in 0..n { let (v, np) = self.es_value(py, b, p, &f.tname, pend)?; p = np; arr.append(v).map_err(pe)?; }
                    out.set_item(&f.name, arr).map_err(pe)?;
                }
                2 => {
                    // std::pair< K , V >
                    let tn = &f.tname;
                    let (kt, vt) = match parse_pair(tn) { Some(x) => x, None => return Err(Err::Desync(format!("ES map type {}", tn))) };
                    let n = self.u32at(b, p)? as usize; p += 4;
                    if n > 1_000_000 { return Err(Err::Desync("ES map count".into())); }
                    let mp = PyList::empty_bound(py);
                    for _ in 0..n {
                        let (k, np) = self.es_value(py, b, p, &kt, pend)?; p = np;
                        let (v, np) = self.es_value(py, b, p, &vt, pend)?; p = np;
                        mp.append(PyTuple::new_bound(py, [k, v])).map_err(pe)?;
                    }
                    out.set_item(&f.name, mp).map_err(pe)?;
                }
                _ => {}
            }
        }
        Ok((out.into_py(py), p))
    }

    fn slot<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, stars: u32, pend: &mut Pend<'py>) -> R<(PyObject, usize)> {
        let v = self.u32at(b, p)?;
        if v == 0 { return Ok((py.None(), p + 4)); }
        if v != 0xffff_ffff {
            if self.defined.contains(&v) || stars == 3 {
                let d = PyDict::new_bound(py);
                d.set_item("ref", v).map_err(|e| Err::Other(e.to_string()))?;
                return Ok((d.into_py(py), p + 4));
            }
            self.defined.insert(v);
        }
        if p + 6 > b.len() { return Err(Err::Other("unpack_from requires a buffer of at least 2 bytes".into())); }
        let tag = u16::from_le_bytes(b[p + 4..p + 6].try_into().unwrap()) as u32;
        let in_range = tag >= 12 && (tag as usize) < 12 + self.classes.len();
        if v == 0xffff_ffff && !in_range { return Ok((py.None(), p + 4)); }
        if !in_range { return Err(Err::Desync(format!("pointer tag {} at {} ({})", tag, p, self.path.join(".")))); }
        let slot = PyDict::new_bound(py);
        slot.set_item("handle", v).map_err(|e| Err::Other(e.to_string()))?;
        slot.set_item("ptr", tag).map_err(|e| Err::Other(e.to_string()))?;
        pend.push((slot.clone(), PendKind::Tag(tag)));
        Ok((slot.into_py(py), p + 6))
    }

    fn struct_<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, ci: i64, pend: &mut Pend<'py>, top_out: Option<&Bound<'py, PyDict>>) -> R<(PyObject, usize)> {
        if ci < 0 || ci as usize >= self.classes.len() { return Err(Err::Desync(format!("bad class index {}", ci))); }
        let ci = ci as usize;
        let out: Bound<'py, PyDict> = match top_out { Some(d) => d.clone(), None => PyDict::new_bound(py) };
        let chain = self.chain(ci);
        let mut p = p;
        for k in chain {
            let nf = self.classes[k].fields.len();
            for fi in 0..nf {
                let f = self.classes[k].fields[fi].clone();
                let cname = self.classes[k].name.clone();
                let r: R<(PyObject, usize)> = if cname == "ESEntity" && f.name == "m_blob" {
                    self.es_slot(py, b, p, pend)
                } else {
                    self.path.push(cname.clone());
                    self.path.push(f.name.clone());
                    let r = self.value(py, b, p, &f, pend, 0);
                    self.path.pop();
                    self.path.pop();
                    r
                };
                match r {
                    Ok((v, np)) => { out.set_item(&f.name, v).map_err(|e| Err::Other(e.to_string()))?; p = np; }
                    Err(Err::Desync(m)) => {
                        if !m.contains("@@") {
                            let e = (p + 24).min(b.len());
                            let tail = if p <= b.len() { hex_sp(&b[p..e]) } else { String::new() };
                            return Err(Err::Desync(format!("{} @@ {}.{} T={:#x} at {}: {}", m, cname, f.name, f.t0, p, tail)));
                        }
                        return Err(Err::Desync(m));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        let nm = &self.classes[ci].name;
        if nm == "ElementId" && out.len() == 1 {
            let mut v: Bound<'py, PyAny> = out.values().get_item(0).map_err(|e| Err::Other(e.to_string()))?;
            if let Ok(d) = v.clone().downcast::<PyDict>() {
                if d.len() == 1 { v = d.values().get_item(0).map_err(|e| Err::Other(e.to_string()))?; }
            }
            if v.is_instance_of::<pyo3::types::PyInt>() {
                let o = self.eid_type.bind(py).call1((v,)).map_err(|e| Err::Other(e.to_string()))?;
                return Ok((o.into_py(py), p));
            }
            return Ok((v.into_py(py), p));
        }
        Ok((out.into_py(py), p))
    }

    fn body<'py>(&mut self, py: Python<'py>, b: &[u8], p: usize, tag: u32, part: &Bound<'py, PyDict>) -> R<(PyObject, usize, bool)> {
        let mut pend: Pend<'py> = vec![];
        self.visited.push(self.cname(tag));
        let (v, mut p) = self.struct_(py, b, p, tag as i64 - 12, &mut pend, Some(part))?;
        let mut q: VecDeque<(Bound<'py, PyDict>, PendKind)> = pend.into_iter().collect();
        let top_path = self.path.clone();
        while let Some((slot, kind)) = q.pop_front() {
            let mut npend: Pend<'py> = vec![];
            match kind {
                PendKind::Es(g) => {
                    self.path = top_path.clone();
                    self.path.push("*ES".into());
                    match self.es_body(py, b, p, &g, &mut npend) {
                        Ok((obj, np)) => { slot.set_item("obj", obj).map_err(|e| Err::Other(e.to_string()))?; p = np; }
                        Err(Err::Desync(m)) => { slot.set_item("obj", py.None()).ok(); self.err = Some(m); self.path = top_path; return Ok((v, p, false)); }
                        Err(Err::Other(m)) => { slot.set_item("obj", py.None()).ok(); self.err = Some(m); self.path = top_path; return Ok((v, p, false)); }
                    }
                }
                PendKind::Tag(t) => {
                    let name = self.cname(t);
                    if self.stop.iter().any(|s| *s == name) {
                        slot.set_item("obj", "<geometry not decoded>").map_err(|e| Err::Other(e.to_string()))?;
                        self.path = top_path;
                        return Ok((v, p, false));
                    }
                    self.visited.push(name.clone());
                    self.path = top_path.clone();
                    self.path.push(format!("*{}", name));
                    match self.struct_(py, b, p, t as i64 - 12, &mut npend, None) {
                        Ok((obj, np)) => { slot.set_item("obj", obj).map_err(|e| Err::Other(e.to_string()))?; p = np; }
                        Err(Err::Desync(m)) => { slot.set_item("obj", py.None()).ok(); self.err = Some(m); self.path = top_path; return Ok((v, p, false)); }
                        Err(Err::Other(m)) => return Err(Err::Other(m)),
                    }
                }
            }
            q.extend(npend);
        }
        self.path = top_path;
        Ok((v, p, true))
    }
}

fn parse_pair(tn: &str) -> Option<(String, String)> {
    // matches r'std::pair<\s*(\w+)\s*,\s*(\w+)\s*>' at the start of the string
    let rest = tn.strip_prefix("std::pair<")?;
    let rest = rest.trim_start();
    let k: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if k.is_empty() { return None; }
    let rest = rest[k.len()..].trim_start();
    let rest = rest.strip_prefix(',')?.trim_start();
    let v: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if v.is_empty() { return None; }
    let rest = rest[v.len()..].trim_start();
    if !rest.starts_with('>') { return None; }
    Some((k, v))
}
