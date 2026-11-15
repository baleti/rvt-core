//! Partition segments and channel-103 (saved graphics) records: port of g103_get.Part / build_index.
use crate::dec::{Dec, Rec, Schema};
use flate2::{Decompress, FlushDecompress, Status};
use rayon::prelude::*;
use std::sync::Arc;

pub const GZ4: [u8; 4] = [0x1f, 0x8b, 0x08, 0x00];

pub struct Member {
    #[allow(dead_code)]
    pub start: usize,
    pub ch: Option<u64>,
    pub fl: u32,
    pub out: Vec<u8>,
}

/// raw-inflate `data`; -> (output, bytes consumed, stream ended) or None on a zlib error
fn inflate(data: &[u8]) -> Option<(Vec<u8>, usize, bool)> {
    let mut d = Decompress::new(false);
    let mut out: Vec<u8> = Vec::with_capacity((data.len() * 4).min(1 << 18).max(1024));
    loop {
        let in0 = d.total_in() as usize;
        let out0 = out.len();
        if out.len() == out.capacity() { out.reserve(out.capacity().max(1 << 16)); }
        let st = d.decompress_vec(&data[in0..], &mut out, FlushDecompress::None).ok()?;
        let used = d.total_in() as usize;
        if st == Status::StreamEnd { out.shrink_to_fit(); return Some((out, used, true)); }
        if used == in0 && out.len() == out0 {
            // no progress at all: input exhausted (truncated stream)
            out.shrink_to_fit();
            return Some((out, used, false));
        }
    }
}

fn gap_meta(data: &[u8], last_end: usize, i: usize) -> (Option<u64>, u32) {
    let gap = &data[last_end.min(i)..i];
    let ch = if gap.len() >= 8 { Some(u64::from_le_bytes(gap[gap.len() - 8..].try_into().unwrap())) } else { None };
    let fl = if gap.len() >= 24 { u32::from_le_bytes(gap[gap.len() - 24..gap.len() - 20].try_into().unwrap()) } else { 4 };
    (ch, fl)
}

/// sequential replica of Part.__init__: find GZ, inflate to the end of the deflate stream, skip false hits
fn members_seq(data: &[u8]) -> Vec<Member> {
    let mut out = vec![];
    let mut pos = 0usize;
    let mut last_end = 0usize;
    while let Some(k) = memchr::memmem::find(&data[pos.min(data.len())..], &GZ4) {
        let i = pos + k;
        match inflate(&data[(i + 10).min(data.len())..]) {
            None => { pos = i + 4; continue; }
            Some((o, used, _)) => {
                let end = (i + 10 + used).min(data.len());
                let (ch, fl) = gap_meta(data, last_end, i);
                out.push(Member { start: i, ch, fl, out: o });
                last_end = end;
                pos = end;
            }
        }
    }
    out
}

/// parallel version: spans from the compressed size in the segment header; each member must end its deflate stream exactly at the span end
fn members_par(data: &[u8]) -> Option<Vec<Member>> {
    let mut spans = vec![];
    let mut pos = 0usize;
    while let Some(k) = memchr::memmem::find(&data[pos..], &GZ4) {
        let i = pos + k;
        if i < 24 { return None; }
        let size = u32::from_le_bytes(data[i - 16..i - 12].try_into().ok()?) as usize;
        if size < 18 || i + size - 8 > data.len() { if std::env::var("RVT_CORE_TIMING").is_ok() { eprintln!("rvt_core g103: bad span size {} at {} (len {})", size, i, data.len()); } return None; }
        spans.push((i, i + size - 8));
        pos = i + size - 8;
    }
    // each member: deflate stream must end within the span (at most 8 bytes early: first member of 44-byte-header partitions)
    let outs: Vec<Option<(Vec<u8>, usize)>> = crate::pool::run(|| spans
        .par_iter()
        .map(|&(a, e)| match inflate(&data[a + 10..e]) {
            Some((o, used, true)) if used + 8 >= e - a - 10 => Some((o, a + 10 + used)),
            r => {
                if std::env::var("RVT_CORE_TIMING").is_ok() { eprintln!("rvt_core g103: span {}..{} inflate -> {:?}", a, e, r.as_ref().map(|x| (x.0.len(), x.1, x.2))); }
                None
            }
        })
        .collect());
    let mut res = Vec::with_capacity(spans.len());
    let mut last_end = 0;
    for (k, ((a, _), o)) in spans.iter().zip(outs).enumerate() {
        let (o, end) = match o { Some(x) => x, None => { if std::env::var("RVT_CORE_TIMING").is_ok() { eprintln!("rvt_core g103: member {} at {} did not inflate to its span", k, a); } return None; } };
        // the reference continues its GZ search at the real end of the deflate stream: it must find the next span start (or nothing)
        let next = memchr::memmem::find(&data[end..], &GZ4).map(|x| x + end);
        if next != spans.get(k + 1).map(|s| s.0) { if std::env::var("RVT_CORE_TIMING").is_ok() { eprintln!("rvt_core g103: member {} chain mismatch {:?} vs {:?}", k, next, spans.get(k + 1)); } return None; }
        let (ch, fl) = gap_meta(data, last_end, *a);
        res.push(Member { start: *a, ch, fl, out: o });
        last_end = end;
    }
    Some(res)
}

pub fn members(data: &[u8]) -> Vec<Member> {
    members_par(data).unwrap_or_else(|| {
        if std::env::var("RVT_CORE_TIMING").is_ok() { eprintln!("rvt_core g103: sequential member scan ({} bytes)", data.len()); }
        members_seq(data)
    })
}

/// groups: consecutive members joined while the 'continues' flag (bit 1) is set -> (channel, member indices)
pub fn groups(ms: &[Member]) -> Vec<(Option<u64>, Vec<usize>)> {
    let mut out = vec![];
    let mut cur: Option<(Option<u64>, Vec<usize>)> = None;
    for (k, m) in ms.iter().enumerate() {
        if cur.is_none() { cur = Some((m.ch, vec![])); }
        cur.as_mut().unwrap().1.push(k);
        if m.fl & 2 == 0 { out.push(cur.take().unwrap()); }
    }
    if let Some(c) = cur { out.push(c); }
    out
}

/// group buffer as g103_get.get_record decodes it: 4 zero bytes + joined members
pub fn group_buf(ms: &[Member], ks: &[usize]) -> Vec<u8> {
    let n: usize = ks.iter().map(|&k| ms[k].out.len()).sum();
    let mut b = Vec::with_capacity(n + 4);
    b.extend_from_slice(&[0, 0, 0, 0]);
    for &k in ks { b.extend_from_slice(&ms[k].out); }
    b
}

pub struct Entry {
    pub rec: Rec,
    pub off: usize,
    pub size: usize,
}

/// chain of records in one group (build_index inner loop); records are kept whole (caller picks fields)
pub fn chain(s: &Schema, buf: &[u8]) -> Vec<Entry> {
    let mut d = Dec::new(s);
    d.quiet = true;
    let mut out = vec![];
    let mut p = 0usize;
    while (p as i64) < buf.len() as i64 - 30 {
        match d.record(buf, p, false) {
            Ok((r, q)) => {
                if r.bodies.is_none() { break; } // partial record: the reference raises KeyError on r['bodies'] here
                out.push(Entry { rec: r, off: p, size: q - p });
                if q == p { break; }
                p = q;
            }
            Err(_) => break,
        }
    }
    out
}

/// Lazy channel-103 group store: the compressed partition bytes stay resident, group buffers are inflated on demand and kept in a byte-capped LRU
/// (RVT_G103_CACHE_MB, default 768; RVT_G103_EAGER=1 keeps every buffer like before). Resident size ~ compressed file instead of the inflated channel.
pub struct GroupRef {
    pub part: Arc<Vec<u8>>,
    pub starts: Vec<usize>,
}

struct Cache {
    map: rustc_hash::FxHashMap<(u32, usize), (Arc<Vec<u8>>, u64)>,
    bytes: usize,
    cap: usize,
    tick: u64,
}

pub struct Store {
    parts: std::sync::RwLock<rustc_hash::FxHashMap<String, u32>>,
    groups: std::sync::RwLock<rustc_hash::FxHashMap<(u32, usize), Arc<GroupRef>>>,
    idx: std::sync::RwLock<rustc_hash::FxHashMap<u64, (u32, usize, usize)>>,
    cache: std::sync::Mutex<Cache>,
    pub eager: bool,
}

impl Store {
    pub fn new() -> Store {
        let eager = std::env::var("RVT_G103_EAGER").map(|v| v == "1").unwrap_or(false);
        let mb: usize = std::env::var("RVT_G103_CACHE_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(768);
        let cap = if eager { usize::MAX } else { mb << 20 };
        Store {
            parts: Default::default(),
            groups: Default::default(),
            idx: Default::default(),
            cache: std::sync::Mutex::new(Cache { map: Default::default(), bytes: 0, cap, tick: 0 }),
            eager,
        }
    }

    pub fn part_id(&self, pn: &str) -> u32 {
        let mut p = self.parts.write().unwrap();
        let n = p.len() as u32;
        *p.entry(pn.to_string()).or_insert(n)
    }

    pub fn find_part(&self, pn: &str) -> Option<u32> { self.parts.read().unwrap().get(pn).copied() }

    pub fn add_group(&self, part: u32, gi: usize, g: GroupRef, eager_buf: Option<Vec<u8>>) {
        self.groups.write().unwrap().insert((part, gi), Arc::new(g));
        if let (true, Some(b)) = (self.eager, eager_buf) { self.cache_put((part, gi), Arc::new(b)); }
    }

    pub fn set_idx(&self, id: u64, part: u32, gi: usize, off: usize) { self.idx.write().unwrap().insert(id, (part, gi, off)); }

    fn cache_put(&self, k: (u32, usize), b: Arc<Vec<u8>>) {
        let mut c = self.cache.lock().unwrap();
        c.tick += 1;
        let t = c.tick;
        c.bytes += b.len();
        if let Some((old, _)) = c.map.insert(k, (b, t)) { c.bytes -= old.len(); }
        while c.bytes > c.cap && c.map.len() > 1 {
            let victim = c.map.iter().filter(|(kk, _)| **kk != k).min_by_key(|(_, v)| v.1).map(|(kk, _)| *kk);
            match victim { Some(v) => { let (b, _) = c.map.remove(&v).unwrap(); c.bytes -= b.len(); } None => break }
        }
    }

    /// group buffer (4 zero bytes + joined members), inflated now when it is not cached
    pub fn buf(&self, part: u32, gi: usize) -> Option<Arc<Vec<u8>>> {
        {
            let mut c = self.cache.lock().unwrap();
            c.tick += 1;
            let t = c.tick;
            if let Some(e) = c.map.get_mut(&(part, gi)) { e.1 = t; return Some(e.0.clone()); }
        }
        let g = self.groups.read().unwrap().get(&(part, gi))?.clone();
        let mut b: Vec<u8> = vec![0, 0, 0, 0];
        for &st in &g.starts {
            let (o, _, _) = inflate(&g.part[(st + 10).min(g.part.len())..])?;
            b.extend_from_slice(&o);
        }
        let a = Arc::new(b);
        self.cache_put((part, gi), a.clone());
        Some(a)
    }

    pub fn lookup(&self, id: u64) -> Option<(Arc<Vec<u8>>, usize)> {
        let (p, gi, off) = *self.idx.read().unwrap().get(&id)?;
        Some((self.buf(p, gi)?, off))
    }
}

/// spans of the members of a partition without keeping their output (pass 1 of the chunked loader): same checks and gap metadata as members_par, None when
/// the layout is not the regular one (the caller then takes the eager path)
pub struct MemberMeta {
    pub start: usize,
    pub span_end: usize,
    pub ch: Option<u64>,
    pub fl: u32,
}

pub fn members_meta(data: &[u8]) -> Option<Vec<MemberMeta>> {
    let mut spans = vec![];
    let mut pos = 0usize;
    while let Some(k) = memchr::memmem::find(&data[pos..], &GZ4) {
        let i = pos + k;
        if i < 24 { return None; }
        let size = u32::from_le_bytes(data[i - 16..i - 12].try_into().ok()?) as usize;
        if size < 18 || i + size - 8 > data.len() { return None; }
        spans.push((i, i + size - 8));
        pos = i + size - 8;
    }
    let used: Vec<Option<usize>> = crate::pool::run(|| spans
        .par_iter()
        .map(|&(a, e)| match inflate(&data[a + 10..e]) {
            Some((o, u, true)) if u + 8 >= e - a - 10 => { drop(o); Some(a + 10 + u) }
            _ => None,
        })
        .collect());
    let mut res = Vec::with_capacity(spans.len());
    let mut last_end = 0;
    for (k, ((a, e), u)) in spans.iter().zip(used).enumerate() {
        let end = u?;
        let next = memchr::memmem::find(&data[end..], &GZ4).map(|x| x + end);
        if next != spans.get(k + 1).map(|s| s.0) { return None; }
        let (ch, fl) = gap_meta(data, last_end, *a);
        res.push(MemberMeta { start: *a, span_end: *e, ch, fl });
        last_end = end;
    }
    Some(res)
}

pub fn groups_meta(ms: &[MemberMeta]) -> Vec<(Option<u64>, Vec<usize>)> {
    let mut out = vec![];
    let mut cur: Option<(Option<u64>, Vec<usize>)> = None;
    for (k, m) in ms.iter().enumerate() {
        if cur.is_none() { cur = Some((m.ch, vec![])); }
        cur.as_mut().unwrap().1.push(k);
        if m.fl & 2 == 0 { out.push(cur.take().unwrap()); }
    }
    if let Some(c) = cur { out.push(c); }
    out
}

/// group buffer (4 zero bytes + joined members) inflated from the compressed partition; None if a member does not inflate to its span
pub fn group_buf_meta(data: &[u8], ms: &[MemberMeta], ks: &[usize]) -> Option<Vec<u8>> {
    let mut b: Vec<u8> = vec![0, 0, 0, 0];
    for &k in ks {
        let m = &ms[k];
        match inflate(&data[m.start + 10..m.span_end]) { Some((o, _, true)) => b.extend_from_slice(&o), _ => return None }
    }
    Some(b)
}
