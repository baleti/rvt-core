#!/usr/bin/env python3
"""partition_frames.py FILE: sequential walk of the gzip members of every Partitions/* stream (see tools/PARTITION-FRAMING.md); prints members, CRC-ok count, gap histogram."""
import sys,os,struct,zlib,collections
sys.path.insert(0,'' + os.path.expanduser("~/rvt-research") + '')
import olefile
from rvtfmt import strip_pages, GZ
f=sys.argv[1]; o=olefile.OleFileIO(f)
for pn in sorted('/'.join(e) for e in o.listdir(streams=True) if e[0]=='Partitions'):
    data=strip_pages(o.openstream(pn).read()); pos=0; fr=[]; nonhdr=0; hdrs=collections.Counter()
    while True:
        i=data.find(GZ,pos)
        if i<0: break
        d=zlib.decompressobj(-15)
        try: out=d.decompress(data[i+10:]); tl=d.unused_data
        except zlib.error: pos=i+4; nonhdr+=1; continue
        ok=len(tl)>=8 and struct.unpack('<II',tl[:8])==(zlib.crc32(out)&0xffffffff,len(out)&0xffffffff)
        end=len(data)-len(tl)+(8 if ok else 0)
        h=struct.unpack_from('<6I',data,i-24) if i>=24 else None
        fr.append((i,end,ok,h,len(out))); pos=end
    gap=[(fr[k+1][0]-fr[k][1]) for k in range(len(fr)-1)]
    print(pn,len(data),'members',len(fr),'crc_ok',sum(x[2] for x in fr),'gap hist',collections.Counter(gap).most_common(4),'bad-find',nonhdr)
