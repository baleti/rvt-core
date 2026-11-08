#!/usr/bin/env python3
"""RVT read coverage, one file -> one JSON line (aggregate only: no names, no element content).
usage: rvtreadcov.py FILE [--records]   (run via tools/rvtreadcov_run.sh for a corpus, in a memory scope)
Fields: sha8, size, year (BasicFileInfo), streams {name: [stored_bytes, status]} where status is
  'decoded' (a semantic decoder exists and ran), 'opaque' (inflates fine, no semantic decoder), 'error:<msg>',
and with --records the per-class record decode counts from rvt-research cov_year.py (ok/total)."""
import sys, os, json, hashlib, re, subprocess, time
RR = os.path.expanduser('~/rvt-research'); sys.path.insert(0, RR)
DECODED = re.compile(r'^(Formats/Latest|Global/ElemTable|Partitions/\d+|BasicFileInfo|Global/PartitionTable|Global/Latest|Global/History|Global/ContentDocuments)$')
def truncated(o, p):
    """olefile pads a truncated file with zeros instead of failing: the sectors the directory says exist must fit in the file"""
    ss = o.sectorsize; need = 1 + -(-o.root.size // ss)
    for e in o.direntries:
        if e is not None and e.entry_type == 2 and e.size >= o.minisectorcutoff: need += -(-e.size // ss)
    return os.path.getsize(p) < need * ss
def check_members(raw):
    """every gzip member of a paged stream must inflate and match its CRC32/ISIZE trailer (rvt2text.segments skips bad members silently)"""
    import zlib, struct
    from rvtfmt import strip_pages, GZ
    data = strip_pages(raw); pos = n = 0
    while True:
        i = data.find(GZ, pos)
        if i < 0: return n
        d = zlib.decompressobj(-15)
        try: out = d.decompress(memoryview(data)[i + 10:])
        except zlib.error as x: raise ValueError('member %d at %d: zlib %s' % (n, i, x))
        tr = d.unused_data[:8]
        if len(tr) == 8 and (struct.unpack('<II', tr) != (zlib.crc32(out) & 0xffffffff, len(out) & 0xffffffff)): raise ValueError('member %d at %d: CRC/ISIZE mismatch' % (n, i))
        n += 1; pos = len(data) - len(d.unused_data) + (8 if len(tr) == 8 else 0)
def stream_inventory(p):
    import olefile
    from rvtfmt import inflate_stream
    out = {}
    o = olefile.OleFileIO(p)
    if truncated(o, p): raise ValueError('file truncated: directory needs more sectors than the file holds')
    for e in o.listdir(streams=True, storages=False):
        n = '/'.join(e)
        try:
            raw = o.openstream(n).read()
        except Exception as x:
            out[n] = [0, 'error:read ' + type(x).__name__]; continue
        try:
            if n.startswith('Partitions/'):
                import rvt2text as T
                segs = list(T.segments(raw))
                if not segs: raise ValueError('no gzip segment')
                check_members(raw)
                st = 'decoded'
            elif n == 'BasicFileInfo':
                st = 'decoded'
            elif n == 'PartAtom' and raw.lstrip(b'\xef\xbb\xbf')[:5] == b'<?xml':
                import xml.dom.minidom as M, xml.parsers.expat as X
                try: M.parseString(raw.lstrip(b'\xef\xbb\xbf')); st = 'opaque'  # plain Atom XML in .rfa, well-formed
                except X.ExpatError:
                    # Revit itself writes an unescaped '&' from the family name into the title: upstream quirk, not damage. Accept only if escaping bare '&' makes it well-formed
                    fixed = re.sub(rb'&(?!(?:amp|lt|gt|quot|apos|#\d+|#x[0-9a-fA-F]+);)', b'&amp;', raw.lstrip(b'\xef\xbb\xbf'))
                    M.parseString(fixed); st = 'opaque'
            elif raw[:2] == b'PK':
                import zipfile, io
                bad = zipfile.ZipFile(io.BytesIO(raw)).testzip()
                st = 'opaque' if bad is None else 'error:zip member ' + str(bad)[:20]
            elif n.startswith('\x05') or n.startswith('TransmissionData') or n.startswith('RevitPreview') or n == 'Contents':
                st = 'opaque'
            else:
                inflate_stream(raw); check_members(raw); st = 'decoded' if DECODED.match(n) else 'opaque'
        except Exception as x:
            st = 'error:' + type(x).__name__ + ' ' + str(x)[:40]
        k = 'Partitions/N' if n.startswith('Partitions/') else n
        prev = out.get(k)
        out[k] = [len(raw) + (prev[0] if prev else 0), st if not prev or prev[1] == 'decoded' else prev[1]]
    return out
def year_of(p):
    import olefile
    try:
        d = olefile.OleFileIO(p).openstream('BasicFileInfo').read().decode('utf-16le', 'ignore')
        m = re.search(r'(?:Autodesk Revit |Format: |Revit Build[^\n]*?)(20\d\d)', d) or re.search(r'(20[12]\d)', d)
        return int(m.group(1)) if m else None
    except Exception:
        return None
if __name__ == '__main__':
    p = sys.argv[1]; t0 = time.time()
    r = {'sha8': hashlib.sha256(open(p, 'rb').read()).hexdigest()[:8], 'size': os.path.getsize(p), 'ext': os.path.splitext(p)[1].lower()}
    try:
        r['year'] = year_of(p); r['streams'] = stream_inventory(p); r['open'] = 'ok'
    except Exception as x:
        r['open'] = 'error:' + type(x).__name__ + ' ' + str(x)[:60]
    if '--records' in sys.argv and r['open'] == 'ok':
        q = subprocess.run([sys.executable, RR + '/cov_year.py', p], capture_output=True, text=True, timeout=3600)
        try:
            c = json.loads(q.stdout.strip().splitlines()[-1])
            r['rec'] = {k: c[k] for k in ('elements', 'el_complete', 'el_exact', 'el_found', 'records', 'complete', 'exact_fit', 'worst_classes', 'top_err', 'unresolved')}
        except Exception:
            r['rec'] = 'error:' + (q.stderr.strip().splitlines() or ['no output'])[-1][:100]
    r['secs'] = round(time.time() - t0)
    print(json.dumps(r))
