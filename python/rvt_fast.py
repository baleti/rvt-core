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
    import rvt_scan as S, rvt_decode as D, rvt2text as T, rvt_inventory as I, rvt_es as ES
    o, cls, ids = S.load(path); W = S.id_size(cls); sch = D.Schema(cls)
    EH = sch.by.get('ElementHeader', -999) + 12; names = {i + 12: c['name'] for i, c in enumerate(cls)}; et = I.element_tags(cls)
    wanted = {t for t, n in names.items() if t in et and (n in E.WANT or n.startswith(E.WANT_PREFIX)) and n != 'DBView'}
    if extra or want_all: wanted = {t for t, n in names.items() if t in et and (want_all or n in extra)}
    try: es = es_load(path)[0]
    except Exception: es = {}
    rd = make_recdecoder(sch, W, es); rd.set_ids(sorted(ids))
    slim_tags = sorted(t for t in wanted if slim is not None and not slim(names[t])) if slim is not None else None
    oid = sorted(only_ids) if only_ids is not None else None
    wl = sorted(wanted)
    recs = {}; eh = {}
    import gc
    gc_was = gc.isenabled(); gc.disable()                   # millions of new container objects: cyclic GC passes would cost more than the decoding
    try:
        _partitions(o, rd, wl, EH, oid, slim_tags, recs, eh, T)
    finally:
        if gc_was: gc.enable()
    return dict(eh=eh, recs=recs, W=W, sch=sch)

def _partitions(o, rd, wl, EH, oid, slim_tags, recs, eh, T):
    for pn in sorted(x for x in ['/'.join(e) for e in o.listdir(streams=True)] if x.startswith('Partitions/')):
        raw = o.openstream(pn).read()
        res = rd.extract_stored(raw, wl, EH, oid, slim_tags)
        if res is None: res = rd.extract_chunk(T.joined(raw), wl, EH, oid, slim_tags)
        del raw
        for k, r in res[0]: recs.setdefault(k, r)
        for k, v in res[1]: eh.setdefault(k, v)

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
        s.d = {'file': os.path.abspath(f), 'stage': 'start', 'done': 0, 'total': 0, 'elements_decoded': 0, 'meshes_done': 0, 'bytes_read': 0, 'eta_s': None, 'started': s.t0, 'final': False}
        s._lock = threading.Lock(); s._dirty = True; s._stop = threading.Event(); s._stage_t = s.t0
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
        import json, time
        with s._lock:
            if not s._dirty: return
            d = dict(s.d); d['updated'] = time.time(); s._dirty = False
        tmp = s.path + '.%d.tmp' % os.getpid()
        with open(tmp, 'w') as fh: json.dump(d, fh)
        os.replace(tmp, s.path)
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
