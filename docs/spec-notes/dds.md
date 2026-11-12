# Spec notes: DDS textures (Layer 2)

Tags as in `nif.md`. Observations cover all 26,907 `.dds` files in the owner's archives
(FNV 1.4.0.525). Public source: Microsoft's DirectDraw Surface documentation
("DDS File Layout", `DDS_HEADER`, `DDS_PIXELFORMAT`), cited below as [documented: MS DDS].

## 1. Layout

1. Magic `DDS ` (4 bytes) [documented: MS DDS; observed in every file].
2. Header of 31 u32 values (124 bytes) [documented: MS DDS]: structure size (always 124
   [observed]), flags, height, width, pitch or linear size, depth, mip-map count,
   11 reserved u32, then the 32-byte pixel format, then four capability u32s and one
   reserved u32. Note that height comes before width.
3. Pixel format [documented: MS DDS]: size (always 32 [observed]), flags, FourCC (4
   bytes), RGB bit count, then red, green, blue and alpha bit masks.
4. Pixel data follows immediately: for each face, every mip level from largest to
   smallest [documented: MS DDS]. There is no DX10 extension header in these files
   (no FourCC `DX10`) [observed].

## 2. Formats present [observed]

| Pixel-format flags | FourCC / masks | Name used here | Files |
|---|---|---|---|
| 0x4 (FourCC) | `DXT1` | DXT1 | 19,437 |
| 0x4 | `DXT5` | DXT5 | 5,955 |
| 0x4 | `DXT3` | DXT3 | 1,486 |
| 0x41 (RGB + alpha) | 32 bit, R 0x00FF0000, G 0x0000FF00, B 0x000000FF, A 0xFF000000 | A8R8G8B8 | 13 |
| 0x40 (RGB) | 24 bit, R 0x00FF0000, G 0x0000FF00, B 0x000000FF | R8G8B8 | 8 |
| 0x41 | 16 bit, R 0x0F00, G 0x00F0, B 0x000F, A 0xF000 | A4R4G4B4 | 8 |

Names follow the Direct3D 9 format names for those masks [documented: MS "Map Direct3D 9
Formats to Direct3D 10"]. Masks are little-endian: an A8R8G8B8 pixel is stored B, G, R, A.

### 2.1 Layouts read beyond the base game's (#1439)

No file of the base game uses these; a mod's or a localized install's textures may (a
font texture in one of them was otherwise drawn with a stand-in font). Flags and masks
as [documented: MS DDS, `DDS_PIXELFORMAT`: `DDPF_RGB` 0x40, `DDPF_ALPHAPIXELS` 0x1,
`DDPF_LUMINANCE` 0x20000; "Using Texture Formats", the masks of each Direct3D 9 format]:

| Flags | Bits and masks (R, G, B, A) | Name |
|---|---|---|
| 0x40 | 32: 0x00FF0000, 0x0000FF00, 0x000000FF, - | X8R8G8B8 |
| 0x41 | 32: 0x000000FF, 0x0000FF00, 0x00FF0000, 0xFF000000 | A8B8G8R8 |
| 0x40 | 32: 0x000000FF, 0x0000FF00, 0x00FF0000, - | X8B8G8R8 |
| 0x40 | 16: 0xF800, 0x07E0, 0x001F, - | R5G6B5 |
| 0x41 | 16: 0x7C00, 0x03E0, 0x001F, 0x8000 | A1R5G5B5 |
| 0x40 | 16: 0x7C00, 0x03E0, 0x001F, - | X1R5G5B5 |
| 0x20000 | 8: luminance 0xFF | L8 |
| 0x20001 | 16: luminance 0xFF, alpha 0xFF00 | A8L8 |

A layout without alpha reads as opaque; luminance is copied to all three colours
[documented: MS "Map Direct3D 9 Formats to Direct3D 10"]. Alpha-only `DDPF_ALPHA`
(A8) is not read: Direct3D samples it with black colour, so what it would look like
in the game is not known.

## 3. Sizes [observed: exact for all 26,907 files]

- Width and height are powers of two in every file, 4096 at most [observed]. A texture
  with a side of 0 or above 16384 is refused as a broken header (`fnv_dds::MAX_SIDE`,
  the engine's own cap) [design: #1995]; the listing still reports the header as it is.
- Number of levels = the stored mip count, or 1 when it is 0. 2,214 files leave the
  "mip count valid" header flag (0x20000) clear; using the stored count anyway still gives
  the exact size, so the flag can be ignored [observed].
- Each level halves width and height (rounding down, minimum 1).
- Block formats store 4×4 texel blocks: 8 bytes per block for DXT1, 16 for DXT3/DXT5;
  blocks per level = max(1, ceil(w/4)) × max(1, ceil(h/4)) [documented: MS DDS].
  Uncompressed levels take w × h × (bit count / 8) bytes.
- Cube maps (capabilities-2 value 0xFE00: cube map with all six faces [documented: MS
  DDS]) store six faces one after another, each a full mip chain. Six files are cube maps.
- The bytes after the 128-byte header equal the sum above exactly; no file has trailing
  data.
- Mods' files are not all exact [observed: Classic Fallout Weapons, 78 of its 468
  textures, DXT1 and DXT5, carry 8 to 32 bytes more than their size and mip count
  need, e.g. `sten.dds`: 1024×1024 DXT1, 10 levels counted, 8 bytes over; the owner
  sees these weapons textured in the original game]. The levels the header counts are
  read and what follows is left alone [hypothesis: the original does the same]; a file
  with less data than the layout needs is refused (#2164).
- A mod's block-format texture may have sides that are not multiples of 4 [observed:
  the same mod's `YK32_innerpanel.dds`, DXT5, 179×196, 8 levels]. Its blocks cover
  ceil(w/4) × ceil(h/4) and the texels past the edge are not part of the picture
  [documented: MS DDS]. The engine decodes such a texture to RGBA on the CPU, as GPUs
  take block formats only at multiples of 4 [design: #2164].

## 4. Naming conventions [hypothesis, to verify against shaders and the NIF texture sets]

Base texture `x.dds`, normal map `x_n.dds`, glow map `x_g.dds`, and environment mask
`x_m.dds` / cube map `*_e.dds`. The NIF `BSShaderTextureSet` lists the texture paths
explicitly, so rendering does not rely on the naming convention.

## 5. CLI: `fnvtool dds list <archive>` → `fnv.dds.listing/1`

```json
{"schema": "fnv.dds.listing/1", "archive": "Fallout - Textures.bsa",
 "files": [{"path": "textures\\x.dds", "width": 512, "height": 512, "mip_count": 10,
            "levels": 10, "format": "DXT1", "cubemap": false, "faces": 1,
            "expected_size": 174904, "actual_size": 174904, "error": null}]}
```

`mip_count` is the stored value; `levels` the number used (§3). `format` is a name from
§2 (a row matches on the whole pixel-format flags value), or, for anything else,
`unknown(flags=0x%x,fourcc=<4 ASCII chars, or 0x + 8 hex digits>,bits=%d,masks=0x%08x/0x%08x/0x%08x/0x%08x)`
with `expected_size` `null`. `cubemap` = capabilities-2 bit 0x200; `faces` = number of the
six face bits set (1 when not a cube map). Sizes count the
bytes after the 128-byte header. `error` is set when the file is not a valid DDS (bad
magic, short header); then the other fields are `null`. Lab comparator: **keyed** on
`path`, all fields.
