"""Strict [MS-CFB] header validation: returns a list of violated rules (empty = clean). Used by rvtreadcov; rules are only kept if 0 local files violate them."""
import struct
def violations(b):
    h = b[:512]; v = []
    if len(h) < 512 or h[:8] != bytes.fromhex('d0cf11e0a1b11ae1'): return ['magic']
    if h[8:24] != bytes(16): v.append('clsid')
    minor, major, bom, sshift, mshift = struct.unpack_from('<HHHHH', h, 24)
    if minor != 0x3e: v.append('minor')
    if major not in (3, 4): v.append('major')
    if bom != 0xfffe: v.append('byteorder')
    if sshift != {3: 9, 4: 12}.get(major, -1): v.append('sectorshift')
    if mshift != 6: v.append('minishift')
    if h[34:40] != bytes(6): v.append('reserved')
    ndir, nfat, dirstart, _tx, cutoff, mfstart, nmf, dfstart, ndf = struct.unpack_from('<IIIIIIIII', h, 40)
    if major == 3 and ndir != 0: v.append('numdirsectors')
    if cutoff != 4096: v.append('cutoff')
    dif = struct.unpack_from('<109I', h, 76)
    used = [x for x in dif if x != 0xffffffff]
    if ndf == 0 and dfstart not in (0xfffffffe, 0xffffffff): v.append('difatstart')
    if ndf == 0 and nfat != len(used): v.append('numfat')
    if ndf == 0 and dif[:len(used)] != tuple(used): v.append('difat-gap')
    return v
