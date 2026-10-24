# SMM2 Super World map format

Super Mario Maker 2 (v3.0.0 and later) shares a Super World as one **world-map blob** of 0x500
bytes plus the list of its courses. TGRCode's `super_world/<id>` answer carries the blob base64
encoded (`unk1`) next to the course records; the game also ships a new-world template in this
format (`MyWorld/new_layout_data.ldt` in its RomFS). Reference implementation:
[`core/crates/formats/src/smm2/world.rs`](../../core/crates/formats/src/smm2/world.rs).

**Verified** below means: checked against the template and five Super Worlds downloaded from
TGRCode (39 worlds), with byte-exact round trips of every decrypted map.

## 1. Encryption (verified)

| Offset | Size | Contents |
|---|---|---|
| 0x000 | 0x4D0 | map, AES-128-CBC, no padding |
| 0x4D0 | 16 | AES IV |
| 0x4E0 | 16 | random state, 4 × u32 LE |
| 0x4F0 | 16 | AES-CMAC of the decrypted map |

The AES and CMAC keys are derived from the random state exactly as for course files
([smm2-course.md §1](smm2-course.md)), but from a different 64-word key table. That table is
not part of this project: the importer finds it in the player's own executable as the window
that decrypts the template from the same dump with a valid CMAC.

## 2. The decrypted map (0x4D0 bytes, verified)

| Offset | Size | Contents |
|---|---|---|
| 0x00 | 0x75 | zero |
| 0x75 | 1 | 3 (format version) |
| 0x76 | 1 | number of worlds, 1–8 |
| 0x77 | 1 | lives at the start, 1–99 |
| 0x78 + w·0x8B | 0x8B | world record w = 0…7 (unused records keep their point slots empty) |

World record:

| Offset | Size | Contents |
|---|---|---|
| +0x00 | 1 | theme: 0 ground, 1 underground, 2 desert, 3 forest, 4 snow, 5 sky, 6 volcano, 7 space |
| +0x01 | 84 | 28 cells × 3 bytes, cell i at x = i mod 7, y = i div 7 (row 0 is the map's bottom row; the game draws row y + 1 above row y) |
| +0x55 | 36 | 6 point slots × 6 bytes |
| +0x79 | 3 | warp pipe A: x, y, present |
| +0x7C | 3 | warp pipe B: x, y, present |
| +0x7F | 12 | 3 Toad Houses × (x, y, minigame, present) |

**Cell**: rotation (0–3), road type, flag. The road types and the sides they connect (E = x+1,
W = x−1, S = y+1, N = y−1) by rotation 0 / 1 / 2 / 3:

| Type | Piece | Connections |
|---|---|---|
| 0 | none | — |
| 1 | decoration (no road) | — |
| 2 | end | N / S / E / W |
| 3 | straight | N-S / N-S / E-W / E-W |
| 4 | corner | E+S / W+N / W+S / E+N |
| 5 | T | E+N+S / W+N+S / E+W+S / E+W+N |
| 6 | curve | as 4 |
| 7 | bridge | as 3 |
| 8 | ledge | E and W along its row; N to a cell that connects S; S to the point under it |

With these rules every point, pipe and Toad House of the sample worlds is reachable from the
start and every connection is two-way.

**Point slot**: type (0 empty, 1 course, 3 start, 4 the world's last course: castle or airship),
icon (the point's model), x, y, course (the maker's Coursebot slot, 0xFF none), flags (bit 0: the
course was deleted).

**Course order** within a world: the course points in slot order, then the last course (type 4).
A Super World's course list (TGRCode's `courses`) gives the courses world by world in this order.
