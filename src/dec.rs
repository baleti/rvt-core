//! GIL-free schema-driven record decoder (port of ~/rvt-research/rvt_decode.Dec, default configuration: no hints, no GG/M experiments).
//! Decodes into a compact value tree (`Val`) that is converted to Python objects later (conv.rs) or serialised to the binary cache (cache.rs).
//! Every message text that can end up in the output (record 'error') reproduces the Python reference exactly.
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub const K_HANDLE: u32 = 0;
pub const K_PTR: u32 = 1;
pub const K_OBJ: u32 = 2;
pub const K_REF: u32 = 3;
pub const K_ES: u32 = 4;

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub key: u32,
    pub t0: u32,
    pub types: Vec<u32>,
    pub ns: Vec<Option<u32>>,
    pub reff: Option<u32>,
    pub has_ref: bool,
    pub ref2: Option<u32>,
    pub es_blob: bool,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub parent: u32,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug)]
pub struct EsField {
    pub key: u32,
    pub entry: i64,
    pub container: i64,
    pub tname: String,
    pub pair: Option<(String, String)>,
}

/// Immutable decoding context shared by all threads.
#[derive(Clone)]
pub struct Schema {
    pub classes: Vec<Class>,
    /// chain (base first) per class; Err = the Python reference would raise a non-Desync exception building it
    pub chains: Vec<Result<Vec<usize>, String>>,
    pub chain_dup: Vec<bool>,
    pub keys: Vec<String>,
    pub key_of: FxHashMap<String, u32>,
    pub idsize: usize,
    pub stop: Vec<bool>,
    pub strcap: usize,
    pub es: FxHashMap<String, Vec<EsField>>,
}

impl Schema {
    pub fn new(mut classes: Vec<Class>, idsize: usize, stop: &[String], strcap: usize) -> Schema {
        let mut keys: Vec<String> = ["handle", "ptr", "obj", "ref", "es"].iter().map(|s| s.to_string()).collect();
        let mut key_of: FxHashMap<String, u32> = FxHashMap::default();
        for (i, k) in keys.iter().enumerate() {
            key_of.insert(k.clone(), i as u32);
        }
        for c in classes.iter_mut() {
            let es_cls = c.name == "ESEntity";
            for f in c.fields.iter_mut() {
                let k = *key_of.entry(f.name.clone()).or_insert_with(|| {
                    keys.push(f.name.clone());
                    (keys.len() - 1) as u32
                });
                f.key = k;
                f.es_blob = es_cls && f.name == "m_blob";
            }
        }
        let n = classes.len() as i64;
        let mut chains = Vec::with_capacity(classes.len());
        let mut chain_dup = Vec::with_capacity(classes.len());
        for ci in 0..classes.len() {
            let mut out = vec![];
            let mut i = ci as i64;
            let mut res: Result<Vec<usize>, String> = Err(String::new());
            for _ in 0..100_000 {
                let ix = if i < 0 { i + n } else { i };
                if ix < 0 || ix >= n {
                    res = Err("list index out of range".into());
                    break;
                }
                out.push(ix as usize);
                let pt = (classes[ix as usize].parent & 0x7fff) as i64;
                if pt == 0 {
                    out.reverse();
                    res = Ok(std::mem::take(&mut out));
                    break;
                }
                i = pt - 12;
            }
            let dup = match &res {
                Ok(ch) => {
                    let mut seen = FxHashSet::default();
                    ch.iter().any(|&k| classes[k].fields.iter().any(|f| !seen.insert(f.key)))
                }
                Err(_) => false,
            };
            chains.push(if let Err(e) = &res { if e.is_empty() { Err("chain too deep".into()) } else { res } } else { res });
            chain_dup.push(dup);
        }
        let stop = classes.iter().map(|c| stop.iter().any(|s| *s == c.name)).collect();
        Schema { classes, chains, chain_dup, keys, key_of, idsize, stop, strcap, es: FxHashMap::default() }
    }

    pub fn key(&mut self, name: &str) -> u32 {
        if let Some(k) = self.key_of.get(name) {
            return *k;
        }
        self.keys.push(name.to_string());
        let k = (self.keys.len() - 1) as u32;
        self.key_of.insert(name.to_string(), k);
        k
    }

    pub fn cname(&self, tag: u32) -> String {
        let i = tag as i64 - 12;
        if i >= 0 && (i as usize) < self.classes.len() { self.classes[i as usize].name.clone() } else { format!("#{}", tag) }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    None,
    I(i64),
    U(u64),
    F(f64),
    S(String),
    B(Vec<u8>),
    L(Vec<Val>),
    T(Vec<Val>),
    D(Vec<(u32, Val)>),
    EI(i64),
    EU(u64),
    /// pointer slot dict {'handle','ptr'|'es'[, 'obj']}: index into Rec.slots
    Slot(u32),
    /// the string '<geometry not decoded>'
    Geo,
    /// undecoded UTF-16LE string (byte offset, chars): only produced with Dec.lazy_str (extensible-storage scan)
    R(usize, usize),
}

impl Val {
    pub fn as_int(&self) -> Option<i128> {
        match self {
            Val::I(v) | Val::EI(v) => Some(*v as i128),
            Val::U(v) | Val::EU(v) => Some(*v as i128),
            _ => None,
        }
    }
    pub fn get(&self, k: u32) -> Option<&Val> {
        if let Val::D(d) = self { d.iter().find(|(kk, _)| *kk == k).map(|(_, v)| v) } else { None }
    }
}

#[derive(Clone, Debug)]
pub enum SlotKind {
    Ptr(u32),
    Es(String),
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub handle: u32,
    pub kind: SlotKind,
    pub obj: Option<Val>,
}

#[derive(Clone, Debug)]
pub struct Rec {
    pub id: u64,
    pub tag: u32,
    pub fields: Val,
    pub complete: bool,
    pub error: Option<String>,
    /// None for the partial (desync) form, which has no 'bodies' key
    pub bodies: Option<Vec<u32>>,
    pub short: bool,
    pub slots: Vec<Slot>,
}

impl Rec {
    /// follow a slot to its object ('obj' value), like rvt_entities.S_ does for dict values with 'obj'
    pub fn slot_obj<'a>(&'a self, v: &'a Val) -> &'a Val {
        if let Val::Slot(s) = v {
            if let Some(o) = &self.slots[*s as usize].obj {
                return o;
            }
        }
        v
    }
}

pub enum DErr {
    /// rvt_decode.Desync
    Desync(String),
    /// any other Python exception (struct.error, IndexError, TypeError ...)
    Other(String),
}
type R<T> = Result<T, DErr>;

pub fn struct_err(size: usize, off: usize, len: usize) -> DErr {
    DErr::Other(format!("unpack_from requires a buffer of at least {} bytes for unpacking {} bytes at offset {} (actual buffer size is {})", off + size, size, off, len))
}

/// bytes.decode('utf-16le', 'replace'): ASCII fast path, general lossy decoding otherwise
pub fn utf16le(b: &[u8]) -> String {
    let mut v: Vec<u8> = Vec::with_capacity(b.len() / 2);
    let mut ascii = true;
    for c in b.chunks_exact(2) {
        if c[1] != 0 || c[0] >= 0x80 { ascii = false; break; }
        v.push(c[0]);
    }
    if ascii { return unsafe { String::from_utf8_unchecked(v) }; }
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}
fn hex_sp(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect::<Vec<_>>().join(" ")
}
fn slice(b: &[u8], a: usize, e: usize) -> &[u8] {
    let a = a.min(b.len());
    let e = e.min(b.len()).max(a);
    &b[a..e]
}

#[derive(Clone, Copy)]
enum PathEl {
    Top(u32),
    Fld(usize, usize),
    Star(u32),
    StarEs,
    Es,
}

#[derive(Clone, Copy)]
enum Pend {
    Tag(u32, u32),
    Es(u32),
}

/// per-record mutable state; one per thread
pub struct Dec<'s> {
    pub s: &'s Schema,
    defined: FxHashSet<u32>,
    visited: Vec<u32>,
    err: Option<String>,
    path: Vec<PathEl>,
    slots: Vec<Slot>,
    pub strcap: usize,
    /// skip building desync messages (callers that discard them; extract re-decodes accepted records with messages)
    pub quiet: bool,
    pub lazy_str: bool,
    /// extraction: stop decoding a record as soon as its top-level m_id is decoded and differs (the record would be rejected anyway)
    pub abort_mid: Option<u64>,
    /// diagnostics (RVT_CORE_SLOW): largest element count seen in one record and where
    pub big: (usize, String),
}

impl<'s> Dec<'s> {
    pub fn new(s: &'s Schema) -> Self {
        let mut defined = FxHashSet::default();
        defined.insert(1);
        defined.insert(2);
        Dec { s, defined, visited: vec![], err: None, path: vec![], slots: vec![], strcap: s.strcap, quiet: false, lazy_str: false, abort_mid: None, big: (0, String::new()) }
    }

    #[inline] fn note_big(&mut self, n: usize) {
        if n > 20_000 && n > self.big.0 && crate::extract::SLOW.load(std::sync::atomic::Ordering::Relaxed) { let ps = self.path_str(); self.big = (n, ps); }
    }

    fn path_str(&self) -> String {
        if self.quiet { return String::new(); }
        let mut parts: Vec<String> = vec![];
        for e in &self.path {
            match *e {
                PathEl::Top(t) => parts.push(self.s.cname(t)),
                PathEl::Fld(k, f) => {
                    parts.push(self.s.classes[k].name.clone());
                    parts.push(self.s.classes[k].fields[f].name.clone())
                }
                PathEl::Star(t) => parts.push(format!("*{}", self.s.cname(t))),
                PathEl::StarEs => parts.push("*ES".into()),
                PathEl::Es => parts.push("es".into()),
            }
        }
        parts.join(".")
    }

    #[inline]
    fn u32at(&self, b: &[u8], p: usize) -> R<u32> {
        if p + 4 > b.len() { return Err(struct_err(4, p, b.len())); }
        Ok(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()))
    }

    fn prim(&self, b: &[u8], p: usize, kind: u32) -> R<(Val, usize)> {
        if kind == 9 {
            return Ok((Val::S(hex(slice(b, p, p + 16))), p + 16));
        }
        if kind == 8 {
            let n = self.u32at(b, p)? as usize;
            if n > self.strcap || p + 4 + 2 * n > b.len() { return Err(DErr::Desync(format!("string length {} at {}", n, p))); }
            if self.lazy_str { return Ok((Val::R(p + 4, n), p + 4 + 2 * n)); }
            return Ok((Val::S(utf16le(&b[p + 4..p + 4 + 2 * n])), p + 4 + 2 * n));
        }
        let sz = match kind { 1 | 2 => 1, 3 | 0x0a => 2, 4 | 5 | 6 => 4, 7 | 0x0b => 8, _ => return Err(DErr::Desync(format!("unknown kind {:#x}", kind))) };
        if p + sz > b.len() { return Err(DErr::Desync("past end".into())); }
        let v = match kind {
            1 | 2 => Val::I(b[p] as i64),
            3 => Val::I(i16::from_le_bytes(b[p..p + 2].try_into().unwrap()) as i64),
            0x0a => Val::I(u16::from_le_bytes(b[p..p + 2].try_into().unwrap()) as i64),
            4 | 5 => Val::I(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as i64),
            6 => Val::F(f32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as f64),
            7 => Val::F(f64::from_le_bytes(b[p..p + 8].try_into().unwrap())),
            _ => Val::U(u64::from_le_bytes(b[p..p + 8].try_into().unwrap())),
        };
        Ok((v, p + sz))
    }

    fn value(&mut self, b: &[u8], p: usize, f: &Field, pend: &mut Vec<Pend>, lvl: usize) -> R<(Val, usize)> {
        if lvl >= f.types.len() { return Err(DErr::Other("list index out of range".into())); }
        let t = f.types[lvl];
        let kind = t & 0xff;
        let fl = (t >> 8) & 0xff;
        let n = if lvl < f.ns.len() { f.ns[lvl] } else { None };
        let last = lvl == f.types.len() - 1;
        let reff = if (lvl == 0 || last) && f.has_ref { f.reff } else { f.ref2 };
        let mut p = p;
        if kind == 0x0d {
            let cnt: usize;
            if fl & 0x10 != 0 && fl & 0x40 == 0 {
                cnt = match n { Some(n) => n as usize, None => return Err(DErr::Other("'NoneType' object cannot be interpreted as an integer".into())) };
            } else {
                cnt = self.u32at(b, p)? as usize;
                p += 4;
                if cnt > 1_000_000 { return Err(DErr::Desync(format!("array count {}", cnt))); }
                self.note_big(cnt);
            }
            let mut out = Vec::with_capacity(cnt.min(4096));
            for _ in 0..cnt {
                let (v, np) = self.value(b, p, f, pend, lvl + 1)?;
                p = np;
                out.push(v);
            }
            return Ok((Val::L(out), p));
        }
        if fl & 0x10 != 0 && fl & 0x40 == 0 {
            let n = match n { Some(n) => n, None => return Err(DErr::Other("'NoneType' object cannot be interpreted as an integer".into())) };
            let mut out = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let (v, np) = self.elem(b, p, kind, fl & !0x10, reff, pend)?;
                p = np;
                out.push(v);
            }
            return Ok((Val::L(out), p));
        }
        self.elem(b, p, kind, fl, reff, pend)
    }

    fn elem(&mut self, b: &[u8], p: usize, kind: u32, fl: u32, reff: Option<u32>, pend: &mut Vec<Pend>) -> R<(Val, usize)> {
        let mut p = p;
        if kind == 0x0e {
            if (fl & 0xf0) == 0x50 && (fl & 0x0f) != 0 {
                let n = self.u32at(b, p)?;
                p += 4;
                if n == 0xffff_ffff { return Ok((Val::None, p)); }
                if n > 1_000_000 { return Err(DErr::Desync(format!("set count {} at {} ({})", n, p - 4, self.path_str()))); }
                self.note_big(n as usize);
                let stars = if matches!(fl & 0x0f, 1 | 2 | 3) { fl & 0x0f } else { 1 };
                let mut out = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    let (v, np) = self.slot(b, p, stars, pend)?;
                    p = np;
                    out.push(v);
                }
                return Ok((Val::L(out), p));
            }
            if matches!(fl, 1 | 2 | 3) { return self.slot(b, p, fl, pend); }
            let rf = (reff.unwrap_or(0) & 0x7fff) as i64;
            if fl == 0x50 {
                let n = self.u32at(b, p)?;
                p += 4;
                if n == 0xffff_ffff { return Ok((Val::None, p)); }
                if n > 1_000_000 { return Err(DErr::Desync(format!("vector count {} at {} ({})", n, p - 4, self.path_str()))); }
                self.note_big(n as usize);
                let mut out = Vec::with_capacity((n as usize).min(4096));
                for _ in 0..n {
                    let (v, np) = self.struct_(b, p, rf - 12, pend, None)?;
                    p = np;
                    out.push(v);
                }
                return Ok((Val::L(out), p));
            }
            if fl == 0 { return self.struct_(b, p, rf - 12, pend, None); }
            return Err(DErr::Desync(format!("flags {:#x} kind 0e", fl)));
        }
        if fl == 0x50 || fl == 0x51 {
            let n = self.u32at(b, p)? as usize;
            p += 4;
            if kind == 1 || kind == 2 {
                if p + n > b.len() { return Err(DErr::Desync(format!("byte vector count {} past end", n))); }
                return Ok((Val::B(b[p..p + n].to_vec()), p + n));
            }
            if n > 1_000_000 { return Err(DErr::Desync(format!("prim vector count {}", n))); }
            self.note_big(n);
            let mut out = Vec::with_capacity(n.min(1 << 16));
            for _ in 0..n {
                let (v, np) = self.prim(b, p, kind)?;
                p = np;
                out.push(v);
            }
            return Ok((Val::L(out), p));
        }
        self.prim(b, p, kind)
    }

    fn es_slot(&mut self, b: &[u8], p: usize, pend: &mut Vec<Pend>) -> R<(Val, usize)> {
        let v = self.u32at(b, p)?;
        if v == 0 { return Ok((Val::None, p + 4)); }
        let g = hex(slice(b, p + 4, p + 20));
        let si = self.slots.len() as u32;
        self.slots.push(Slot { handle: v, kind: SlotKind::Es(g), obj: None });
        pend.push(Pend::Es(si));
        Ok((Val::Slot(si), p + 20))
    }

    fn es_value(&mut self, b: &[u8], p: usize, tn: &str, pend: &mut Vec<Pend>) -> R<(Val, usize)> {
        let ck = |n: usize| -> R<()> { if p + n > b.len() { Err(struct_err(n, p, b.len())) } else { Ok(()) } };
        match tn {
            "TCHAR" | "AString" => self.prim(b, p, 8),
            "int" => { ck(4)?; Ok((Val::I(i32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as i64), p + 4)) }
            "short" => { ck(2)?; Ok((Val::I(i16::from_le_bytes(b[p..p + 2].try_into().unwrap()) as i64), p + 2)) }
            "char" | "bool" => { ck(1)?; Ok((Val::I(b[p] as i64), p + 1)) }
            "double" => { ck(8)?; Ok((Val::F(f64::from_le_bytes(b[p..p + 8].try_into().unwrap())), p + 8)) }
            "float" => { ck(4)?; Ok((Val::F(f32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as f64), p + 4)) }
            "GUIDvalue" => Ok((Val::S(hex(slice(b, p, p + 16))), p + 16)),
            "ElementId" => {
                let w = self.s.idsize;
                ck(w)?;
                let v = if w == 4 { Val::EI(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as i64) } else { Val::EU(u64::from_le_bytes(b[p..p + 8].try_into().unwrap())) };
                Ok((v, p + w))
            }
            "XYZ" => { ck(24)?; Ok((Val::L((0..3).map(|i| Val::F(f64::from_le_bytes(b[p + 8 * i..p + 8 * i + 8].try_into().unwrap()))).collect()), p + 24)) }
            "UV" => { ck(16)?; Ok((Val::L((0..2).map(|i| Val::F(f64::from_le_bytes(b[p + 8 * i..p + 8 * i + 8].try_into().unwrap()))).collect()), p + 16)) }
            "ESEntity" => self.es_slot(b, p, pend),
            _ => Err(DErr::Desync(format!("ES type {}", tn))),
        }
    }

    fn es_body(&mut self, b: &[u8], p: usize, guid: &str, pend: &mut Vec<Pend>) -> R<(Val, usize)> {
        let s = self.s;
        let fields = match s.es.get(guid) { Some(f) => f, None => return Err(DErr::Desync(format!("unknown ES schema {}", guid))) };
        let mut out: Vec<(u32, Val)> = Vec::with_capacity(fields.len());
        let mut p = p;
        for f in fields.iter() {
            let v = match f.container {
                0 => { let (v, np) = self.es_value(b, p, &f.tname, pend)?; p = np; v }
                1 => {
                    let n = self.u32at(b, p)? as usize;
                    p += 4;
                    if n > 1_000_000 { return Err(DErr::Desync("ES array count".into())); }
                    let mut arr = Vec::with_capacity(n.min(4096));
                    for _ in 0..n { let (v, np) = self.es_value(b, p, &f.tname, pend)?; p = np; arr.push(v); }
                    Val::L(arr)
                }
                2 => {
                    let (kt, vt) = match &f.pair { Some(x) => x, None => return Err(DErr::Desync(format!("ES map type {}", f.tname))) };
                    let n = self.u32at(b, p)? as usize;
                    p += 4;
                    if n > 1_000_000 { return Err(DErr::Desync("ES map count".into())); }
                    let mut mp = Vec::with_capacity(n.min(4096));
                    for _ in 0..n {
                        let (k, np) = self.es_value(b, p, kt, pend)?; p = np;
                        let (v, np) = self.es_value(b, p, vt, pend)?; p = np;
                        mp.push(Val::T(vec![k, v]));
                    }
                    Val::L(mp)
                }
                _ => continue,
            };
            dict_set(&mut out, f.key, v, true);
        }
        Ok((Val::D(out), p))
    }

    fn slot(&mut self, b: &[u8], p: usize, stars: u32, pend: &mut Vec<Pend>) -> R<(Val, usize)> {
        let v = self.u32at(b, p)?;
        if v == 0 { return Ok((Val::None, p + 4)); }
        if v != 0xffff_ffff {
            if self.defined.contains(&v) || stars == 3 {
                return Ok((Val::D(vec![(K_REF, Val::I(v as i64))]), p + 4));
            }
            self.defined.insert(v);
        }
        if p + 6 > b.len() { return Err(struct_err(2, p + 4, b.len())); }
        let tag = u16::from_le_bytes(b[p + 4..p + 6].try_into().unwrap()) as u32;
        let in_range = tag >= 12 && (tag as usize) < 12 + self.s.classes.len();
        if v == 0xffff_ffff && !in_range { return Ok((Val::None, p + 4)); }
        if !in_range { return Err(DErr::Desync(format!("pointer tag {} at {} ({})", tag, p, self.path_str()))); }
        let si = self.slots.len() as u32;
        self.slots.push(Slot { handle: v, kind: SlotKind::Ptr(tag), obj: None });
        pend.push(Pend::Tag(si, tag));
        Ok((Val::Slot(si), p + 6))
    }

    /// top: Some(out) = decode into the caller's dict (the top-level object, kept on desync as the partial record)
    fn struct_(&mut self, b: &[u8], p: usize, ci: i64, pend: &mut Vec<Pend>, top: Option<&mut Vec<(u32, Val)>>) -> R<(Val, usize)> {
        let s = self.s;
        if ci < 0 || ci as usize >= s.classes.len() { return Err(DErr::Desync(format!("bad class index {}", ci))); }
        let ci = ci as usize;
        let chain = match &s.chains[ci] { Ok(c) => c, Err(e) => return Err(DErr::Other(e.clone())) };
        let dup = s.chain_dup[ci];
        let mut local: Vec<(u32, Val)> = Vec::new();
        let is_top = top.is_some();
        let out: &mut Vec<(u32, Val)> = match top { Some(t) => t, None => &mut local };
        let mut p = p;
        for &k in chain.iter() {
            let cls = &s.classes[k];
            for (fi, f) in cls.fields.iter().enumerate() {
                let r = if f.es_blob {
                    self.es_slot(b, p, pend)
                } else {
                    self.path.push(PathEl::Fld(k, fi));
                    let r = self.value(b, p, f, pend, 0);
                    self.path.pop();
                    r
                };
                match r {
                    Ok((v, np)) => {
                        if is_top && !dup {
                            if let Some(want) = self.abort_mid {
                                if Some(f.key) == s.key_of.get("m_id").copied() && v.as_int() != Some(want as i128) && !matches!(v, Val::F(_)) {
                                    return Err(DErr::Other("m_id mismatch".into()));
                                }
                            }
                        }
                        dict_set(out, f.key, v, dup); p = np;
                    }
                    Err(DErr::Desync(m)) => {
                        if !self.quiet && !m.contains("@@") {
                            let tail = hex_sp(slice(b, p, p + 24));
                            return Err(DErr::Desync(format!("{} @@ {}.{} T={:#x} at {}: {}", m, cls.name, f.name, f.t0, p, tail)));
                        }
                        return Err(DErr::Desync(m));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        if is_top {
            return Ok((Val::None, p));
        }
        if s.classes[ci].name == "ElementId" && local.len() == 1 {
            let mut v = local.pop().unwrap().1;
            if let Val::D(mut d) = v {
                if d.len() == 1 { v = d.pop().unwrap().1; } else { v = Val::D(d); }
            }
            let v = match v {
                Val::I(x) | Val::EI(x) => Val::EI(x),
                Val::U(x) | Val::EU(x) => Val::EU(x),
                o => o,
            };
            return Ok((v, p));
        }
        Ok((Val::D(local), p))
    }

    /// -> (fields, end, complete); on a top-level desync: Err((partial, message))
    fn body(&mut self, b: &[u8], p: usize, tag: u32) -> Result<(Vec<(u32, Val)>, usize, bool), (Option<Vec<(u32, Val)>>, DErr)> {
        let mut pend: Vec<Pend> = vec![];
        self.visited.push(tag);
        let mut part: Vec<(u32, Val)> = vec![];
        let mut p = match self.struct_(b, p, tag as i64 - 12, &mut pend, Some(&mut part)) {
            Ok((_, np)) => np,
            Err(DErr::Desync(m)) => return Err((Some(part), DErr::Desync(m))),
            Err(e) => return Err((None, e)),
        };
        let mut q: VecDeque<Pend> = pend.into_iter().collect();
        let top_len = self.path.len();
        while let Some(it) = q.pop_front() {
            let mut npend: Vec<Pend> = vec![];
            match it {
                Pend::Es(si) => {
                    let g = match &self.slots[si as usize].kind { SlotKind::Es(g) => g.clone(), _ => unreachable!() };
                    self.path.truncate(top_len);
                    self.path.push(PathEl::StarEs);
                    match self.es_body(b, p, &g, &mut npend) {
                        Ok((obj, np)) => { self.slots[si as usize].obj = Some(obj); p = np; }
                        Err(DErr::Desync(m)) | Err(DErr::Other(m)) => {
                            self.slots[si as usize].obj = Some(Val::None);
                            self.err = Some(m);
                            self.path.truncate(top_len);
                            return Ok((part, p, false));
                        }
                    }
                }
                Pend::Tag(si, t) => {
                    let ti = t as usize - 12;
                    if self.s.stop[ti] {
                        self.slots[si as usize].obj = Some(Val::Geo);
                        self.path.truncate(top_len);
                        return Ok((part, p, false));
                    }
                    self.visited.push(t);
                    self.path.truncate(top_len);
                    self.path.push(PathEl::Star(t));
                    match self.struct_(b, p, t as i64 - 12, &mut npend, None) {
                        Ok((obj, np)) => { self.slots[si as usize].obj = Some(obj); p = np; }
                        Err(DErr::Desync(m)) => {
                            self.slots[si as usize].obj = Some(Val::None);
                            self.err = Some(m);
                            self.path.truncate(top_len);
                            return Ok((part, p, false));
                        }
                        Err(e) => return Err((None, e)),
                    }
                }
            }
            q.extend(npend);
        }
        self.path.truncate(top_len);
        Ok((part, p, true))
    }

    /// header [u32 A][id][u64 stamp][u16 tag] (short: u32 stamp) -> (record, end). Err = the Python reference raises.
    pub fn record(&mut self, b: &[u8], p: usize, short: bool) -> R<(Rec, usize)> {
        let w = self.s.idsize;
        let sl = if short { 4 } else { 8 };
        if p + 4 + w > b.len() { return Err(struct_err(w, p + 4, b.len())); }
        if p + 4 + w + sl + 2 > b.len() { return Err(struct_err(2, p + 4 + w + sl, b.len())); }
        let eid: u64 = if w == 4 { u32::from_le_bytes(b[p + 4..p + 8].try_into().unwrap()) as u64 } else { u64::from_le_bytes(b[p + 4..p + 12].try_into().unwrap()) };
        let tag = u16::from_le_bytes(b[p + 4 + w + sl..p + 6 + w + sl].try_into().unwrap()) as u32;
        self.err = None;
        self.defined.clear();
        self.defined.insert(1);
        self.defined.insert(2);
        self.visited.clear();
        self.slots.clear();
        self.path.clear();
        self.path.push(PathEl::Top(tag));
        let body0 = p + 4 + w + sl + 2;
        match self.body(b, body0, tag) {
            Ok((obj, q, complete)) => Ok((
                Rec { id: eid, tag, fields: Val::D(obj), complete, error: self.err.take(), bodies: Some(std::mem::take(&mut self.visited)), short, slots: std::mem::take(&mut self.slots) },
                q,
            )),
            Err((Some(part), DErr::Desync(m))) => {
                if part.is_empty() || !part.iter().any(|(k, _)| self.s.keys[*k as usize] == "m_id") { return Err(DErr::Desync(m)); }
                let m200: String = m.chars().take(200).collect();
                Ok((Rec { id: eid, tag, fields: Val::D(part), complete: false, error: Some(m200), bodies: None, short, slots: std::mem::take(&mut self.slots) }, p))
            }
            Err((_, e)) => Err(e),
        }
    }

    /// ESSchema entry decoder for the extensible-storage scan (rvt_es.decode_entry): fields of class ci, `mid` pad bytes after m_documentation
    pub fn es_entry(&mut self, b: &[u8], p: usize, pre: usize, mid: usize, post: usize, ci: usize, doc_key: u32) -> R<(String, Vec<(u32, Val)>, usize)> {
        let key = hex(slice(b, p + pre, p + pre + 16));
        let mut q = p + pre + 16 + post;
        let mut out: Vec<(u32, Val)> = vec![];
        let mut pend = vec![];
        self.path.clear();
        self.path.push(PathEl::Es);
        self.slots.clear();
        let s = self.s;
        for f in s.classes[ci].fields.iter() {
            let (v, nq) = self.value(b, q, f, &mut pend, 0)?;
            dict_set(&mut out, f.key, v, true);
            q = nq;
            if f.key == doc_key { q += mid; }
        }
        Ok((key, out, q))
    }
    pub fn take_slots(&mut self) -> Vec<Slot> { std::mem::take(&mut self.slots) }
}

#[inline]
pub fn dict_set(d: &mut Vec<(u32, Val)>, k: u32, v: Val, check: bool) {
    if check {
        if let Some(e) = d.iter_mut().find(|(kk, _)| *kk == k) {
            e.1 = v;
            return;
        }
    }
    d.push((k, v));
}

pub fn parse_pair(tn: &str) -> Option<(String, String)> {
    // re.match(r'std::pair<\s*(\w+)\s*,\s*(\w+)\s*>', tn)
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

