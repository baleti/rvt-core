//! Python face of the record decoder (rvt_decode.FastDec uses RecDecoder.record) and of the partition extraction loop (rvt_entities.extract).
use crate::conv::Conv;
use crate::dec::{parse_pair, Class, DErr, Dec, EsField, Field, Schema};
use crate::extract::{self, IdSet, Params, TagKind};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::sync::Arc;

#[pyclass]
pub struct RecDecoder {
    pub schema: Arc<Schema>,
    pub conv: Conv,
    desync_type: Py<PyAny>,
    strcap: usize,
    ids: Option<(Vec<u64>, IdSet)>,
    /// channel-103 group buffers (4 zero bytes + joined members) by (partition name, group index)
    pub g103: rustc_hash::FxHashMap<(String, usize), Arc<Vec<u8>>>,
    /// element id -> (group buffer, offset) of its channel-103 record (last definition wins, like build_index's dict)
    pub g103_idx: rustc_hash::FxHashMap<u64, (Arc<Vec<u8>>, usize)>,
    /// binary-cache sink: serialised partition blocks of extract_* calls while active
    sink: Option<Vec<u8>>,
    writer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    /// two-stage extraction: full decoded results kept per partition until take_kept()
    /// (full records serialised in the binary-cache block format, one entry per extract call: decoded trees are not kept in memory)
    kept: Option<Vec<Vec<u8>>>,
}

pub fn classes_from_py(classes: &Bound<'_, PyList>) -> PyResult<Vec<Class>> {
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
            let ns: Vec<Option<u32>> = match f.get_item("ns")? { Some(x) if !x.is_none() => x.extract()?, _ => vec![] };
            let ns = if ns.is_empty() { vec![None; types.len()] } else { ns };
            let has_ref = f.contains("ref")?;
            let reff: Option<u32> = match f.get_item("ref")? { Some(x) if !x.is_none() => Some(x.extract()?), _ => None };
            let ref2: Option<u32> = match f.get_item("ref2")? { Some(x) if !x.is_none() => Some(x.extract()?), _ => None };
            let t0: u32 = match f.get_item("T")? { Some(x) => x.extract()?, None => types[0] };
            fields.push(Field { name: fname, key: 0, t0, types, ns, reff, has_ref, ref2, es_blob: false });
        }
        cl.push(Class { name, parent, fields });
    }
    Ok(cl)
}

pub fn es_from_py(s: &mut Schema, es: &Bound<'_, PyDict>) -> PyResult<()> {
    s.es.clear();
    for (k, v) in es.iter() {
        let g: String = k.extract()?;
        let mut fs = Vec::new();
        for f in v.downcast::<PyDict>()?.get_item("m_fields")?.unwrap().downcast::<PyList>()?.iter() {
            let f = f.downcast::<PyDict>()?;
            let name: String = f.get_item("m_fieldName")?.unwrap().extract()?;
            let tname: String = f.get_item("m_fieldTypeName")?.unwrap().extract()?;
            fs.push(EsField {
                key: s.key(&name),
                entry: f.get_item("m_entryIndex")?.unwrap().extract()?,
                container: f.get_item("m_containerType")?.unwrap().extract()?,
                pair: parse_pair(&tname),
                tname,
            });
        }
        fs.sort_by_key(|f| f.entry);
        s.es.insert(g, fs);
    }
    Ok(())
}

impl RecDecoder {
    pub fn raise(&self, py: Python<'_>, e: DErr) -> PyErr {
        match e {
            DErr::Desync(m) => match self.desync_type.bind(py).call1((m,)) { Ok(e) => PyErr::from_value_bound(e), Err(e) => e },
            DErr::Other(m) => PyValueError::new_err(m),
        }
    }
}

#[pymethods]
impl RecDecoder {
    /// classes: list of dicts as produced by fmt_parser.parse; eid_type = rvt_decode.EID; desync_type = rvt_decode.Desync
    #[new]
    fn new(py: Python<'_>, classes: &Bound<'_, PyList>, idsize: usize, stop: Vec<String>, strcap: usize, eid_type: Py<PyAny>, desync_type: Py<PyAny>) -> PyResult<Self> {
        let s = Schema::new(classes_from_py(classes)?, idsize, &stop, strcap);
        let conv = Conv::new(py, &s, eid_type);
        Ok(RecDecoder { schema: Arc::new(s), conv, desync_type, strcap, ids: None, g103: Default::default(), g103_idx: Default::default(), sink: None, writer: None, kept: None })
    }

    /// es_schemas: {guid_hex: {'m_fields': [{'m_entryIndex','m_containerType','m_fieldTypeName','m_fieldName'}]}}
    fn set_es(&mut self, py: Python<'_>, es: &Bound<'_, PyDict>) -> PyResult<()> {
        let mut s = (*self.schema).clone();
        es_from_py(&mut s, es)?;
        self.conv.sync(py, &s);
        self.schema = Arc::new(s);
        Ok(())
    }

    fn set_strcap(&mut self, n: usize) { self.strcap = n; }

    /// header [u32 A][id][u64 stamp][u16 tag] (short: u32 stamp) -> (record dict, end)
    fn record<'py>(&mut self, py: Python<'py>, b: &Bound<'py, PyBytes>, p: usize, short: bool) -> PyResult<(Bound<'py, PyDict>, usize)> {
        let s = self.schema.clone();
        let mut d = Dec::new(&s);
        d.strcap = self.strcap;
        match d.record(b.as_bytes(), p, short) {
            Ok((r, q)) => Ok((self.conv.rec(py, &r)?, q)),
            Err(e) => Err(self.raise(py, e)),
        }
    }

    /// g103_get.Part(o, pn) + the build_index loop for one partition: keeps the channel-103 group buffers, returns the index entries
    /// [(id, group index, off, m_bBox, m_tightbBox, m_gElemType, [(body class, count)], size, complete)] in decode order
    fn g103_load<'py>(&mut self, py: Python<'py>, pn: String, stored: &[u8]) -> PyResult<Bound<'py, PyList>> {
        let s = self.schema.clone();
        let (bufs, entries) = py.allow_threads(|| {
            let data = crate::strip_pages_impl(stored);
            let ms = crate::g103::members(&data);
            let gs = crate::g103::groups(&ms);
            let bufs: Vec<(usize, Vec<u8>)> = gs.iter().enumerate().filter(|(_, (ch, _))| *ch == Some(103)).map(|(gi, (_, ks))| (gi, crate::g103::group_buf(&ms, ks))).collect();
            drop(ms);
            let entries: Vec<(usize, Vec<crate::g103::Entry>)> = crate::pool::run(|| bufs.par_iter().map(|(gi, b)| (*gi, crate::g103::chain(&s, b))).collect());
            (bufs, entries)
        });
        let k_bb = s.key_of.get("m_bBox").copied();
        let k_tb = s.key_of.get("m_tightbBox").copied();
        let k_et = s.key_of.get("m_gElemType").copied();
        let out = PyList::empty_bound(py);
        for (gi, es) in entries.iter() {
            for e in es {
                let r = &e.rec;
                let pick = |k: Option<u32>| -> PyResult<PyObject> {
                    match k.and_then(|k| r.fields.get(k)) { Some(v) => self.conv.val(py, v, &r.slots), None => Ok(py.None()) }
                };
                let mut cnt: Vec<(String, usize)> = vec![];
                for t in r.bodies.as_ref().unwrap() {
                    let n = s.cname(*t);
                    match cnt.iter_mut().find(|(m, _)| *m == n) { Some(c) => c.1 += 1, None => cnt.push((n, 1)) }
                }
                let t = PyTuple::new_bound(py, [r.id.into_py(py), gi.into_py(py), e.off.into_py(py), pick(k_bb)?, pick(k_tb)?, pick(k_et)?, cnt.into_py(py), e.size.into_py(py), r.complete.into_py(py)]);
                out.append(t)?;
            }
        }
        let mut arcs: rustc_hash::FxHashMap<usize, Arc<Vec<u8>>> = Default::default();
        for (gi, b) in bufs { let a = Arc::new(b); self.g103.insert((pn.clone(), gi), a.clone()); arcs.insert(gi, a); }
        for (gi, es) in entries.iter() { for e in es { self.g103_idx.insert(e.rec.id, (arcs[gi].clone(), e.off)); } }
        Ok(out)
    }

    /// g103_get.get_record: the record at `off` of channel-103 group `gi` of partition `pn` (None if that group is not loaded)
    fn g103_record<'py>(&self, py: Python<'py>, pn: String, gi: usize, off: usize) -> PyResult<Option<Bound<'py, PyDict>>> {
        let b = match self.g103.get(&(pn, gi)) { Some(b) => b.clone(), None => return Ok(None) };
        let s = self.schema.clone();
        let mut d = Dec::new(&s);
        d.strcap = self.strcap;
        match d.record(&b, off, false) {
            Ok((r, _)) => Ok(Some(self.conv.rec(py, &r)?)),
            Err(e) => Err(self.raise(py, e)),
        }
    }

    /// rvt_es.find_schemas(d, sch, W) on the inflated Global/Latest stream -> {guid_hex: schema dict}
    fn es_find_schemas<'py>(&self, py: Python<'py>, d: &[u8]) -> PyResult<Bound<'py, PyDict>> {
        let s = self.schema.clone();
        let found = py.allow_threads(|| crate::es::find_schemas(&s, d));
        let out = PyDict::new_bound(py);
        for f in found { out.set_item(&f.guid, self.conv.val(py, &f.sc, &f.slots)?)?; }
        Ok(out)
    }

    /// element-mesh context over the loaded channel-103 records; census: class per element id (rvt_elemmesh.Doc.cls), ws: WallSweep type per id
    fn mesh_doc(&self, py: Python<'_>, cls: &Bound<'_, PyDict>, ws: &Bound<'_, PyDict>) -> PyResult<crate::elemmesh::MeshDoc> {
        let mut c: rustc_hash::FxHashMap<u64, String> = Default::default();
        for (k, v) in cls.iter() { c.insert(k.extract()?, v.extract()?); }
        let mut w: rustc_hash::FxHashMap<u64, i64> = Default::default();
        for (k, v) in ws.iter() { w.insert(k.extract()?, v.extract()?); }
        let ctx = crate::elemmesh::Ctx::new(self.schema.clone(), self.g103_idx.clone(), c, w);
        let conv = Conv::new(py, &self.schema, self.conv.eid.clone_ref(py));
        crate::elemmesh::MeshDoc::new(py, ctx, conv)
    }

    /// keep the full decoded results of the following extract_* calls (their returned view may be slim)
    fn keep_begin(&mut self) { self.kept = Some(vec![]); }

    /// the kept results converted in full: [(records, headers)] per extract call, same form as extract_chunk returns
    fn take_kept<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let kept = self.kept.take().ok_or_else(|| PyValueError::new_err("keep_begin first"))?;
        let geo = PyString::new_bound(py, "<geometry not decoded>");
        let out = PyList::empty_bound(py);
        for b in kept.iter() {
            let mut rd = crate::cache::Rd::new(py, b, 0);
            let lr = PyList::empty_bound(py);
            let le = PyList::empty_bound(py);
            while rd.p < b.len() {
                let nr = rd.u32()? as usize;
                for _ in 0..nr {
                    let eid = rd.u64()?;
                    let ct = rd.u32()?;
                    let key = PyTuple::new_bound(py, [eid.into_py(py), self.conv.cname(py, ct)]);
                    let r = rd.rec(py, &self.conv, &self.conv.keys, &geo)?;
                    lr.append(PyTuple::new_bound(py, [key.into_py(py), r.into_py(py)]))?;
                }
                let ne = rd.u32()? as usize;
                for _ in 0..ne {
                    let eid = rd.u64()?;
                    let v = rd.val(py, &self.conv, &self.conv.keys, &geo)?;
                    le.append(PyTuple::new_bound(py, [eid.into_py(py), v.unbind()]))?;
                }
            }
            out.append(PyTuple::new_bound(py, [lr.into_py(py), le.into_py(py)]))?;
        }
        Ok(out)
    }

    /// start collecting extraction results for the binary cache
    fn cache_begin(&mut self) { self.sink = Some(Vec::new()); }

    /// write the collected blocks: header = JSON text (the caller's versions + hashes); the schema key table is appended to it
    fn cache_write(&mut self, path: &str, header: &str) -> PyResult<()> {
        let blocks = self.sink.take().ok_or_else(|| PyValueError::new_err("cache_begin first"))?;
        let keys = serde_json::to_string(&self.schema.keys).unwrap();
        let hdr = format!("{{\"user\":{},\"keys\":{}}}", header, keys);
        let tmp = format!("{}.{}.tmp", path, std::process::id());
        let path = path.to_string();
        // written by a background thread: the caller does not wait for the disk (a process that exits first leaves only the .tmp)
        let h = std::thread::spawn(move || -> std::io::Result<()> {
            use std::io::Write;
            {
                let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
                f.write_all(b"RVTC")?;
                f.write_all(&crate::cache::FORMAT.to_le_bytes())?;
                f.write_all(&(hdr.len() as u32).to_le_bytes())?;
                f.write_all(hdr.as_bytes())?;
                f.write_all(&blocks)?;
                f.flush()?;
            }
            std::fs::rename(&tmp, &path)
        });
        self.writer = Some(h);
        Ok(())
    }

    /// wait for a pending background cache write (tests)
    fn cache_flush(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(h) = self.writer.take() { py.allow_threads(|| h.join()).map_err(|_| PyValueError::new_err("cache writer panicked"))??; }
        Ok(())
    }

    /// header JSON of a cache file (None when it is not a cache file of this format)
    #[staticmethod]
    fn cache_header(path: &str) -> PyResult<Option<String>> {
        use std::io::Read;
        let mut f = match std::fs::File::open(path) { Ok(f) => f, Err(_) => return Ok(None) };
        let mut h = [0u8; 12];
        if f.read_exact(&mut h).is_err() || &h[0..4] != b"RVTC" || u32::from_le_bytes(h[4..8].try_into().unwrap()) != crate::cache::FORMAT { return Ok(None); }
        let n = u32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
        let mut b = vec![0u8; n];
        if f.read_exact(&mut b).is_err() { return Ok(None); }
        Ok(String::from_utf8(b).ok())
    }

    /// load a cache file written with this schema -> (recs, eh) dicts, merged first-wins in partition order; None if the key table differs
    fn cache_load<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Option<(Bound<'py, PyDict>, Bound<'py, PyDict>)>> {
        let f = std::fs::File::open(path)?;
        let mm = unsafe { memmap2::Mmap::map(&f)? };
        let b: &[u8] = &mm;
        if b.len() < 12 || &b[0..4] != b"RVTC" { return Ok(None); }
        let n = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
        let hdr: serde_json::Value = match serde_json::from_slice(&b[12..12 + n]) { Ok(v) => v, Err(_) => return Ok(None) };
        let keys: Vec<String> = match serde_json::from_value(hdr["keys"].clone()) { Ok(k) => k, Err(_) => return Ok(None) };
        let kt: Vec<Py<PyString>> = keys.iter().map(|k| PyString::intern_bound(py, k).unbind()).collect();
        let geo = PyString::new_bound(py, "<geometry not decoded>");
        let mut rd = crate::cache::Rd::new(py, b, 12 + n);
        let recs = PyDict::new_bound(py);
        let eh = PyDict::new_bound(py);
        while rd.p < b.len() {
            let nr = rd.u32()? as usize;
            for _ in 0..nr {
                let eid = rd.u64()?;
                let ct = rd.u32()?;
                let key = PyTuple::new_bound(py, [eid.into_py(py), self.conv.cname(py, ct)]);
                let r = rd.rec(py, &self.conv, &kt, &geo)?;
                if !recs.contains(&key)? { recs.set_item(key, r)?; }
            }
            let ne = rd.u32()? as usize;
            for _ in 0..ne {
                let eid = rd.u64()?;
                let v = rd.val(py, &self.conv, &kt, &geo)?;
                if !eh.contains(eid)? { eh.set_item(eid, v)?; }
            }
        }
        Ok(Some((recs, eh)))
    }

    /// ElemTable ids (sorted) used by extract_chunk
    fn set_ids(&mut self, ids: Vec<u64>) {
        let set = IdSet::new(&ids);
        self.ids = Some((ids, set));
    }

    /// rvt_scan.header_candidates_tagged with the ids of set_ids
    fn candidates_tagged(&self, py: Python<'_>, chunk: &[u8], tags: Vec<u32>) -> PyResult<Vec<usize>> {
        let (_, ids) = self.ids.as_ref().ok_or_else(|| PyValueError::new_err("set_ids first"))?;
        let mut mask = vec![false; 65536];
        for t in tags { if (t as usize) < mask.len() { mask[t as usize] = true; } }
        let w = self.schema.idsize;
        Ok(py.allow_threads(|| extract::candidates_tagged(chunk, ids, w, &mask)))
    }

    /// The candidate loop of rvt_entities.extract for one decoded partition.
    /// wanted: class tags to keep as records; eh_tag: ElementHeader tag; slim: tags whose records are reduced to {'id','class','fields':{},'complete'}
    /// -> ([((eid, class), rec)], [(eid, header fields)]) in first-wins order
    #[pyo3(signature = (chunk, wanted, eh_tag, only_ids=None, slim=None))]
    fn extract_chunk<'py>(&mut self, py: Python<'py>, chunk: &[u8], wanted: Vec<u32>, eh_tag: i64, only_ids: Option<Vec<u64>>, slim: Option<Vec<u32>>) -> PyResult<Bound<'py, PyTuple>> {
        self.extract_impl(py, chunk, wanted, eh_tag, only_ids, slim)
    }

    /// extract_chunk on a raw (stored) partition stream: pages stripped and members inflated here. None when the member layout is unexpected (caller falls back).
    #[pyo3(signature = (stored, wanted, eh_tag, only_ids=None, slim=None))]
    fn extract_stored<'py>(&mut self, py: Python<'py>, stored: &[u8], wanted: Vec<u32>, eh_tag: i64, only_ids: Option<Vec<u64>>, slim: Option<Vec<u32>>) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let chunk = match py.allow_threads(|| crate::joined_vec(stored)) { Some(c) => c, None => return Ok(None) };
        Ok(Some(self.extract_impl(py, &chunk, wanted, eh_tag, only_ids, slim)?))
    }
}

impl RecDecoder {
    fn extract_impl<'py>(&mut self, py: Python<'py>, chunk: &[u8], wanted: Vec<u32>, eh_tag: i64, only_ids: Option<Vec<u64>>, slim: Option<Vec<u32>>) -> PyResult<Bound<'py, PyTuple>> {
        let keep = self.kept.is_some();
        let s = self.schema.clone();
        let (_, ids) = self.ids.as_ref().ok_or_else(|| PyValueError::new_err("set_ids first"))?;
        let mut kinds = vec![TagKind::Other; 65536];
        let mut mask = vec![false; 65536];
        if eh_tag >= 0 && (eh_tag as usize) < kinds.len() { kinds[eh_tag as usize] = TagKind::Header; mask[eh_tag as usize] = true; }
        for &t in &wanted { if (t as usize) < kinds.len() { kinds[t as usize] = TagKind::Wanted; mask[t as usize] = true; } }
        let mut full = vec![true; 65536];
        if let Some(sl) = &slim { for &t in sl { if (t as usize) < full.len() { full[t as usize] = false; } } }
        // keep mode: decode everything in full (kept for take_kept), apply the slim form only to the Python view returned now
        let full_dec: Vec<bool> = if keep { vec![true; 65536] } else { full.clone() };
        let canon = extract::canon_tags(&s);
        let only: Option<FxHashSet<u64>> = only_ids.map(|v| v.into_iter().collect());
        let w = s.idsize;
        let p = Params { w, kinds: &kinds, canon: &canon, full: &full_dec, only: only.as_ref() };
        let tm = std::env::var("RVT_CORE_TIMING").is_ok();
        extract::SLOW.store(std::env::var("RVT_CORE_SLOW").is_ok(), std::sync::atomic::Ordering::Relaxed);
        let t0 = std::time::Instant::now();
        let want_block = self.sink.is_some() || keep;
        let cands = py.allow_threads(|| extract::candidates_tagged(chunk, ids, w, &mask));
        let t1 = t0.elapsed();
        // batches of candidates: decode + first-wins merge (state carried over), then the newly accepted records are serialised, converted and dropped,
        // so only one batch of decoded trees is alive (2020 rme_advanced: 775k candidates, > 4 GB when decoded at once)
        let mut st = extract::MergeState::default();
        let mut block: Vec<u8> = vec![];
        let lr = PyList::empty_bound(py);
        let le = PyList::empty_bound(py);
        let mut tconv = std::time::Duration::ZERO;
        for part in cands.chunks(2048) {
            let (recs, ehs, b) = py.allow_threads(|| {
                let dec = extract::decode_all(&s, chunk, part, &p);
                extract::merge_into(&s, chunk, part, dec, &p, &mut st);
                let recs = std::mem::take(&mut st.recs);
                let ehs = std::mem::take(&mut st.ehs);
                let b = if want_block {
                    let mut b: Vec<u8> = Vec::new();
                    b.extend_from_slice(&(recs.len() as u32).to_le_bytes());
                    for ((eid, ct), r) in recs.iter() { b.extend_from_slice(&eid.to_le_bytes()); b.extend_from_slice(&ct.to_le_bytes()); crate::cache::put_rec(&mut b, r, !p.full[r.tag as usize]); }
                    b.extend_from_slice(&(ehs.len() as u32).to_le_bytes());
                    for (eid, r) in ehs.iter() { b.extend_from_slice(&eid.to_le_bytes()); crate::cache::put_val(&mut b, &r.fields, &r.slots); }
                    Some(b)
                } else { None };
                (recs, ehs, b)
            });
            if let Some(b) = b { block.extend_from_slice(&b); }
            let tc = std::time::Instant::now();
            for ((eid, ct), r) in recs.iter() {
                let key = PyTuple::new_bound(py, [eid.into_py(py), self.conv.cname(py, *ct)]);
                let d = if full[r.tag as usize] { self.conv.rec(py, r)? } else { self.conv.rec_slim(py, r)? };
                lr.append(PyTuple::new_bound(py, [key.into_py(py), d.into_py(py)]))?;
            }
            for (eid, r) in ehs.iter() {
                le.append(PyTuple::new_bound(py, [eid.into_py(py), self.conv.val(py, &r.fields, &r.slots)?]))?;
            }
            tconv += tc.elapsed();
        }
        if tm { eprintln!("rvt_core extract: {} bytes, {} candidates: scan {:?} decode+merge+convert {:?} (to python {:?})", chunk.len(), cands.len(), t1, t0.elapsed() - t1, tconv); }
        if keep {
            if let Some(sk) = self.sink.as_mut() { sk.extend_from_slice(&block); }
            if let Some(k) = self.kept.as_mut() { k.push(block); }
        } else if let Some(sk) = self.sink.as_mut() { sk.extend_from_slice(&block); }
        Ok(PyTuple::new_bound(py, [lr.into_py(py), le.into_py(py)]))
    }
}
