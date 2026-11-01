# Partitions/* member framing (measured, Autodesk samples 2017-2026)

After stripping the 'page' layer (rvtfmt.strip_pages) a Partitions stream is a sequence of frames:

    [24-byte header: 6 x u32 LE]  [gzip member, 10-byte header .. deflate .. CRC32 + ISIZE]  [8 bytes]  [optional block]  [next 24-byte header] ...

* Header words seen: (4, n, S, m, 101, 0). S = gzip member length (including its 8-byte CRC/ISIZE trailer) + 8, i.e. S counts the 8 bytes after the member. n and m vary per frame (not decoded).
* Gap between the end of one member (after its CRC/ISIZE) and the start of the next member's gzip magic is 32 bytes (8 tail + 24 header) in most frames, and 220 (2018+; 78 in 2017) in every Nth frame: an extra 188-byte (2017: 46-byte) block whose content is not decoded.
* Correct member walk: find the gzip magic, inflate, check CRC32/ISIZE, then CONTINUE AT THE END OF THAT MEMBER (never rescan inside it). Scanning for 1f8b0800 anywhere gives false hits inside deflate data / the extra blocks; that is why a scan-everything walk reports hundreds of 'bad members' on clean files.
* Measured with tools/partition_frames.py (sequential walk): 2017-2026, smallest file per year plus the 2024 Precast template and 2024 Snowdon Structural: every member passes CRC (e.g. 1572/1572 in Snowdon Structural Partitions/20, 1337/1337 in the template, 344-405 per year's main partition), 0 unparseable finds, gaps only 32 / 220 (78 in 2017). Not proven for all 78 files (script not run on the others) and the meaning of n, m and the extra block is open.
