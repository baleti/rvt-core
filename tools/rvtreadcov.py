#!/usr/bin/env python3
"""RVT read coverage, one file -> one JSON line (aggregate only: no names, no element content).
usage: rvtreadcov.py FILE [--records]   (run via tools/rvtreadcov_run.sh for a corpus, in a memory scope)
Fields: sha8, size, year (BasicFileInfo), streams {name: [stored_bytes, status]} where status is
  'decoded' (a semantic decoder exists and ran), 'opaque' (inflates fine, no semantic decoder), 'error:<msg>',
and with --records the per-class record decode counts from rvt-research cov_year.py (ok/total)."""
import sys, os, json, hashlib, re, subprocess, time
RR = os.path.expanduser('~/rvt-research'); sys.path.insert(0, RR); sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
DECODED = re.compile(r'^(Formats/Latest|Global/ElemTable|Partitions/\d+|BasicFileInfo|Global/PartitionTable|Global/Latest|Global/History|Global/ContentDocuments)$')
CORE = ('BasicFileInfo', 'Formats/Latest', 'Global/Latest', 'Global/ElemTable', 'Global/PartitionTable', 'Global/History', 'Partitions/N', 'ProjectInformation', 'TransmissionData', 'Contents', 'Global/ContentDocuments', 'Global/DocumentIncrementTable')
def truncated(o, p):
    """olefile pads a truncated file with zeros instead of failing: the sectors the directory says exist must fit in the file"""
    ss = o.sectorsize; need = 1 + -(-o.root.size // ss)
    for e in o.direntries:
        if e is not None and e.entry_type == 2 and e.size >= o.minisectorcutoff: need += -(-e.size // ss)
    return os.path.getsize(p) < need * ss
def minifat_header_bad(o):
    """CFB header mini-FAT fields (offsets 60/64) vs the FAT: Revit 2024 refuses a start sector whose chain is longer than the declared count (c5x header flips 60) and a count larger than the FAT (66); a count above the real chain length alone opens (64), so it is not flagged. Returns a message or None."""
    n = o.num_mini_fat_sectors
    if n == 0 or o.first_mini_fat_sector >= 0xfffffffa: return None
    if n > len(o.fat): return 'mini-FAT sector count %d exceeds the FAT (%d)' % (n, len(o.fat))
    s = o.first_mini_fat_sector; seen = set()
    while s < 0xfffffffa and s not in seen and s < len(o.fat): seen.add(s); s = o.fat[s]
    if len(seen) > n: return 'mini-FAT chain has %d sectors but the header declares %d' % (len(seen), n)
    return None
def dirtree_bad(o):
    """CFB directory tree from the raw directory sectors: every in-use entry must be reachable exactly once from the root via child/left/right, no cycles (a flipped sibling/child pointer silently hides or duplicates a stream; olefile only loads reachable entries so it cannot see this)"""
    import struct
    ss = o.sectorsize; fp = o.fp; ents = []; s = o.first_dir_sector; seen_s = set()
    while s < 0xfffffffa and s not in seen_s and s < len(o.fat):
        seen_s.add(s); fp.seek((s + 1) * ss); b = fp.read(ss)
        for k in range(len(b) // 128):
            e = b[k * 128:(k + 1) * 128]
            if len(e) == 128: ents.append((e[66], struct.unpack_from('<III', e, 68)))    # type, (left, right, child)
        s = o.fat[s]
    seen = set(); st = [0] if ents else []
    while st:
        i = st.pop()
        if i >= len(ents): return 'directory pointer %d beyond the %d entries' % (i, len(ents))
        if i in seen: return 'directory entry %d reached twice (cycle or shared sibling)' % i
        seen.add(i); st += [x for x in ents[i][1] if x != 0xffffffff]
    used = {i for i, (t, _) in enumerate(ents) if t != 0}
    if used - seen: return 'directory entries %s in use but unreachable from the root' % sorted(used - seen)[:5]
    return None
def elemtable_dups(a):
    """ids repeated in a decompressed Global/ElemTable (header 6 bytes, count at 2, rows of 28 or 40 bytes, id first); None if layout unrecognised"""
    import struct
    if len(a) < 6: return None
    n = struct.unpack_from('<I', a, 2)[0]
    RW, W = next(((rw, w) for rw, w in ((28, 4), (40, 8)) if 0 <= len(a) - 6 - rw * n < 64), (None, None))   # no loose 28-byte fallback: 2024-2026 RFA ElemTables (40-byte rows + 600 B tail) read as 1.5k bogus duplicates and genuine Autodesk samples were refused
    if RW is None: return None
    ids = [int.from_bytes(a[6 + RW * q:6 + RW * q + W], 'little') for q in range(n)]
    return len(ids) - len(set(ids))
def check_members(raw):
    """every gzip member of a paged stream must inflate and match its CRC32/ISIZE trailer (rvt2text.segments skips bad members silently)"""
    import zlib, struct
    from rvtfmt import strip_pages, GZ
    data = strip_pages(raw); pos = n = prev_end = prev_i = 0; tag = None
    while True:
        i = data.find(GZ, pos)
        if i < 0: return n
        d = zlib.decompressobj(-15)
        try: out = d.decompress(memoryview(data)[i + 10:])
        except zlib.error as x: raise ValueError('member %d at %d: zlib %s' % (n, i, x))
        if not d.eof: raise ValueError('member %d at %d: deflate stream never terminates (chain damaged)' % (n, i))   # decompressobj returns without error on a cut-off stream; the trailer check below was skipped
        tr = d.unused_data[:8]
        if len(tr) < 8: raise ValueError('member %d at %d: gzip trailer cut short (%d of 8 bytes; stream size or chain damaged)' % (n, i, len(tr)))
        if struct.unpack('<II', tr) != (zlib.crc32(out) & 0xffffffff, len(out) & 0xffffffff): raise ValueError('member %d at %d: CRC/ISIZE mismatch' % (n, i))
        end = len(data) - len(d.unused_data) + 8
        if n and i - prev_end >= 16:
            # the record between two members ends with [.., u32 = this member's gzip length + 8, .., 101, 0]: 100% of ~12000 gaps in 12 files 2017-2026 (a flip there silently re-points the member chain)
            g = data[prev_end:i]
            if struct.unpack_from('<I', g, 2)[0] != (prev_end - prev_i) + 8: raise ValueError('member %d at %d: inter-member record does not carry the previous member length (chain record damaged)' % (n, i))   # 100% of ~5000 sampled gaps, 2017-2026
            if tag is None: tag = g[:2]
            elif g[:2] != tag: raise ValueError('member %d at %d: inter-member record tag differs from the stream\'s other records' % (n, i))   # constant within a stream in all sampled files
            if struct.unpack_from('<I', g, len(g) - 16)[0] != (end - i) + 8: raise ValueError('member %d at %d: inter-member record length field does not match the member (chain record damaged)' % (n, i))
        prev_i = i; prev_end = end; n += 1; pos = end
def member_diff(a, b):
    """compare two paged streams member by member: (compressed_differ, plain_differ, count). Untouched members of a saved file must be compressed-byte identical (Revit's deflate cannot be re-created by zlib: 0/1579 members)"""
    import zlib
    from rvtfmt import strip_pages, GZ
    def mem(raw):
        d = strip_pages(raw); pos = 0; L = []
        while True:
            i = d.find(GZ, pos)
            if i < 0: return L
            x = zlib.decompressobj(-15); o = x.decompress(d[i + 10:]); cl = len(d) - i - 10 - len(x.unused_data)
            L.append((d[i + 10:i + 10 + cl], o)); pos = i + 10 + cl + 8
    ma, mb = mem(a), mem(b)
    if len(ma) != len(mb): raise ValueError('member count %d != %d' % (len(ma), len(mb)))
    return sum(x[0] != y[0] for x, y in zip(ma, mb)), sum(x[1] != y[1] for x, y in zip(ma, mb)), len(ma)
def history_bad(a):
    """Global/History plaintext: u32 count at offset 102 then that many strictly ascending u32 ids (60/60 corpus files 2017-2026, 399/399 random local rvt/rte/rfa)"""
    import struct
    if len(a) < 106: return None
    c = struct.unpack_from('<I', a, 102)[0]
    if 106 + 4 * c > len(a): return 'History id list longer than the stream'
    i = struct.unpack_from('<%dI' % c, a, 106)
    if any(x >= y for x, y in zip(i, i[1:])): return 'History id list not ascending'
    return None
def png_bad(raw):
    """RevitPreview4.0 embeds a PNG: every chunk CRC must match and IEND must be reached (430 of 430 genuine local files pass; 2 files carry no PNG and are not flagged)"""
    import zlib, struct
    i = raw.find(b'\x89PNG\r\n\x1a\n')
    if i < 0: return None
    p = i + 8
    while p + 12 <= len(raw):
        n, t = struct.unpack_from('>I4s', raw, p)
        if p + 12 + n > len(raw): return 'PNG chunk %s runs past the stream' % t
        if zlib.crc32(raw[p + 4:p + 8 + n]) != struct.unpack_from('>I', raw, p + 8 + n)[0]: return 'PNG chunk %s CRC mismatch' % t
        p += 12 + n
        if t == b'IEND': return ''
    return 'PNG has no IEND'
def stream_inventory(p):
    import olefile
    from rvtfmt import inflate_stream
    out = {}
    import cfbhdr
    _hv = cfbhdr.violations(open(p, 'rb').read(512))     # strict [MS-CFB] header rules: 0 violations in 12558 local rvt/rte/rfa; stricter than Revit 2024, which still opens flips at 28, 29, 36, 40
    if _hv: raise ValueError('CFB header violates ' + ','.join(_hv))
    o = olefile.OleFileIO(p)
    if truncated(o, p): raise ValueError('file truncated: directory needs more sectors than the file holds')
    _t = dirtree_bad(o)
    if _t: raise ValueError('CFB directory: ' + _t)
    _m = minifat_header_bad(o)
    if _m: raise ValueError('CFB header: ' + _m)
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
                _t = ''.join(raw[a:].decode('utf-16le', 'ignore') for a in (0, 1))     # UTF-16 text starts at an odd or even byte
                _m = [l for l in ('Worksharing:', 'Central Model Path:', 'Build:', 'Username:', 'Open Workset Default:') if l not in _t]   # present in 1115 of 1115 local files, 2017-2026
                _y = re.search(r'Revit[^\n]*?(20\d\d)', _t)
                if _m and not (_y and int(_y.group(1)) < 2016): raise ValueError('BasicFileInfo lacks label %s (mini-stream damage?)' % _m[0])   # pre-2016 files (outside the 1115 local 2016+ files the rule was calibrated on) legitimately lack some labels
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
            elif n == 'Contents':
                # magic 62 19 22 05 at offset 0 in 78 of 78 corpus files (2017-2026)
                if raw[:4] != bytes.fromhex('62192205'): raise ValueError('Contents lacks magic 62192205')
                st = 'opaque'
            elif n == 'TransmissionData':
                # u32 char count + UTF-16LE XML; 78 of 78 corpus files satisfy len == 4 + 2*count and start with <?xml
                import struct as _s
                if len(raw) < 6 or len(raw) != 4 + 2 * _s.unpack_from('<I', raw)[0]: raise ValueError('TransmissionData length prefix does not match the stream size')
                if not raw[4:].decode('utf-16-le').lstrip().startswith('<?xml'): raise ValueError('TransmissionData is not XML')
                import xml.dom.minidom as _M
                try: _M.parseString(raw[4:].decode('utf-16-le').encode('utf-8').replace(b'encoding="utf-16"', b'encoding="utf-8"'))   # well-formed in 396 of 396 genuine local files
                except Exception as _e: raise ValueError('TransmissionData XML not well-formed: ' + str(_e)[:30])
                st = 'opaque'
            elif n.startswith('RevitPreview') and png_bad(raw):
                raise ValueError(png_bad(raw))
            elif n.startswith('\x05') or n.startswith('TransmissionData') or n.startswith('RevitPreview') or n == 'Contents':
                st = 'opaque'
            else:
                _p, _a, _u = inflate_stream(raw); check_members(raw); st = 'decoded' if DECODED.match(n) else 'opaque'
                if n == 'Global/History' and history_bad(_a): st = 'error:' + history_bad(_a)
                if n == 'Global/ElemTable' and elemtable_dups(_a): st = 'error:ElemTable duplicate ids %d' % elemtable_dups(_a)
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
        if p.lower().endswith(('.rvt', '.rte')):     # all 1115 local .rvt/.rte have these; a directory-entry flip (type byte) silently drops a whole storage and used to read ok
            miss = [k for k in CORE if k not in r['streams']]
            if miss: r['open'] = 'error:ValueError missing core stream ' + ','.join(miss)[:50]
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
