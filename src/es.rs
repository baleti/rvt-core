//! Extensible-storage schema scan over Global/Latest: port of rvt_es._find_schemas (numpy prefilter + sequential candidate loop, same results).
use crate::dec::{Dec, Schema, Slot, Val};
use rayon::prelude::*;

pub struct Found {
    pub guid: String,
    pub sc: Val,
    pub slots: Vec<Slot>,
}

/// Python re `^[A-Za-z0-9_\- ]{0,64}$` with re.match (`$` also matches before one final newline)
fn name_ok(s: &str) -> bool {
    let b = s.strip_suffix('\n').unwrap_or(s);
    b.chars().count() <= 64 && b.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
}

/// a string value: decoded, or a lazy UTF-16 range in d
enum Sv<'a> { S(&'a str), R(&'a [u8]) }
fn sval<'a>(v: &'a Val, d: &'a [u8]) -> Option<Sv<'a>> {
    match v { Val::S(x) => Some(Sv::S(x)), Val::R(a, n) => Some(Sv::R(&d[*a..*a + 2 * n])), _ => None }
}
fn sv_empty(v: &Sv) -> bool { match v { Sv::S(x) => x.is_empty(), Sv::R(b) => b.is_empty() } }
fn sv_name_ok(v: &Sv) -> bool {
    match v {
        Sv::S(x) => name_ok(x),
        Sv::R(b) => {
            let n = b.len() / 2;
            if n > 65 { return false; }
            name_ok(&crate::dec::utf16le(b))
        }
    }
}

/// rvt_es._good; Err = the reference raises (TypeError/KeyError), which discards the candidate
fn good(s: &Schema, d: &[u8], key: &str, sc: &Val) -> Result<bool, ()> {
    let k = |n: &str| s.key_of.get(n).copied();
    let g = sc.get(k("m_guid").ok_or(())?).ok_or(())?;
    let gg = match g { Val::D(_) => g.get(k("m_guid").ok_or(())?).ok_or(())?, _ => return Err(()) };
    let gstr = match sval(gg, d) { Some(Sv::S(x)) => x.to_string(), Some(Sv::R(b)) => crate::dec::utf16le(b), None => return Ok(false) }; // str == non-str is False, no exception
    if key != gstr { return Ok(false); }
    let name = sc.get(k("m_schemaName").ok_or(())?).ok_or(())?;
    // (m_schemaName != '' or len(m_fields) > 0)
    let nonempty = match sval(name, d) { Some(x) => !sv_empty(&x), None => true };
    if !nonempty {
        match sc.get(k("m_fields").ok_or(())?).ok_or(())? { Val::L(l) => { if l.is_empty() { return Ok(false); } } _ => return Err(()) }
    }
    let name_s = sval(name, d).ok_or(())?;
    if !sv_name_ok(&name_s) { return Ok(false); }
    let vend = sval(sc.get(k("m_vendorId").ok_or(())?).ok_or(())?, d).ok_or(())?;
    if !sv_name_ok(&vend) { return Ok(false); }
    let kf = k("m_fieldName").ok_or(())?;
    match sc.get(k("m_fields").ok_or(())?).ok_or(())? {
        Val::L(l) => {
            for f in l {
                let fname = match f { Val::D(_) => f.get(kf).ok_or(())?, _ => return Err(()) };
                match sval(fname, d) { Some(x) => { if !sv_name_ok(&x) { return Ok(false); } } None => return Err(()) }
            }
        }
        _ => return Err(()),
    }
    Ok(true)
}

pub fn find_schemas(s: &Schema, d: &[u8]) -> Vec<Found> {
    let mut out: Vec<Found> = vec![];
    let ci = match s.classes.iter().position(|c| c.name == "ESSchema") { Some(c) => c, None => return out };
    let doc_key = s.key_of.get("m_documentation").copied().unwrap_or(u32::MAX);
    let n = d.len();
    if n < 11 { return out; }
    let l = n - 11; // number of u32 positions in the reference's u32 array
    let u = |p: usize| u32::from_le_bytes(d[p..p + 4].try_into().unwrap());
    let mut dec = Dec::new(s);
    dec.strcap = 100_000;
    dec.quiet = true;
    dec.lazy_str = true;
    for &(pre, mid, post, counted) in &[(0usize, 0usize, 0usize, true), (0, 0, 4, true), (1, 4, 0, false), (1, 0, 4, false)] {
        let lim = l as i64 - 60 - if counted { 4 } else { 0 };
        if lim <= 0 { continue; }
        let lim = lim as usize;
        let off = if counted { 4 } else { 0 } + pre + 16 + post;
        let cands: Vec<usize> = crate::pool::run(|| (0..lim)
            .into_par_iter()
            .filter(|&i| {
                let qq = i + off;
                if !(qq + 6 < l) { return false; }
                let ln = u(qq);
                if ln > 4000 { return false; }
                if counted {
                    let c = u(i);
                    if !(1..=5000).contains(&c) { return false; }
                }
                let p2 = (qq + 4 + 2 * ln as usize + mid).min(l - 1);
                if u(p2) > 500 { return false; }
                let (ch, ch2) = if ln > 0 { (d[(qq + 4).min(n - 1)], d[(qq + 5).min(n - 1)]) } else { (0x20, 0) };
                (0x20..0x7f).contains(&ch) && ch2 == 0
            })
            .collect());
        let mut ci_ = 0usize;
        while ci_ < cands.len() {
            let p = cands[ci_];
            ci_ += 1;
            let q = if counted { p + 4 } else { p };
            let cnt = if counted { u(p) as usize } else { 100_000 };
            let qq2 = q + pre + 16 + post;
            if qq2 + 4 > n { break; }
            let ln = u(qq2) as usize;
            if !(ln <= 4000 && qq2 + 4 + 2 * ln <= n && (0..ln.min(3)).all(|i| d[qq2 + 4 + 2 * i + 1] == 0 && (0x20..0x7f).contains(&d[qq2 + 4 + 2 * i]))) { continue; }
            let mut pp = q;
            let mut got: Vec<Found> = vec![];
            let mut raised = false;
            for _ in 0..cnt {
                let (key, sc, q2) = match dec.es_entry(d, pp, pre, mid, post, ci, doc_key) { Ok(x) => x, Err(_) => break };
                let sc = Val::D(sc);
                match good(s, d, &key, &sc) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(()) => { raised = true; break; }
                }
                // accepted: decode the entry again with real strings
                dec.lazy_str = false;
                let (sc, slots) = match dec.es_entry(d, pp, pre, mid, post, ci, doc_key) { Ok((_, sc, _)) => (Val::D(sc), dec.take_slots()), Err(_) => { dec.lazy_str = true; break; } };
                dec.lazy_str = true;
                got.push(Found { guid: key, sc, slots });
                pp = q2;
            }
            if raised || got.is_empty() { continue; }
            for f in got {
                match out.iter_mut().find(|o| o.guid == f.guid) { Some(o) => { o.sc = f.sc; o.slots = f.slots; } None => out.push(f) }
            }
            // bisect.bisect_left(cands, pp, ci)
            let rest = &cands[ci_..];
            ci_ += rest.partition_point(|&c| c < pp);
        }
    }
    out
}
