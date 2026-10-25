//! Native Formats/Latest schema parser: exact mirror of fmt_parser._parse_raw (class records, field types, optional GUID tables).
//! Returns None when no layout fits; the Python caller then runs the reference parser to produce the error.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

struct Bad;
type R<T> = Result<T, Bad>;

struct P<'a> {
    b: &'a [u8],
    p: usize,
    guid: bool,
    classes: Vec<Class>,
}

#[derive(Default)]
struct Field {
    name: String,
    types: Vec<u32>,
    ns: Vec<Option<u32>>,
    n: Option<u32>,
    reff: Option<u16>,
    def: Option<String>,
    k: Option<u32>,
    mark: Option<u8>,
    ref2: Option<u16>,
}

#[derive(Default)]
struct Class {
    name: String,
    off: usize,
    parent: u16,
    x: u16,
    parent_def: Option<String>,
    y: u16,
    count: u32,
    fields: Vec<Field>,
    guids: Option<Vec<String>>,
    m: Option<u16>,
}

fn ident_ok(s: &[u8]) -> bool {
    if s.is_empty() || !(s[0].is_ascii_alphabetic() || s[0] == b'_') {
        return false;
    }
    s[1..].iter().all(|&c| c.is_ascii_alphanumeric() || b"_:<>, *&".contains(&c))
}

impl<'a> P<'a> {
    fn u16(&mut self) -> R<u16> {
        if self.p + 2 > self.b.len() {
            return Err(Bad);
        }
        let v = u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]]);
        self.p += 2;
        Ok(v)
    }
    fn u32(&mut self) -> R<u32> {
        if self.p + 4 > self.b.len() {
            return Err(Bad);
        }
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        Ok(v)
    }
    fn name(&mut self, w: usize) -> R<String> {
        let l = if w == 2 { self.u16()? as usize } else { self.u32()? as usize };
        if !(1..=120).contains(&l) {
            return Err(Bad);
        }
        let end = (self.p + l).min(self.b.len());
        let sl = &self.b[self.p.min(self.b.len())..end];
        if !ident_ok(sl) {
            return Err(Bad);
        }
        let n = String::from_utf8_lossy(sl).into_owned();
        self.p += l;
        Ok(n)
    }
    fn record(&mut self, field_ctx: bool) -> R<usize> {
        let start = self.p;
        let name = self.name(2)?;
        let idx = self.classes.len();
        self.classes.push(Class { name, off: start, ..Default::default() });
        let parent = self.u16()?;
        let x = self.u16()?;
        self.classes[idx].parent = parent;
        self.classes[idx].x = x;
        if parent & 0x8000 != 0 {
            let pi = self.record(false)?;
            self.classes[idx].parent_def = Some(self.classes[pi].name.clone());
        }
        let y = self.u16()?;
        let cnt = self.u32()?;
        self.classes[idx].y = y;
        self.classes[idx].count = cnt;
        for _ in 0..cnt {
            let f = self.field()?;
            self.classes[idx].fields.push(f);
        }
        if self.guid {
            let n = self.u32()? as usize;
            if n > 60000 {
                return Err(Bad);
            }
            let mut g = Vec::with_capacity(n);
            for i in 0..n {
                let a = self.p + 16 * i;
                let e = (a + 16).min(self.b.len());
                let a = a.min(self.b.len());
                g.push(self.b[a..e].iter().map(|x| format!("{:02x}", x)).collect::<String>());
            }
            self.p += 16 * n;
            if self.p > self.b.len() {
                return Err(Bad);
            }
            self.classes[idx].guids = Some(g);
        }
        if !field_ctx {
            let m = self.u16()?;
            self.classes[idx].m = Some(m);
        }
        Ok(idx)
    }
    fn ftype(&mut self, f: &mut Field) -> R<()> {
        let t = self.u32()?;
        let kind = t & 0xff;
        let flags = (t >> 8) & 0xff;
        f.types.push(t);
        if flags & 0x10 != 0 && flags & 0x40 == 0 {
            let n = self.u32()?;
            f.n = Some(n);
            f.ns.push(Some(n));
        } else {
            f.ns.push(None);
        }
        if kind == 0x0e && (flags & 0x0f) == 0 {
            let r = self.u16()?;
            f.reff = Some(r);
            if r & 0x8000 != 0 {
                self.u16()?;
                let i = self.record(true)?;
                f.def = Some(self.classes[i].name.clone());
            }
        } else if kind == 0x0d {
            f.k = Some(self.u32()?);
            if self.p >= self.b.len() {
                return Err(Bad);
            }
            f.mark = Some(self.b[self.p]);
            self.p += 1;
            self.ftype(f)?;
            if f.types.last().unwrap() & 0xff == 0x0e {
                f.ref2 = Some(self.u16()?);
            }
        }
        Ok(())
    }
    fn field(&mut self) -> R<Field> {
        let name = self.name(4)?;
        let mut f = Field { name, ..Default::default() };
        self.ftype(&mut f)?;
        Ok(f)
    }
}

fn run(b: &[u8], guid: bool) -> Option<Vec<Class>> {
    let mut p = P { b, p: 2, guid, classes: Vec::new() };
    while p.p < b.len() {
        if !b[p.p..].iter().any(|&x| x != 0) {
            break;
        }
        if p.record(false).is_err() {
            return None;
        }
    }
    Some(p.classes)
}

/// parse_formats(bytes) -> (classes, {'guid_tables': bool}) or None
#[pyfunction]
pub fn parse_formats<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Option<(Bound<'py, PyList>, Bound<'py, PyDict>)>> {
    for guid in [true, false] {
        if let Some(cl) = run(b, guid) {
            let out = PyList::empty_bound(py);
            for c in cl {
                let d = PyDict::new_bound(py);
                d.set_item("name", c.name)?;
                d.set_item("off", c.off)?;
                let fl = PyList::empty_bound(py);
                for f in c.fields {
                    let fd = PyDict::new_bound(py);
                    fd.set_item("name", f.name)?;
                    fd.set_item("types", f.types.clone())?;
                    fd.set_item("ns", f.ns)?;
                    if let Some(v) = f.n { fd.set_item("n", v)?; }
                    if let Some(v) = f.reff { fd.set_item("ref", v)?; }
                    if let Some(v) = f.def { fd.set_item("def", v)?; }
                    if let Some(v) = f.k { fd.set_item("k", v)?; }
                    if let Some(v) = f.mark { fd.set_item("mark", v)?; }
                    if let Some(v) = f.ref2 { fd.set_item("ref2", v)?; }
                    fd.set_item("T", f.types[0])?;
                    fl.append(fd)?;
                }
                d.set_item("fields", fl)?;
                d.set_item("parent", c.parent)?;
                d.set_item("X", c.x)?;
                if let Some(v) = c.parent_def { d.set_item("parent_def", v)?; }
                d.set_item("Y", c.y)?;
                d.set_item("count", c.count)?;
                if let Some(g) = c.guids { d.set_item("guids", g)?; }
                if let Some(m) = c.m { d.set_item("M", m)?; }
                out.append(d)?;
            }
            let v = PyDict::new_bound(py);
            v.set_item("guid_tables", guid)?;
            return Ok(Some((out, v)));
        }
    }
    Ok(None)
}
