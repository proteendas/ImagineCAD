# File formats

| Format | Open | Save | Implementation |
|---|---|---|---|
| `.dwg` | ✓ | ✓ | DXF 2000 ⇄ DWG through the bundled GNU LibreDWG tools (or the ODA File Converter if installed) |
| `.dxf` | ✓ | ✓ | native reader/writer: DXF 2000 (default) or R12 |
| `.icad` | ✓ | ✓ | native JSON, lossless |
| `.pdf` | | ✓ | native vector writer, true scale, embedded raster images |
| `.svg` | | ✓ | native writer, paper millimetres |
| images | → Image to CAD / `IMAGEATTACH` | | |

## DWG

DWG is a closed binary format. ImagineCAD converts through DXF 2000:

- **Bundled LibreDWG** (GNU, GPL-3.0). Every installer contains `dwg2dxf` and
  `dxf2dwg` built from LibreDWG 0.13.3 with ImagineCAD's patch
  (`packaging/libredwg/0001-resolve-postponed-dimension-block.patch`, which makes
  dimensions survive DXF → DWG). They live in `ImagineCAD.app/Contents/Resources/libredwg`
  (macOS), `<install dir>\libredwg` (Windows) and `/usr/lib/imaginecad` (Linux).
- **ODA File Converter** (free download from opendesign.com). If installed, it
  is preferred because it covers more DWG versions and object types.

Writing produces DWG 2000 (AC1015), which every current CAD program reads.
Reading accepts DWG R13–2018 as supported by the converter. A drawing opened
from DWG is saved back to DWG by **Save**.

Round-trip coverage (verified by the `dwg_roundtrip` test in CI): lines,
polylines, circles, arcs, ellipses, splines, points, text, multiline text,
hatches, block inserts, and linear and angular dimensions.

`IMAGINECAD_LIBREDWG=<dir>` points ImagineCAD at a different LibreDWG build.

## DXF

**DXF 2000 (default):** full structure (HEADER with `$HANDSEED`, TABLES
including BLOCK_RECORD, BLOCKS, ENTITIES, OBJECTS) with handles and owners.
Native `LINE`, `CIRCLE`, `ARC`, `LWPOLYLINE`, `TEXT`, `MTEXT`, `ELLIPSE`,
`SPLINE` (clamped cubic B-spline through the fit points), `POINT`, `XLINE`,
`RAY`, `HATCH` (polyline boundaries, pattern definition lines), `INSERT` with
`BLOCK` definitions, `DIMENSION` (rotated, aligned, radius, diameter, 3-point
angular) with anonymous `*D` blocks, `SOLID`; layer lineweights (370) and
plot flags. Validated with an independent DXF auditor (ezdxf, strict load, zero
errors and zero fixes).

Also native: ordinate dimensions, `TOLERANCE` (feature control frames, GD&T
font codes), `ATTDEF` and `ATTRIB` (block attributes), the `STYLE` table with
TrueType font files and text style references, `IMAGE` with `IMAGEDEF` /
`IMAGEDEF_REACTOR` and a `CLASSES` section, and external references (block
flag 4 with the file path, repeated in extended data because some converters
drop it). Objects with no DXF 2000 equivalent (arc-length dimensions, datum,
surface-texture and weld symbols, tables, regions) are written as an anonymous
block of their graphics with an `INSERT`, plus `IMAGINECAD` extended data that
holds the exact object, so other programs show them and ImagineCAD reads them
back unchanged. Fields are written as their current values, with the field
codes kept in extended data. The header carries `$DIMSCALE` and the dimension
style (`$DIMTXT`, `$DIMASZ`, `$DIMEXO`, `$DIMEXE`, `$DIMGAP`, `$DIMDEC`), and
reading restores them, so a drawing keeps its annotation scale.

**R12 (option):** for very old readers. Entities newer than R12 are written as
equivalents: ellipses and splines become polylines, hatches become lines or
solids, multiline text becomes text lines, and construction lines become long
lines. Blocks and dimensions remain real blocks and dimensions.

**Reading** (any version): the entities above, plus `POLYLINE`/`VERTEX` and
`LWPOLYLINE` with bulges (arcs), `MTEXT` (formatting stripped), `LEADER`,
`TRACE`, `ATTRIB`, named block definitions and `INSERT`s, `HATCH` with
polyline, line, arc, ellipse and spline edges, and two-line angular
dimensions. Layer colours, linetypes, lineweights, on/off, lock and plot flags
are imported. Unsupported types (e.g. 3D solids, proxies) are skipped and listed.

Symbols are written as `%%c` (Ø), `%%d` (°), `%%p` (±); other non-ASCII
characters as `\U+XXXX`.

## `.icad`

Pretty-printed JSON of the document (version 2): entities, layers, current
layer, dimension style, sheet and title block, linetype scale and **block
definitions**. Shapes: `Line`, `Circle`, `Arc` (CCW), `Polyline`, `Text`, `Dim`
(`Horizontal`, `Vertical`, `Aligned`, `Radius`, `Diameter`, `Angular`),
`Ellipse`, `Spline`, `Point`, `XLine`, `Ray`, `MText`, `Hatch`, `Insert`,
`Leader`, `Image`. Version 1 files open unchanged.

Raster images are stored by file path (like external references), so keep the
image with the drawing.

## PDF

One page the size of the sheet, mapped at true scale; Flate-compressed vector
content (lines, Bézier arcs, filled hatches and arrowheads), lineweights,
linetype dash patterns, monochrome or colour, Helvetica text (Ø ° ± supported),
embedded raster images (with fade), no-plot layers skipped.

## SVG

The sheet in paper millimetres, with the same styling as PDF; raster images
are embedded as data URIs.
