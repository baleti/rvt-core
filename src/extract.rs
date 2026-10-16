//! Port of the per-partition loop of rvt_entities.extract (work()): header candidates, record decoding, validation, first-wins merge.
//! Decoding runs in parallel (no GIL); the merge replays the sequential reference loop exactly (RVT_WORKERS=1 semantics per partition).
use crate::dec::{Dec, Rec, Schema, Val};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

/// id membership: bitmap for small id ranges, hash set otherwise
pub struct IdSet {
    bits: Vec<u64>,
    max: u64,
    set: FxHashSet<u64>,
    use_bits: bool,
}

impl IdSet {
    pub fn new(ids: &[u64]) -> IdSet {
        let max = ids.iter().copied().max().unwrap_or(0);
        if max < (1u64 << 31) {
            let mut bits = vec![0u64; (max as usize >> 6) + 1];
            for &i in ids {
                bits[(i >> 6) as usize] |= 1 << (i & 63);
            }
            IdSet { bits, max, set: FxHashSet::default(), use_bits: true }
        } else {
            IdSet { bits: vec![], max, set: ids.iter().copied().collect(), use_bits: false }
        }
    }
    #[inline]
    pub fn has(&self, v: u64) -> bool {
        if self.use_bits { v <= self.max && (self.bits[(v >> 6) as usize] >> (v & 63)) & 1 == 1 } else { self.set.contains(&v) }
    }
}

#[inline]
fn rd(chunk: &[u8], p: usize, w: usize) -> u64 {
    if w == 4 { u32::from_le_bytes(chunk[p..p + 4].try_into().unwrap()) as u64 } else { u64::from_le_bytes(chunk[p..p + 8].try_into().unwrap()) }
}

/// rvt_scan.header_candidates_tagged: sorted positions i (0 <= i <= n-24) with an ElemTable id at i+4 (any alignment) and a class tag in `tagmask`
/// at i+12+W (long header) or i+8+W (short header)
pub fn candidates_tagged(chunk: &[u8], ids: &IdSet, w: usize, tagmask: &[bool]) -> Vec<usize> {
    let n = chunk.len();
    if n < 32 { return vec![]; }
    let hi = n - 24; // max header position
    let block = 1 << 16;
    let nb = (hi + 1 + block - 1) / block;
    let parts: Vec<Vec<usize>> = crate::pool::run(|| (0..nb)
        .into_par_iter()
        .map(|bk| {
            let mut out = vec![];
            let a = bk * block;
            let e = ((bk + 1) * block).min(hi + 1);
            for i in a..e {
                let v = rd(chunk, i + 4, w);
                if !ids.has(v) { continue; }
                let mut ok = false;
                for off in [12 + w, 8 + w] {
                    let q = i + off;
                    if q + 1 < n {
                        let t = chunk[q] as usize | (chunk[q + 1] as usize) << 8;
                        if tagmask[t] { ok = true; break; }
                    }
                }
                if ok { out.push(i); }
            }
            out
        })
        .collect());
    parts.concat()
}

#[derive(Clone, Copy, PartialEq)]
pub enum TagKind {
    Wanted,
    Header,
    Other,
}

pub struct Params<'a> {
    pub w: usize,
    /// per tag: kind
    pub kinds: &'a [TagKind],
    /// per tag: canonical tag of its class name (key of the record dict)
    pub canon: &'a [u32],
    /// per tag: keep full fields (false = slim)
    pub full: &'a [bool],
    pub only: Option<&'a FxHashSet<u64>>,
}

pub enum Out {
    Accept(Rec),
    Reject,
    /// not decoded (tag neither wanted nor ElementHeader)
    Skip,
}

pub static SLOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Cand {
    pub eid: u64,
    pub tags: [u32; 2],
    pub res: [Option<Out>; 2],
}

fn m_id_eq(s: &Schema, r: &Rec, eid: u64) -> bool {
    let k = match s.key_of.get("m_id") { Some(k) => *k, None => return false };
    match r.fields.get(k) {
        Some(Val::F(x)) => x.is_finite() && x.fract() == 0.0 && x.abs() < 1.8e19 && (*x as i128) == eid as i128,
        Some(v) => v.as_int() == Some(eid as i128),
        None => false,
    }
}

pub fn decode_variant(dec: &mut Dec, chunk: &[u8], i: usize, short: bool, tag: u32, eid: u64, p: &Params) -> Out {
    let kind = p.kinds.get(tag as usize).copied().unwrap_or(TagKind::Other);
    if kind == TagKind::Other { return Out::Skip; }
    dec.quiet = true;
    dec.abort_mid = if kind == TagKind::Wanted { Some(eid) } else { None };
    let r = dec.record(chunk, i, short);
    dec.abort_mid = None;
    let r = match r {
        Ok((r, _)) => r,
        Err(_) => return Out::Reject,
    };
    match kind {
        TagKind::Wanted => {
            if m_id_eq(dec.s, &r, eid) && r.id == eid {
                let mut r = r;
                if r.error.is_some() && p.full[tag as usize] {
                    // the record keeps its error message: decode again with messages
                    dec.quiet = false;
                    dec.abort_mid = None;
                    if let Ok((r2, _)) = dec.record(chunk, i, short) { r = r2; }
                    dec.quiet = true;
                }
                if !p.full[tag as usize] {
                    r.fields = Val::D(vec![]);
                    r.slots = vec![];
                    r.bodies = None;
                    r.error = None;
                }
                Out::Accept(r)
            } else {
                Out::Reject
            }
        }
        TagKind::Header => {
            if r.complete && r.error.is_none() && r.id == eid { Out::Accept(r) } else { Out::Reject }
        }
        TagKind::Other => Out::Skip,
    }
}

/// decode every candidate (long header; short header when the long one is not accepted) in parallel
/// `claim`: (eid, canonical tag | HDR) -> lowest candidate offset whose FIRST variant (long header) was accepted. The merge keeps the first accepted record of a key, so a variant
/// whose key was accepted by an earlier candidate is never used (it `continue`s / `break`s before looking at the result) and is not decoded: 2017 rme_advanced has ~150 false or
/// repeated ElementHeader candidates (garbage counts like 851968) of 0.3-0.9 s each. Only variant 0 claims (its accept is always taken or already taken in the merge).
pub type Claim = std::sync::RwLock<FxHashMap<(u64, u32), usize>>;
const HDR: u32 = u32::MAX;
pub fn decode_all(s: &Schema, chunk: &[u8], cands: &[usize], p: &Params, claim: Option<&Claim>) -> Vec<Cand> {
    let w = p.w;
    crate::pool::run(|| cands
        .par_chunks(8)
        .map_init(
            || Dec::new(s),
            |dec, cs| {
                let mut out = Vec::with_capacity(cs.len());
                for &i in cs {
                    let eid = rd(chunk, i + 4, w);
                    let tl = u16::from_le_bytes(chunk[i + 12 + w..i + 14 + w].try_into().unwrap()) as u32;
                    let ts = u16::from_le_bytes(chunk[i + 8 + w..i + 10 + w].try_into().unwrap()) as u32;
                    if let Some(o) = p.only {
                        if !o.contains(&eid) {
                            out.push(Cand { eid, tags: [tl, ts], res: [None, None] });
                            continue;
                        }
                    }
                    let t0 = if SLOW.load(std::sync::atomic::Ordering::Relaxed) { Some(std::time::Instant::now()) } else { None };
                    dec.big = (0, String::new());
                    let key_of = |tag: u32| -> Option<(u64, u32)> {
                        match p.kinds.get(tag as usize).copied().unwrap_or(TagKind::Other) {
                            TagKind::Wanted => Some((eid, p.canon[tag as usize])),
                            TagKind::Header => Some((eid, HDR)),
                            TagKind::Other => None,
                        }
                    };
                    let taken = |tag: u32| -> bool {
                        let cl = match claim { Some(c) => c, None => return false };
                        match key_of(tag) { Some(k) => cl.read().map_or(false, |m| m.get(&k).map_or(false, |&pos| pos < i)), None => false }
                    };
                    let (skip0, skip1) = (taken(tl), taken(ts));
                    let r0 = if skip0 { None } else { Some(decode_variant(dec, chunk, i, false, tl, eid, p)) };
                    if let (Some(Out::Accept(_)), Some(cl), Some(k)) = (&r0, claim, key_of(tl)) {
                        if let Ok(mut m) = cl.write() { let e = m.entry(k).or_insert(i); if i < *e { *e = i; } }
                    }
                    let r1 = if matches!(r0, Some(Out::Accept(_))) || skip1 { None } else { Some(decode_variant(dec, chunk, i, true, ts, eid, p)) };
                    if let Some(t0) = t0 {
                        let dt = t0.elapsed();
                        if dt.as_micros() > 2000 { let why = if matches!(r0, Some(Out::Reject)) {
                            dec.quiet = false; let big = dec.big.clone();
                            let w = match dec.record(chunk, i, false) { Ok((r, np)) => format!("ok complete {} error {:?} id {} consumed {}", r.complete, r.error.as_ref().map(|e| e.chars().take(80).collect::<String>()), r.id, np - i), Err(e) => format!("err {:?}", e) };
                            dec.quiet = true; dec.big = big; w
                        } else { String::new() };
                        eprintln!("rvt_core slow candidate at {} eid {} tags {} {} {:?} accepted {} {} big {} {} why [{}]", i, eid, dec.s.cname(tl), dec.s.cname(ts), dt, matches!(r0, Some(Out::Accept(_))), matches!(r1, Some(Out::Accept(_))), dec.big.0, dec.big.1, why); }
                    }
                    out.push(Cand { eid, tags: [tl, ts], res: [r0, r1] });
                }
                out
            },
        )
        .flatten()
        .collect())
}

/// sequential replay of work(): returns (records in insertion order keyed (eid, canonical tag), headers in insertion order)
pub fn merge(s: &Schema, chunk: &[u8], cands: &[usize], decoded: Vec<Cand>, p: &Params) -> (Vec<((u64, u32), Rec)>, Vec<(u64, Rec)>) {
    let mut st = MergeState::default();
    merge_into(s, chunk, cands, decoded, p, &mut st);
    (st.recs, st.ehs)
}

/// first-wins merge state carried across candidate batches
#[derive(Default)]
pub struct MergeState {
    pub recs: Vec<((u64, u32), Rec)>,
    rkeys: FxHashSet<(u64, u32)>,
    pub ehs: Vec<(u64, Rec)>,
    ekeys: FxHashSet<u64>,
}

/// decode + merge in batches of candidates (same order, same state as one pass): only one batch of decoded trees (incl. duplicates the merge rejects) is alive
pub fn decode_merge(s: &Schema, chunk: &[u8], cands: &[usize], p: &Params, batch: usize) -> (Vec<((u64, u32), Rec)>, Vec<(u64, Rec)>) {
    let mut st = MergeState::default();
    let claim = Claim::default();
    for part in cands.chunks(batch.max(1)) {
        let dec = decode_all(s, chunk, part, p, Some(&claim));
        merge_into(s, chunk, part, dec, p, &mut st);
    }
    (st.recs, st.ehs)
}

pub fn merge_into(s: &Schema, chunk: &[u8], cands: &[usize], mut decoded: Vec<Cand>, p: &Params, st: &mut MergeState) {
    let MergeState { recs, rkeys, ehs, ekeys } = st;
    let mut dec = Dec::new(s);
    for (ci, c) in decoded.iter_mut().enumerate() {
        if let Some(o) = p.only {
            if !o.contains(&c.eid) { continue; }
        }
        for v in 0..2 {
            let tag = c.tags[v];
            let kind = p.kinds.get(tag as usize).copied().unwrap_or(TagKind::Other);
            match kind {
                TagKind::Wanted => {
                    let key = (c.eid, p.canon[tag as usize]);
                    if rkeys.contains(&key) {
                        let t1 = c.tags[1];
                        let t1_taken = match p.kinds.get(t1 as usize).copied().unwrap_or(TagKind::Other) {
                            TagKind::Wanted => rkeys.contains(&(c.eid, p.canon[t1 as usize])),
                            TagKind::Header => ekeys.contains(&c.eid),
                            TagKind::Other => false,
                        };
                        if v == 0 && c.res[1].is_none() && !t1_taken {
                            c.res[1] = Some(decode_variant(&mut dec, chunk, cands[ci], true, c.tags[1], c.eid, p));
                        }
                        continue;
                    }
                    if let Some(Out::Accept(_)) = &c.res[v] {
                        if let Some(Out::Accept(r)) = c.res[v].take() {
                            rkeys.insert(key);
                            recs.push((key, r));
                        }
                        break;
                    }
                }
                TagKind::Header => {
                    if ekeys.contains(&c.eid) { break; }
                    if let Some(Out::Accept(_)) = &c.res[v] {
                        if let Some(Out::Accept(r)) = c.res[v].take() {
                            ekeys.insert(c.eid);
                            ehs.push((c.eid, r));
                        }
                        break;
                    }
                }
                TagKind::Other => {}
            }
        }
    }
}

pub fn canon_tags(s: &Schema) -> Vec<u32> {
    let mut first: FxHashMap<&str, u32> = FxHashMap::default();
    let mut out = vec![0u32; 65536];
    for (i, c) in s.classes.iter().enumerate() {
        let t = (i + 12) as u32;
        let f = *first.entry(c.name.as_str()).or_insert(t);
        if (t as usize) < out.len() { out[t as usize] = f; }
    }
    out
}
