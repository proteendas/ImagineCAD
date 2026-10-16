# File formats

| Format | Open | Save | Implementation |
|---|---|---|---|
| `.dwg` | ✓ | ✓ | read by ImagineCAD's own DWG reader (DWG 2000–2018); written through the bundled GNU LibreDWG tools (or the ODA File Converter if installed) |
| `.dxf` | ✓ | ✓ | native reader/writer: DXF 2000 (default) or R12 |
| `.icad` | ✓ | ✓ | native JSON, lossless |
| `.pdf` | | ✓ | native vector writer, true scale, embedded raster images |
| `.svg` | | ✓ | native writer, paper millimetres |
| images | → Image to CAD / `IMAGEATTACH` | | |

## DWG

**Reading.** ImagineCAD reads DWG 2000, 2004, 2007, 2010, 2013 and 2018 files
itself (`icad-io/src/dwgnative`), with no converter: layers (colour,
linetype, lineweight, on/off, frozen, locked), linetype names, blocks and
every 2D drafting entity — lines, circles, arcs, ellipses, points, light and
classic polylines with bulges, solids, hatches (lines, arcs, elliptical arcs,
splines and polyline boundaries), splines, text, multiline text, block
references (also arrays) with their attribute values, attribute definitions,
leaders, tolerance frames, construction lines and rays, raster images,
external references, and all dimension types (also arc length) as native,
editable dimensions. Drawings saved by ImagineCAD keep everything: the
extended data with exact symbols, tables and regions, and the records of
constraints and dynamic block parameters are read too. The header's dimension variables and
linetype scale give the drawing its annotation scale. 3D objects (faces,
meshes, solids), multilines, multileaders, wipeouts and tables from other
programs are counted and left out. Paper layouts are read with their paper
size, margins, viewports (scale and view) and paper-space content.

The converter (below) is used for reading only when the built-in reader could
not decode some objects of a file; when no converter is installed the
built-in reader is always used.
`IMAGINECAD_DWG_READER=native` or `converter` forces one or the other.

**Writing** goes through DXF 2000 and a converter:

- **Bundled LibreDWG** (GNU, GPL-3.0). Every installer contains `dwg2dxf` and
  `dxf2dwg` built from LibreDWG 0.13.3 with ImagineCAD's patch
  (`packaging/libredwg/0001-resolve-postponed-dimension-block.patch`, which makes
  dimensions survive DXF → DWG). They live in `ImagineCAD.app/Contents/Resources/libredwg`
  (macOS), `<install dir>\libredwg` (Windows) and `/usr/lib/imaginecad` (Linux).
- **ODA File Converter** (free download from opendesign.com). If installed, it
  is preferred because it covers more DWG versions and object types.

Writing produces DWG 2000 (AC1015), which every current CAD program reads. A
drawing opened from DWG is saved back to DWG by **Save**.

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

Revision clouds are written as closed `LWPOLYLINE`s of bulged segments (how
other programs draw them) with extended data that keeps them clouds for
ImagineCAD. Constraints and driving dimensions are stored in an `XRECORD`
(`IMAGINECAD_CONSTRAINTS` in the named object dictionary) as JSON that refers
to entities by their handles, so they survive DXF and DWG round trips; other
programs ignore it and show the geometry. Dynamic block parameters are kept
the same way (`IMAGINECAD_DYNBLOCKS`, keyed by block record handle); a
reference with its own parameter values is written as an anonymous block of
its current geometry with extended data holding the values, so other programs
show each size as drawn.

**PDF:** true-scale vector PDF 1.4 (or PDF/A-2b, with text as outlines, an
sRGB output intent, XMP metadata and a file identifier), with an optional plot
stamp in the margin.

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
image with the drawing. Constraints are stored in `constraints` (geometric
ones by entity id and point index, driving dimensions with their name,
expression and value).

## `.icadset`

A sheet set (JSON): its name, the shared title block values (company,
project, drawn, checked and approved by, drawing number prefix) and the
ordered sheets, each with its drawing file (relative to the set file), layout
(none for the main sheet), number and title.

## PDF

One page the size of the sheet, mapped at true scale; Flate-compressed vector
content (lines, Bézier arcs, filled hatches and arrowheads), lineweights,
linetype dash patterns, monochrome or colour, Helvetica text (Ø ° ± supported),
embedded raster images (with fade), no-plot layers skipped.

## SVG

The sheet in paper millimetres, with the same styling as PDF; raster images
are embedded as data URIs.
