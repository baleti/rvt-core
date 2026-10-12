"""Rust fast paths for the reader in ~/rvt-research (session 176's Python stays the reference oracle and the fallback).
Every function here has the SAME signature and result as the oracle function it replaces; differential tests in ~/src/rvt-core/tests.
RVT_NOFAST=1 (or a missing extension, or any experiment/hint configuration) -> the oracle function is called instead."""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
READER = os.environ.get('RVT_READER_DIR', os.path.expanduser('~/rvt-research'))
if READER not in sys.path: sys.path.append(READER)
if HERE not in sys.path: sys.path.insert(0, HERE)
os.environ.setdefault('RVT_GEOM', '1')

def _load_ext():
    """the extension built next to this file (rvt_core.abi3.so), registered as 'rvt_core' so the oracle's own optional import (rvt_decode.FastDec) uses the same build"""
    m = sys.modules.get('rvt_core')
    if m is not None and hasattr(m, 'RecDecoder') and hasattr(m.RecDecoder, 'extract_stored'): return m
    import importlib.util
    fn = os.path.join(HERE, 'rvt_core.abi3.so')
    if not os.path.exists(fn): return None
    spec = importlib.util.spec_from_file_location('rvt_core', fn); m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
    sys.modules['rvt_core'] = m
    return m

try: _EXT = None if os.environ.get('RVT_NOFAST') else _load_ext()
except Exception: _EXT = None

def _rc():
    return None if os.environ.get('RVT_NOFAST') else _EXT

def _decoder_default_config():
    """the Rust decoder implements rvt_decode.Dec's default configuration only"""
    import rvt_decode as D
    return not (D.load_hints() or os.environ.get('RVT_GG', '0') != '0' or os.environ.get('RVT_M') or os.environ.get('RVT_K5'))

def on():
    """True when the Rust paths apply (extension present, RVT_NOFAST unset, decoder in its default configuration). Hooks in the oracle call
    rvt_fast only when on() is True, and rvt_fast calls the oracle only when it is False: no recursion."""
    try: return _rc() is not None and not os.environ.get('RVT_OLDCAND') and _decoder_default_config()
    except Exception: return False

def make_recdecoder(sch, W, es=None):
    import rvt_decode as D
    RC = _rc()
    rd = RC.RecDecoder(sch.c, W, list(D.Dec.STOP), D.STRCAP, D.EID, D.Desync)
    if es: rd.set_es(es)
    return rd

def find_schemas(d, sch, W, rd=None):
    """== rvt_es.find_schemas(d, sch, W)"""
    if not on():
        import rvt_es as ES; return ES.find_schemas(d, sch, W)
    if 'ESSchema' not in sch.by: return {}
    rd = rd or make_recdecoder(sch, W)
    return rd.es_find_schemas(bytes(d))

def es_load(path):
    """== rvt_es.load(path) -> (schemas, sch); same cache (~/.cache/rvt-schema/es-<sha1 of Global/Latest>.pkl, honours RVT_NOCACHE)"""
    import olefile, fmt_parser as fp, hashlib, pickle, rvt_decode as D, rvt_scan as S
    from rvtfmt import inflate_stream
    o = olefile.OleFileIO(path); cls, _ = fp.parse(inflate_stream(o.openstream('Formats/Latest').read())[1]); sch = D.Schema(cls); W = S.id_size(cls)
    pre, d, un = inflate_stream(o.openstream('Global/Latest').read())
    cd = os.path.expanduser('~/.cache/rvt-schema'); fn = os.path.join(cd, 'es-' + hashlib.sha1(bytes(d)).hexdigest() + '.pkl')
    if not os.environ.get('RVT_NOCACHE'):
        try:
            with open(fn, 'rb') as fh: return pickle.load(fh), sch
        except Exception: pass
    r = find_schemas(d, sch, W)
    if os.environ.get('RVT_NOCACHE'): return r, sch
    try:
        os.makedirs(cd, exist_ok=True); tmp = fn + '.%d.tmp' % os.getpid()
        with open(tmp, 'wb') as fh: pickle.dump(r, fh, protocol=5)
        os.replace(tmp, fn)
    except Exception: pass
    return r, sch

def extract(path, extra=(), only_ids=None, want_all=False, slim=None):
    """== rvt_entities.extract (same arguments, same result)"""
    import rvt_entities as E
    if not on(): return E.extract(path, extra=extra, only_ids=only_ids, want_all=want_all, slim=slim)
    return _extract(path, extra, only_ids, want_all, slim)

def extract_two_stage(path, slim):
    """(x_slim, finish): x_slim == extract(path, want_all=True, slim=slim) now; finish() -> extract(path, want_all=True), converted from the same decode
    (no second inflate/decode). The binary cache is written in the full variant."""
    return _extract(path, (), None, True, slim, two_stage=True)

def _extract(path, extra, only_ids, want_all, slim, two_stage=False):
    import rvt_entities as E, rvt_scan as S, rvt_decode as D, rvt2text as T, rvt_inventory as I
    import gc, json, hashlib
    o, cls, ids = S.load(path); W = S.id_size(cls); sch = D.Schema(cls)
    EH = sch.by.get('ElementHeader', -999) + 12; names = {i + 12: c['name'] for i, c in enumerate(cls)}; et = I.element_tags(cls)
    wanted = {t for t, n in names.items() if t in et and (n in E.WANT or n.startswith(E.WANT_PREFIX)) and n != 'DBView'}
    if extra or want_all: wanted = {t for t, n in names.items() if t in et and (want_all or n in extra)}
    slim_tags = sorted(t for t in wanted if slim is not None and not slim(names[t])) if slim is not None else None
    oid = sorted(only_ids) if only_ids is not None else None
    wl = sorted(wanted)
    cache_slim = None if two_stage else slim_tags                 # two-stage: the cache holds the full variant
    cfn = hdr = None
    if only_ids is None and _cache_on():
        hdr = json.dumps({'reader': reader_version(), 'schema': hashlib.sha1(o.openstream('Formats/Latest').read()).hexdigest(), 'file': file_key(path), 'W': W})
        cfn = os.path.join(CACHE, '%s-x%s.rvtc' % (file_key(path), hashlib.sha1(repr((wl, EH, cache_slim)).encode()).hexdigest()[:10]))
        h = _rc().RecDecoder.cache_header(cfn)
        if h is not None:
            try: user = json.loads(h)['user']
            except Exception: user = None
            if user == json.loads(hdr):
                rd = make_recdecoder(sch, W)
                gc_was = gc.isenabled(); gc.disable()
                try: got = rd.cache_load(cfn)
                except Exception: got = None
                finally:
                    if gc_was: gc.enable()
                if got is not None:
                    x = dict(eh=got[1], recs=got[0], W=W, sch=sch)
                    return (x, lambda: x) if two_stage else x
    try: es = es_load(path)[0]
    except Exception: es = {}
    rd = make_recdecoder(sch, W, es); rd.set_ids(sorted(ids))
    if cfn: rd.cache_begin()
    if two_stage: rd.keep_begin()
    recs = {}; eh = {}
    gc_was = gc.isenabled(); gc.disable()                   # millions of new container objects: cyclic GC passes would cost more than the decoding
    try:
        ok = _partitions(o, rd, wl, EH, oid, slim_tags, recs, eh, T)
    finally:
        if gc_was: gc.enable()
    if cfn and ok:
        try: os.makedirs(CACHE, exist_ok=True); rd.cache_write(cfn, hdr)
        except Exception: pass
    x = dict(eh=eh, recs=recs, W=W, sch=sch)
    if not two_stage: return x
    def finish():
        r2 = {}; e2 = {}
        gc_was = gc.isenabled(); gc.disable()
        try:
            for lr, le in rd.take_kept():
                for k, r in lr: r2.setdefault(k, r)
                for k, v in le: e2.setdefault(k, v)
        finally:
            if gc_was: gc.enable()
        return dict(eh=e2, recs=r2, W=W, sch=sch)
    return x, finish

_RV = {}
def reader_version():
    """content hash of the reference modules whose semantics the fast paths reproduce + the extension build (cache version key)"""
    import hashlib
    fs = ['rvt_decode.py', 'rvt_entities.py', 'rvt_scan.py', 'rvt_es.py', 'fmt_parser.py', 'rvtfmt.py', 'rvt2text.py', 'rvt_inventory.py']
    st = tuple((f, os.stat(os.path.join(READER, f)).st_mtime_ns) for f in fs if os.path.exists(os.path.join(READER, f)))
    so = os.path.join(HERE, 'rvt_core.abi3.so'); sst = os.stat(so) if os.path.exists(so) else None
    k = (st, sst and (sst.st_size, sst.st_mtime_ns))
    if k not in _RV:
        h = hashlib.sha1()
        for f, _ in st: h.update(open(os.path.join(READER, f), 'rb').read())
        if sst: h.update(open(so, 'rb').read())
        _RV.clear(); _RV[k] = h.hexdigest()[:16]
    return _RV[k]

def _partitions(o, rd, wl, EH, oid, slim_tags, recs, eh, T, progress=None):
    pns = sorted(x for x in ['/'.join(e) for e in o.listdir(streams=True)] if x.startswith('Partitions/'))
    nb = 0
    for i, pn in enumerate(pns):
        raw = o.openstream(pn).read(); nb += len(raw)
        res = rd.extract_stored(raw, wl, EH, oid, slim_tags)
        if res is None: res = rd.extract_chunk(T.joined(raw), wl, EH, oid, slim_tags)
        del raw
        for k, r in res[0]: recs.setdefault(k, r)
        for k, v in res[1]: eh.setdefault(k, v)
        if progress: progress.set(done=i + 1, total=len(pns), bytes_read=nb, elements_decoded=len(recs))
    return True

# ---------------- channel-103 saved graphics (g103_get) ----------------
class G103:
    """per-file state: RecDecoder with every channel-103 group buffer loaded, and the index"""
    def __init__(s, f):
        import olefile, rvt_scan as S, rvt_decode as D, rvt_es as ES
        o, cls, ids = S.load(f); W = S.id_size(cls); sch = D.Schema(cls)
        try: es = es_load(f)[0]
        except Exception: es = {}
        s.f = f; s.W = W; s.sch = sch; s.rd = make_recdecoder(sch, W, es); s.index = {}
        import gc
        gc_was = gc.isenabled(); gc.disable()
        try:
            for pn in sorted(n for n in ['/'.join(e) for e in o.listdir(streams=True)] if n.startswith('Partitions/')):
                for (eid, gi, off, bb, tb, et, bodies, size, complete) in s.rd.g103_load(pn, o.openstream(pn).read()):
                    s.index[eid] = dict(pn=pn, seg=gi, off=off, bbox=bb, tight=tb, etype=et, bodies=dict(bodies), size=size, complete=complete)
        finally:
            if gc_was: gc.enable()
    def record(s, info):
        """== g103_get.get_record(f, info)"""
        r = s.rd.g103_record(info['pn'], info['seg'], info['off'])
        if r is None: raise KeyError((info['pn'], info['seg']))
        return r

_G103 = {}
def g103(f):
    st = os.stat(f); k = (os.path.abspath(f), st.st_size, int(st.st_mtime))
    g = _G103.get(k)
    if g is None:
        _G103.clear(); g = _G103[k] = G103(f)            # one file at a time (group buffers are large)
    return g

def build_index(f, maxrec=10**9):
    """== g103_get.build_index(f)"""
    if not on():
        import g103_get as GG; return GG.build_index(f, maxrec)
    return dict(g103(f).index)

def get_record(f, info, o=None, dec=None):
    """== g103_get.get_record(f, info, o, dec)"""
    if not on():
        import g103_get as GG; return GG.get_record(f, info, o, dec)
    return g103(f).record(info)

# ---------------- element meshes (rvt_elemmesh) ----------------
CACHE = os.environ.get('RVT_CORE_CACHE', os.path.expanduser('~/.cache/rvt-core'))

def _census_fast(f):
    """== rvt_elemmesh.Doc._census() content: element class per id + WallSweep types (light extraction)"""
    import rvt_entities as E
    x = extract(f, want_all=True, slim=lambda c: c == 'WallSweep')
    cen = {'cls': {}, 'ws': {}}
    for (eid, c), r in x['recs'].items():
        cen['cls'].setdefault(eid, c)
        if c == 'WallSweep':
            try: cen['ws'][eid] = int(E.S_(r['fields'], 'm_wallSweepInfo', 'm_wallSweepType'))
            except Exception: pass
    return cen

def _doc_class():
    import rvt_elemmesh as EM
    class Doc(EM.Doc):
        """rvt_elemmesh.Doc with records served from the Rust-held channel-103 buffers (no per-group re-inflation), census from the fast extractor,
        and symbol meshes memoised (an instance's symbol is tessellated once per (symbol, depth, detail) instead of once per instance)"""
        def __init__(s, f, index_pkl=None):
            import pickle
            s.f = f; s._g = g103(f); s.res = pickle.load(open(index_pkl, 'rb')) if index_pkl else s._g.index; s.o = None; s.dec = None; s._memo = {}
        def _census(s):
            if getattr(s, '_cen', None) is None: s._cen = _census_fast(s.f)
            return s._cen
        def rec(s, eid):
            i = s.res.get(eid)
            return s._g.record(i) if i else None
    return Doc

_DOC = None
def Doc(f, index_pkl=None):
    """drop-in for rvt_elemmesh.Doc(f) (oracle Doc when the fast path is off)"""
    import rvt_elemmesh as EM
    global _DOC
    if not on(): return EM.Doc(f, index_pkl)
    if _DOC is None: _DOC = _doc_class(); _install_memo()
    return _DOC(f, index_pkl)

def _install_memo():
    """inner (symbol) element_mesh calls on a fast Doc are memoised; top-level calls and oracle Docs are untouched"""
    import rvt_elemmesh as EM
    if getattr(EM.element_mesh, '_rvt_fast', False): return
    orig = EM.element_mesh
    def element_mesh(doc, eid, symbol_depth=2, detail=3):
        memo = getattr(doc, '_memo', None)
        if memo is None or symbol_depth >= 2: return orig(doc, eid, symbol_depth, detail)
        k = (int(eid), symbol_depth, detail)
        r = memo.get(k)
        if r is None: r = memo[k] = orig(doc, eid, symbol_depth, detail)
        return r
    element_mesh._rvt_fast = True; element_mesh._orig = orig
    EM.element_mesh = element_mesh

def element_mesh(doc, eid, symbol_depth=2, detail=3):
    """== rvt_elemmesh.element_mesh (Python mesh code on the fast Doc)"""
    import rvt_elemmesh as EM
    return EM.element_mesh(doc, eid, symbol_depth, detail)

def mesh_doc(doc):
    """the Rust element-mesh context of a fast Doc (built once)"""
    md = getattr(doc, '_md', None)
    if md is None:
        cen = doc._census(); md = doc._md = doc._g.rd.mesh_doc(cen['cls'], cen['ws'])
    return md

def element_mesh_rust(doc, eid, symbol_depth=2, detail=3):
    """rvt_elemmesh.element_mesh computed in Rust (tests/diff_mesh.py --impl rust compares it with the reference)"""
    return mesh_doc(doc).element_mesh(int(eid), symbol_depth, detail)

# ---------------- progress file + streaming meshes for the viewer ----------------
def file_key(f):
    import hashlib
    st = os.stat(f); return hashlib.sha1(('%s|%d|%d' % (os.path.abspath(f), st.st_size, int(st.st_mtime))).encode()).hexdigest()[:16]

class Progress:
    """~/.cache/rvt-progress/<file key>.json, rewritten (atomically) at most every 250 ms while work runs and once at every stage change:
    {file, stage, done, total, elements_decoded, meshes_done, bytes_read, eta_s, started, updated, final}"""
    DIR = os.path.expanduser('~/.cache/rvt-progress')
    def __init__(s, f):
        import time, threading
        os.makedirs(s.DIR, exist_ok=True)
        s.path = os.path.join(s.DIR, file_key(f) + '.json'); s.t0 = time.time()
        s.d = {'file': os.path.abspath(f), 'stage': 'start', 'done': 0, 'total': 0, 'elements_decoded': 0, 'meshes_done': 0, 'meshes_total': 0, 'bytes_read': 0, 'eta_s': None, 'started': s.t0, 'final': False}
        s._lock = threading.Lock(); s._wlock = threading.Lock(); s._dirty = True; s._stop = threading.Event(); s._stage_t = s.t0
        s._th = threading.Thread(target=s._loop, daemon=True); s._th.start()
    def set(s, stage=None, **kw):
        import time
        with s._lock:
            if stage is not None and stage != s.d['stage']: s.d['stage'] = stage; s._stage_t = time.time(); s.d['done'] = 0; s.d['total'] = kw.pop('total', 0)
            s.d.update(kw); dn, tot = s.d['done'], s.d['total']; el = time.time() - s._stage_t
            s.d['eta_s'] = round(el / dn * (tot - dn), 1) if dn and tot and dn < tot else (0.0 if tot and dn >= tot else None)
            s._dirty = True
        if stage is not None: s._write()
    def _write(s):
        import json, time, threading
        with s._wlock:
            with s._lock:
                if not s._dirty: return
                d = dict(s.d); d['updated'] = time.time(); s._dirty = False
            tmp = s.path + '.%d.%d.tmp' % (os.getpid(), threading.get_ident())
            try:
                with open(tmp, 'w') as fh: json.dump(d, fh)
                os.replace(tmp, s.path)
            except OSError: pass
    def _loop(s):
        while not s._stop.wait(0.25): s._write()
    def close(s, stage='done'):
        s.set(stage, final=True); s._stop.set(); s._write()

def _mesh_worker(args):
    ids, = args
    out = []
    for e in ids:
        try: m = element_mesh(_W['doc'], e)
        except Exception as ex: m = {'tris': [], 'source': 'none', 'flags': ['error: %r' % ex]}
        if m.get('meta'): m = dict(m, meta=[(k, None, uv) for k, _ev, uv in m['meta']])        # surface evaluators are closures (not picklable): kind and uv kept
        out.append((e, m))
    return out
_W = {}

def iter_meshes(f, ids, workers=None, chunk=24, progress=None, doc=None):
    """yield (element id, element_mesh result) as soon as each is ready (order not guaranteed). Worker processes (fork, <= 6) share the decoded file.
    Results are element_mesh dicts except that meta entries carry no evaluator: (kind, None, uvs). progress: a Progress (meshes_done/done/total updated)."""
    import multiprocessing as mp
    doc = doc or Doc(f); ids = list(ids)
    nw = workers or _auto_workers()
    if progress: progress.set('meshes', total=len(ids))
    n = 0
    if nw <= 1 or len(ids) < 2 * chunk:
        _W['doc'] = doc
        for e in ids:
            for r in _mesh_worker(([e],)):
                n += 1
                if progress: progress.set(done=n, meshes_done=n)
                yield r
        return
    _W['doc'] = doc
    doc._census()                                        # computed once in the parent, inherited by the workers
    parts = [(ids[k:k + chunk],) for k in range(0, len(ids), chunk)]
    with mp.get_context('fork').Pool(nw) as pool:
        for res in pool.imap_unordered(_mesh_worker, parts):
            for r in res:
                n += 1
                if progress: progress.set(done=n, meshes_done=n)
                yield r

def _auto_workers():
    """<= 6, bounded by the cgroup memory limit (each forked mesh worker grows to ~0.6 GB on large files)"""
    n = min(6, os.cpu_count() or 1)
    try:
        cg = open('/proc/self/cgroup').read().strip().split('::')[-1]
        lim = open('/sys/fs/cgroup%s/memory.max' % cg).read().strip()
        if lim != 'max': n = max(1, min(n, int((int(lim) / 2**30 - 1.5) / 0.6)))
    except Exception: pass
    return int(os.environ.get('RVT_WORKERS', n))

# ---------------- parameter layer (rvt_params / rvt_map2 / rvt_ifc) ----------------
def ifc_stored(recs):
    """== rvt_ifc.stored(recs)"""
    if not on():
        import rvt_ifc; return rvt_ifc.stored(recs)
    return _rc().ifc_stored(recs)

def Src(last, hdr=None):
    """== rvt_map2.Src(last, hdr) (flat / resolve in Rust)"""
    import rvt_decode as D, rvt_map2 as M
    if not on(): return M.Src(last, hdr)
    return _rc().FlatSrc(last, hdr or {}, D.EID)

def is_fast_src(src):
    RC = _rc(); return RC is not None and isinstance(src, RC.FlatSrc)

def resolve(src, rid, via, path):
    """== rvt_map2.resolve(src, rid, via, path)"""
    if is_fast_src(src): return src.resolve(rid, via, path)
    import rvt_map2 as M; return M.resolve(src, rid, via, path)

def _params_impl(path, x):
    """rvt_params.params(path) with x = the extraction: the reference loop line by line, MAPPING loop / resolve / IfcGUID in Rust (tests/diff_params.py)"""
    import rvt_params as P, rvt_map2 as M, rvt_famparams as F, rvt_geom as Gm, rvt_uid, ifcguid, rvt_entities as E, rvt_decode as D, rvt_scan as S
    RC = _rc()
    recs = {}
    for (eid, c), r in x['recs'].items(): recs.setdefault(eid, r)
    hdr = {eid: {'fields': f} for eid, f in x['eh'].items()}
    src = RC.FlatSrc(recs, hdr, D.EID); src.set_mapping(P.MAPPING, P.PNAMES, sorted(M.SENT)); cap = F.captions(x['recs'])
    lv = {}
    for (eid, c), r in x['recs'].items():
        if c == 'Level':
            o = E.S_(r['fields'], 'm_pSurface', 'm_origin')
            if o: lv[eid] = o[2]
    uid = rvt_uid.unique_ids(path)
    st = RC.ifc_stored({e: [r] for e, r in recs.items()})
    own = S.owners(path); out = {}
    UNIV = not os.environ.get('RVT_NOUNIV')
    fu = RC.ifc_from_unique
    def _sg(v):
        v = int(v); return v - (1 << 64) if v >= 1 << 63 else (v - (1 << 32) if 1 << 31 <= v < 1 << 32 else v)
    for eid, r in recs.items():
        c = r['class']; f = r['fields']; d = src.apply(eid, c)
        sid = F.S_(f, 'm_pInstanceInfo', 'm_symbolId'); sym = recs.get(int(sid), {}).get('fields') if sid is not None else None
        for k, (v0, vi, vs, ve) in F.named_values(f, cap, sym).items():
            if isinstance(v0, (int, float)): d.setdefault(k, float(v0))
        if c in ('RbsPipeCurve', 'RbsDuctCurve', 'RbsConduitCurve', 'CableTray'): d.update(Gm.mep_curve(f, c, lv))
        elif c == 'FamilyInstance': d.update(Gm.fam_instance(f, lv))
        elif c == 'SWall' or c.endswith('CenterLine'):
            cu = Gm.primary_curve(f)
            if cu is not None and Gm.curve_length(cu) is not None: d['Length'] = Gm.curve_length(cu)
        hf = x['eh'].get(eid) or {}
        if UNIV: cid = hf.get('m_categroryId'); d['Category|ElementId'] = _sg(cid) if cid is not None else -1
        dop = hf.get('m_designOptionId', f.get('m_designOptionId')); dop = _sg(dop) if dop is not None else -1
        if UNIV: d['Design Option|ElementId'] = dop
        if UNIV: d['Design Option|String'] = 'Main Model' if dop == -1 else (recs.get(dop, {}).get('fields', {}).get('m_name') or 'Main Model')
        if eid in uid:
            d['UniqueId'] = uid[eid]; d['IfcGUID'] = st.get(eid) or fu(uid[eid])
            o = own.get(eid)
            if not st.get(eid) and c.endswith('CenterLine') and o is not None and o in uid: d['IfcGUID'] = st.get(o) or fu(uid[o])
        out[eid] = {'class': c, 'params': d}
    return out

_PLOCK = __import__('threading').Lock()
def params(path, x=None):
    """== rvt_params.params(path): the reference code, with its three hot kernels (Src/resolve/rvt_ifc.stored) running in Rust"""
    import rvt_params as P, rvt_map2 as M, rvt_ifc
    if not on(): return P.params(path)
    if not os.environ.get('RVT_CORE_PARAMS_PATCH'):
        import gc
        if x is None: x = extract(path, want_all=True)
        gc_was = gc.isenabled(); gc.disable()
        try: return _params_impl(path, x)
        finally:
            if gc_was: gc.enable()
    with _PLOCK:
        import rvt_xcache
        if x is None and on():
            x = extract(path, want_all=True)              # the extraction rvt_xcache.get_x would return (fast path + binary cache), without its pickle
        if x is not None:
            xx = {k: v for k, v in x.items() if k != 'sch'}
            saved_x = rvt_xcache.get_x; rvt_xcache.get_x = lambda p, xx=xx: xx
        saved = (M.Src, M.resolve, rvt_ifc.stored)
        FS = _rc().FlatSrc; orig = M.resolve
        def resolve_bound(src, rid, via, path, FS=FS, orig=orig):
            return src.resolve(rid, via, path) if type(src) is FS else orig(src, rid, via, path)
        M.Src, M.resolve, rvt_ifc.stored = Src, resolve_bound, _rc().ifc_stored
        import gc
        gc_was = gc.isenabled(); gc.disable()
        try: return P.params(path)
        finally:
            M.Src, M.resolve, rvt_ifc.stored = saved
            if x is not None: rvt_xcache.get_x = saved_x
            if gc_was: gc.enable()

# ---------------- progressive open for the viewer ----------------
QUICK_FULL = ('Level', 'Grid', 'WallSweep')

def _sgn(v, W):
    v = int(v); b = 8 * W
    return v - (1 << b) if v >= 1 << (b - 1) else v

def _mod_hash(names):
    import hashlib
    h = hashlib.sha1(reader_version().encode())
    for n in names:
        fn = os.path.join(READER, n)
        if os.path.exists(fn): h.update(open(fn, 'rb').read())
    return h.hexdigest()[:12]

def params_version(): return _mod_hash(['rvt_params.py', 'rvt_map2.py', 'rvt_famparams.py', 'rvt_geom.py', 'rvt_uid.py', 'ifcguid.py', 'rvt_ifc.py', 'map3_train.json', 'param_names.json', 'rvt_entities.py'])
def mesh_version(): return _mod_hash(['rvt_mesh.py', 'rvt_elemmesh.py', 'rvt_curves.py', 'g103_get.py'])

def _cache_on(): return not os.environ.get('RVT_NOCACHE') and not os.environ.get('RVT_CORE_NOCACHE')

def _census_of(x):
    import rvt_entities as E
    cen = {'cls': {}, 'ws': {}}
    for (eid, c), r in x['recs'].items():
        cen['cls'].setdefault(eid, c)
        if c == 'WallSweep':
            try: cen['ws'][eid] = int(E.S_(r['fields'], 'm_wallSweepInfo', 'm_wallSweepType'))
            except Exception: pass
    return cen

class MeshStream:
    """background computation of element meshes (Rust, GIL released while computing); iterate to get lists of (id, mesh) as chunks finish"""
    def __init__(s, model, ids, chunk, compact):
        import threading, queue
        s.q = queue.Queue(); s.model = model; s.ids = [int(i) for i in ids]; s.chunk = chunk; s.compact = compact; s.t_first = s.t_done = None
        s.th = threading.Thread(target=s._run, daemon=True); s.th.start()
    def _run(s):
        m = s.model
        try:
            cache = m._mesh_cache() if s.compact else {}
            todo = [e for e in s.ids if e not in cache]
            hit = [(e, cache[e]) for e in s.ids if e in cache]
            import time
            n = 0; m._set(meshes_total=len(s.ids), meshes_done=0)
            for k in range(0, len(hit), 4096):
                part = hit[k:k + 4096]; n += len(part); m._set(meshes_done=n); s._put(part)
            if todo:
                md = mesh_doc(m.doc()); new = {}
                for k in range(0, len(todo), s.chunk):
                    part = todo[k:k + s.chunk]; res = md.element_meshes(part, compact=s.compact)
                    n += len(part); m._set(meshes_done=n)
                    out = list(zip(part, res)); new.update(out); s._put(out)
                if s.compact: m._mesh_cache_add(new)
        except Exception as ex:
            s.q.put(ex)
        s.t_done = time.time(); s.q.put(None)
    def _put(s, part):
        import time
        if s.t_first is None: s.t_first = time.time()
        s.q.put(part)
    def __iter__(s):
        while True:
            x = s.q.get()
            if x is None: return
            if isinstance(x, Exception): raise x
            yield x

class Model:
    """Progressive open of a Revit file; every stage is reported in the progress file (Progress, ~/.cache/rvt-progress/<file_key>.json):
       quick()  -> {'elements': {id: {'class', 'category', 'bbox'}}, 'levels', 'grids', 'W'}  (records decoded, only Level/Grid/WallSweep kept in full)
       start_meshes(ids, chunk=128, compact=True) -> MeshStream (iterate: lists of (id, mesh)); runs while full() works
       full()   -> (x, params) == (rvt_entities.extract(path, want_all=True), rvt_params.params(path))
       close()  -> final progress state ('done')
    Results are cached in ~/.cache/rvt-core (binary, keyed by file + reader/mesh/params code versions + schema)."""
    def __init__(s, path, progress=True):
        s.path = os.path.abspath(path); s.key = file_key(path); s.prog = Progress(path) if progress else None
        s._x = None; s._doc = None; s._cen = None; s._mc = None; s._finish = None
        import threading; s._lock = threading.Lock()
    def _set(s, stage=None, **kw):
        if s.prog: s.prog.set(stage, **kw)
    def quick(s):
        """element table + levels + grids; decodes the file once (the same extraction full() returns)"""
        import rvt_decode as D
        s._set('quick', total=1)
        fn = os.path.join(CACHE, '%s-q%s.pk' % (s.key, params_version()))
        if _cache_on() and os.path.exists(fn) and s._x is None:
            try:
                q = _rc().unpack(fn, D.EID)
                q['elements'] = {int(k): v for k, v in q['elements'].items()}
                s._set(done=1, elements_decoded=len(q['elements'])); return q
            except Exception: pass
        if s._x is not None: x = s._x
        else: x, s._finish = extract_two_stage(s.path, slim=lambda c: c in QUICK_FULL)
        s._cen = _census_of(x)
        W = x['W']; els = {}
        for (eid, c), r in x['recs'].items():
            if eid in els: continue
            h = x['eh'].get(eid); bb = None; cat = None
            if h:
                cid = h.get('m_categroryId'); cat = _sgn(cid, W) if cid is not None else None
                pb = h.get('m_pBBox'); pb = pb.get('obj', pb) if isinstance(pb, dict) else pb
                mm = pb.get('m_minmax') if isinstance(pb, dict) else None
                if mm and len(mm) == 2: bb = [list(map(float, mm[0])), list(map(float, mm[1]))]
            els[eid] = {'class': c, 'category': cat, 'bbox': bb}
        levels = []; grids = []
        for (eid, c), r in x['recs'].items():
            f = r['fields']
            if c == 'Level':
                o = f.get('m_pSurface'); o = o.get('obj', o) if isinstance(o, dict) else o
                z = (o.get('m_origin') or [None] * 3)[2] if isinstance(o, dict) else None
                levels.append({'id': eid, 'name': f.get('m_text', ''), 'elevation_ft': z})
            elif c == 'Grid': grids.append({'id': eid, 'name': f.get('m_text', '')})
        levels.sort(key=lambda l: (l['elevation_ft'] is None, l['elevation_ft']))
        q = {'elements': els, 'levels': levels, 'grids': grids, 'W': W}
        if _cache_on():
            try: os.makedirs(CACHE, exist_ok=True); tmp = fn + '.%d.tmp' % os.getpid(); open(tmp, 'wb').write(_rc().pack(q, D.EID)); os.replace(tmp, fn)
            except Exception: pass
        s._set(done=1, elements_decoded=len(els))
        return q
    def full(s):
        import rvt_decode as D
        s._set('extract', total=1)
        if s._x is None:
            fin = getattr(s, '_finish', None); s._finish = None
            s._x = fin() if fin else extract(s.path, want_all=True)
        x = s._x
        s._set(done=1, elements_decoded=len(x['recs']))
        s._set('params', total=1)
        fn = os.path.join(CACHE, '%s-p%s.pk' % (s.key, params_version()))
        p = None
        if _cache_on() and os.path.exists(fn):
            try: p = _rc().unpack(fn, D.EID)
            except Exception: p = None
        if p is None:
            p = params(s.path, x)
            if _cache_on():
                try: os.makedirs(CACHE, exist_ok=True); tmp = fn + '.%d.tmp' % os.getpid(); open(tmp, 'wb').write(_rc().pack(p, D.EID)); os.replace(tmp, fn)
                except Exception: pass
        s._set(done=1)
        return x, p
    def doc(s):
        with s._lock:
            if s._doc is None:
                d = Doc(s.path)
                if s._cen is None and s._x is not None: s._cen = _census_of(s._x)
                cen = s._cen
                if cen is not None: d._cen = cen
                s._doc = d
            return s._doc
    def _mesh_fn(s): return os.path.join(CACHE, '%s-m%s.pk' % (s.key, mesh_version()))
    def _mesh_cache(s):
        import rvt_decode as D
        if s._mc is None:
            s._mc = {}
            if _cache_on() and os.path.exists(s._mesh_fn()):
                try: s._mc = _rc().unpack(s._mesh_fn(), D.EID)
                except Exception: s._mc = {}
        return s._mc
    def _mesh_cache_add(s, new):
        import rvt_decode as D
        if not _cache_on() or not new: return
        mc = s._mesh_cache(); mc.update(new)
        try: os.makedirs(CACHE, exist_ok=True); fn = s._mesh_fn(); tmp = fn + '.%d.tmp' % os.getpid(); open(tmp, 'wb').write(_rc().pack(mc, D.EID)); os.replace(tmp, fn)
        except Exception: pass
    def start_meshes(s, ids, chunk=128, compact=True):
        return MeshStream(s, ids, chunk, compact)
    def meshes(s, ids, chunk=128, compact=True):
        """synchronous form of start_meshes"""
        yield from s.start_meshes(ids, chunk, compact)
    def mesh_ids(s):
        """elements the viewer meshes: channel-103 record with a bbox, class not a symbol/type (cached with the meshes)"""
        import rvt_decode as D
        fn = os.path.join(CACHE, '%s-i%s.pk' % (s.key, mesh_version()))
        if _cache_on() and os.path.exists(fn):
            try: return list(_rc().unpack(fn, D.EID))
            except Exception: pass
        d = s.doc(); cen = d._census()
        ids = [e for e, i in d.res.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol')]
        if _cache_on():
            try: os.makedirs(CACHE, exist_ok=True); tmp = fn + '.%d.tmp' % os.getpid(); open(tmp, 'wb').write(_rc().pack(ids, D.EID)); os.replace(tmp, fn)
            except Exception: pass
        return ids
    def close(s):
        if s.prog: s.prog.close()
