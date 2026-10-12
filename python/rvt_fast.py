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

def make_recdecoder(sch, W, es=None):
    import rvt_decode as D
    RC = _rc()
    rd = RC.RecDecoder(sch.c, W, list(D.Dec.STOP), D.STRCAP, D.EID, D.Desync)
    if es: rd.set_es(es)
    return rd

def find_schemas(d, sch, W, rd=None):
    """== rvt_es.find_schemas(d, sch, W)"""
    RC = _rc()
    if RC is None:
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
    RC = _rc()
    if RC is None or os.environ.get('RVT_OLDCAND') or not _decoder_default_config():
        return E.extract(path, extra=extra, only_ids=only_ids, want_all=want_all, slim=slim)
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
    RC = _rc()
    if RC is None or not _decoder_default_config():
        import g103_get as GG; return GG.build_index(f, maxrec)
    return dict(g103(f).index)

def get_record(f, info, o=None, dec=None):
    """== g103_get.get_record(f, info, o, dec)"""
    RC = _rc()
    if RC is None or not _decoder_default_config():
        import g103_get as GG; return GG.get_record(f, info, o, dec)
    return g103(f).record(info)
