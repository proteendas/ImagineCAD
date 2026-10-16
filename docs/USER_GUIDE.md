# ImagineCAD User Guide

## The window

```
┌ [logo] New Open Save Undo Redo Plot  File Edit View Insert Format Draw … Help   Image to CAD ? ┐  title bar / menus
├ Home  Insert  Annotate  View  Output  AI ──────────────────────────────────────────────┤  ribbon tabs
│ Draw │ Modify │ Annotation │ Layers │ Block │ Utilities │ Clipboard                      │  ribbon panels
├ Drawing1 × | bracket.dwg* × | + ────────────────────────────────────────────────────────┤  drawing tabs
│ PROPERTIES │                                                                            │
│            │                       drawing canvas                                      │
│            │  (crosshair, grips, snap markers, polar tracking, dynamic input tooltip)   │
├────────────┴────────────────────────────────────────────────────────────────────────────┤
│ command history                                                                         │  command line
│ ›_ LINE Specify next point or [Undo]: @50<30                                            │
├ Model Layout │ 125.4, 88.2 │         GRID SNAP POLAR ORTHO OSNAP LWT DYN  Scale 1:2 ┤  status bar
```

- **Logo menu** (top left): new, open, save, export, plot, recent drawings, About, exit.
- **Menu bar**: *File, Edit, View, Insert, Format, Draw, Dimension, Modify, Tools, AI, Help* list every command with its alias, so anything can be found without the ribbon.
- **Ribbon**: every button runs the same command you could type; hover for the command name and alias. Buttons with a **▾** are split buttons: click the top for the usual tool, the arrow for its variants (Circle ▾: centre/radius, centre/diameter, 2-point, 3-point; Dimension ▾: every dimension type). Click a panel title with a ▾ (**Draw ▾**, **Modify ▾**, **Annotation ▾**, **Layers ▾**) for the panel's other tools. The ribbon never scrolls: when the window is too narrow, panels collapse from the right into single buttons (**Layers ▾**, **Block ▾**…) that drop the whole panel down as a menu, and whatever still does not fit goes under **More ▾**.
- **Properties palette** (`PR`): layer, colour, linetype and exact geometry of the selection: coordinates, lengths, angles, radii, ellipse axes, spline fit points, text, hatch pattern/scale/angle, block scale/rotation, image width/fade.
- **Command line**: start typing anywhere on the canvas. Suggestions appear as you type; `Tab` completes.
- **Status bar**: Model/Layout, coordinates, drafting aids, annotation scale. Right-click **POLAR** to set the angle increment, **OSNAP** to choose snap modes, **SNAP** to set the spacing. Click **Scale** to pick the annotation scale (1:1, 1:2, 1:5 … 1:1000, 2:1 …): dimension text and arrows, like the sheet, are sized for that plot scale.

## Navigating

| Action | How |
|---|---|
| Zoom | Mouse wheel (at the cursor), trackpad pinch |
| Pan | Middle-button drag, `Alt`/`Option` + drag, or `PAN` |
| Zoom extents | Double-click middle button, `ZOOM E` |
| Zoom window / previous / sheet | `ZOOM W`, `ZOOM P`, `ZOOM A` |
| Zoom by a factor | `ZOOM 2X` (in), `ZOOM 0.5X` (out) |
| Paper view | `LAYOUT` or the *Layout* button |

## Entering points

| Input | Meaning |
|---|---|
| click | snapped point (object snap, polar tracking, ortho, grid snap) |
| `100,50` | absolute X,Y |
| `@20,0` | relative to the last point |
| `@50<30` | polar: 50 units at 30° |
| `75` | direct distance along the cursor direction |

`Enter`, `Space` or right-click confirms; an empty `Enter` repeats the last
command; `Esc` cancels. A command line can carry its inputs: `CIRCLE 0,0 25`,
`ZOOM E`. `<ENTER>` inside such a line presses Enter (useful for macros).

### Drafting aids

| Aid | Key | Behaviour |
|---|---|---|
| OSNAP | F3 | Endpoint □, Midpoint △, Center ○, Quadrant ◇, Intersection ×, Perpendicular ⊥, Insertion, Node, Nearest |
| POLAR | F10 | Snaps the rubber band to multiples of the increment (default 15°) and shows a green tracking ray |
| ORTHO | F8 | Horizontal/vertical only (overrides polar) |
| GRID / SNAP | F7 / F9 | Grid display / grid snap |
| DYN | F12 | Cursor tooltip with prompt, length and angle |

## Selecting

- **Click** to add; `Shift`+click removes.
- **Window** (left → right, blue) selects objects fully inside. **Crossing** (right → left, green) selects anything touched; `STRETCH` uses the last crossing window.
- `Ctrl+A`, `SELECTALL`, or `QSELECT` (by type, layer, colour).
- **Grips**: click a blue grip, then the new position. You can stretch line ends, polyline and spline vertices, circle radius, arc shape, ellipse axes, text/block/image positions, and dimension points.
- **Double-click** text, multiline text, a leader or a dimension to edit its text.

## Commands

### Draw

| Command | Alias | Notes |
|---|---|---|
| `LINE` | `L` | **C**lose, **U**ndo |
| `PLINE` | `PL` | one polyline object |
| `CIRCLE` | `C` | center/radius, **D**iameter, **2P**, **3P** |
| `ARC` | `A` | three-point; **C** = center/start/end |
| `RECTANG` | `REC` | try `@100,50` |
| `POLYGON` | `POL` | sides, center, vertex or radius |
| `ELLIPSE` | `EL` | axis endpoints + other-axis distance; **C** = by center |
| `SPLINE` | `SPL` | smooth curve through fit points; **C**lose |
| `POINT` | `PO` | |
| `XLINE` / `RAY` | `XL` | construction lines on layer `CONSTRUCTION` (not plotted); **H**/**V** |
| `HATCH` | `H` | click inside closed areas (islands detected); **P**attern, **S**cale, **A**ngle, select**O**bjects |

Hatch patterns: `SOLID`, `ANSI31`–`ANSI38`, `NET`, `LINE`, `ANGLE`.

### Modify

| Command | Alias | Notes |
|---|---|---|
| `MOVE` `COPY` `ROTATE` `SCALE` `MIRROR` | `M` `CO` `RO` `SC` `MI` | base point / second point, or typed angle/factor |
| `STRETCH` | `S` | crossing-window selection, then base and second point |
| `OFFSET` | `O` | lines, arcs, circles, polylines |
| `TRIM` / `EXTEND` | `TR` / `EX` | quick mode: every other object is a boundary |
| `FILLET` / `CHAMFER` | `F` / `CHA` | **R** radius / **D** distance |
| `BREAK` / `BREAKATPOINT` | `BR` / `BAP` | gap between two points / split at a point |
| `JOIN` | `J` | touching lines → polyline, arcs on one circle → arc/circle |
| `LENGTHEN` | `LEN` | **DE**lta, then click near the end to change |
| `ARRAYRECT` / `ARRAYPOLAR` | `AR` / `APO` | rows × columns / items around a center |
| `EXPLODE` | `X` | polylines, dimensions, leaders, text, hatches, **blocks** |
| `ERASE` / `OOPS` | `E` | `OOPS` restores the last erase |
| `MATCHPROP` | `MA` | copy layer/colour/linetype |
| `DIVIDE` / `MEASURE` | `DIV` / `ME` | place points along a curve |
| `OVERKILL` | | delete duplicate objects |

### Annotate

| Command | Alias | Notes |
|---|---|---|
| `TEXT` | `DT` | single-line text |
| `MTEXT` | `MT` | box sets the wrap width; `\P` = new line; edit with double-click |
| `TEXTEDIT` | `ED` | edit selected text |
| `LEADER` | `LE` | arrow, vertices, Enter, text |
| `DIMLINEAR` | `DLI` | `Enter` to pick a line; **H**/**V** |
| `DIMALIGNED` | `DAL` | |
| `DIMANGULAR` | `DAN` | two lines (pick on the legs to measure), or an arc |
| `DIMRADIUS` / `DIMDIAMETER` | `DRA` / `DDI` | |
| `DIMBASELINE` / `DIMCONTINUE` | `DBA` / `DCO` | from the last linear, aligned or angular dimension |
| `DIMORDINATE` | `DOR` | feature point, then leader end (mostly vertical → X, horizontal → Y); **X**/**Y** force the axis, **D** sets the datum (default 0,0) |
| `DIMARC` | `DAR` | arc length with the ISO arc symbol |
| `QDIM` | | select geometry, then place the line: **C**ontinuous, **S**taggered, **B**aseline, **O**rdinate, **R**adius, **D**iameter. Above/below the selection gives horizontal dimensions, beside it vertical |
| `DIMREASSOCIATE` / `DIMDISASSOCIATE` | `DRE` / `DDA` | tie dimensions to (or free them from) the geometry they measure |
| `CENTERMARK` | `CM` | |
| `DIMSTYLE` | `D` | text height, arrows, gaps, precision, LTSCALE |

Dimension text override: leave empty for the measured value; `<>` inserts it (`2x <>`).

**Engineering symbols** (Annotate → Symbols). Each opens a dialog with a live
preview, then asks where to place the symbol; double-click a placed symbol (or
use the Properties palette) to edit it.

| Command | Symbol | Placement |
|---|---|---|
| `TOLERANCE` (`TOL`) | geometric tolerance feature control frame (ISO 1101): 14 characteristics, Ø zone, Ⓜ/Ⓛ/Ⓢ modifiers, up to three datums, composite second row | arrow point on the feature (Enter for no leader), then the frame |
| `DATUM` | datum feature symbol (framed letter and filled triangle); the next unused letter is proposed | point on the feature, then the frame |
| `SURFFINISH` (`SF`) | surface texture (ISO 1302/21920): any process / machined / no removal, requirement (Ra, Rz…), method, lay, all around | point on the surface, then the direction away from it (Enter = upright) |
| `WELD` | weld symbol (ISO 2553): fillet, square, V, bevel, U, J, plug, spot, seam on the arrow and other side, size and length, all-around, field weld, tail | arrow point on the joint, then the reference line |

Feature control frames are written to DXF/DWG as native tolerance entities;
the other symbols travel as their graphics plus ImagineCAD data.

**Associative dimensions.** A dimension placed on snap points (endpoints,
centres, midpoints) or on a circle or arc is tied to that geometry. Move,
stretch, scale or grip-edit the geometry and its dimensions update; delete
it and the dimension stays where it is, no longer associated. Dimensions in
opened DXF/DWG files and in Image to CAD results are associated automatically.

### Blocks and references

| Command | Notes |
|---|---|
| `BLOCK` (`B`) | name → base point → objects; the objects are replaced by an insert |
| `INSERT` (`I`) | name → point → scale → rotation; or **Insert → Insert Block** for the Blocks palette |
| `EXPLODE` | turns an insert back into objects (layer-0 contents take the insert's layer) |
| `PURGE` (`PU`) | removes unused layers and blocks |
| `IMAGEATTACH` (`IAT`) | raster underlay (e.g. a scan to trace over); set *Fade* in Properties |

Objects inside a block that are on layer `0` with ByLayer properties take the
insert's layer, colour and linetype, the usual CAD convention.

### Layers

`LAYER` (`LA`) opens the Layer Properties Manager: new, delete (empty layers),
set current, on/off, lock, plot, colour, linetype, lineweight. Quick tools:
`LAYISO` (show only the selected objects' layers), `LAYUNISO`, `LAYOFF`,
`LAYON`, `LAYLCK`, `LAYULK`, `LAYMCUR`.

Mechanical template layers: `0`, `OUTLINE` (0.50), `HIDDEN`, `CENTER`,
`DIMENSIONS`, `TEXT`, `HATCH`, `CONSTRUCTION` (no-plot).

### Inquiry

`DIST` (`DI`), `AREA` (`AA`, points or **O**bject), `ID`, `LIST` (`LI`).

## AI Chat

**AI → AI Chat** (`AICHAT`) opens a chat panel beside the drawing. Describe a
change in plain language and the AI makes it:

- "Change the company name to ACME ENGINEERING and the material to C45"
- "Make the 1390 mm segment 1400 mm" (a shaft or plate created by Image to CAD
  is regenerated from its parameters, with every dimension updated)
- "Add a 10 mm hole at the centre of the plate and dimension it"
- "Put the selected lines on layer HIDDEN"
- "What is the overall length?" (questions are answered without changes)

The AI sees the drawing's entities, layers, title block and selection. Select
objects first to refer to "this" or "these". Each answer is applied as **one
undo step**: press **Ctrl+Z** or the message's **Undo** button to revert it.
Manual editing works alongside the chat at all times.

The chat uses the engine chosen in **AI Settings** (any cloud key, Ollama or a
local server). From a terminal: `imaginecad ask part.icad "make the bore 12 mm" -o part.icad`.

## Regions and mass properties

- `BOUNDARY` (`BO`): click inside closed areas to create closed polylines
  along their boundaries (islands included); **O** switches the object type to
  regions.
- `REGION` (`REG`): turn closed objects, or lines and arcs that join into loops,
  into regions.
- `UNION` (`UNI`), `SUBTRACT` (`SU`: first the regions to keep, then those to
  remove), `INTERSECT` (`IN`): combine regions.
- `MASSPROP`: area, perimeter, bounding box, centroid, moments and product of
  inertia, radii of gyration and principal moments of regions or closed
  objects.

## Precision aids

- **Object snap tracking** (`OTRACK`, F11, status bar): rest the cursor on an
  object snap point for a moment to acquire it (a green +). Moving away then
  shows dotted tracking lines horizontally, vertically and at polar angles from
  acquired points; where two meet, the cursor snaps to the intersection.
  Resting on an acquired point again releases it.
- **Selection cycling**: where objects overlap, click the same spot again to
  select the next one (the command line names it).

## Text styles and spelling

`STYLE` (`ST`) manages **text styles**: any installed TrueType/OpenType font
(searchable list), width factor, oblique (slant) angle and an optional fixed
height. New text uses the current style; change a text's style in the
Properties palette. Text in a TrueType style is drawn from the font's own
outlines, so it looks the same on screen, in PDF and in SVG; DXF/DWG keep the
style with its font file name. *Standard* uses the built-in drafting font.

`SPELL` (`SP`) checks the selection, or every text in the drawing: single and
multiline text, leaders, dimension text, attribute values and table cells.
For each unknown word choose a suggestion and **Change** / **Change All**, or
**Ignore**, **Ignore All**, **Add to Dictionary** (kept in your personal
dictionary). Part numbers, codes with digits, acronyms and fields are skipped.

## Tables and multileaders

`TABLE` opens a dialog: start **empty** (rows × columns), **from CSV / a
spreadsheet** (paste or open a file), or as a **parts list** built from the
drawing's block attributes; optionally add a merged title row, then place the
table by its top-left corner. Double-click a table (or `TABLEDIT`) to edit cells,
add or remove rows and columns, merge a row, set alignment and column widths, or
**Fit columns** to the text. Cells may contain fields.

`MLEADER` (`MLD`) draws a leader in the current **multileader style**;
`MLEADERSTYLE` (`MLS`) manages styles: arrowhead (closed filled, open, dot,
tick, none), sizes, landing and a text frame (box, or a circle for item
balloons). The drawing starts with *Standard* and *Balloon*. Change a leader's
style in the Properties palette.

## Block attributes

`ATTDEF` (`ATT`) defines an attribute: a tag (e.g. `PART_NO`), the prompt shown
when the block is inserted, and a default value; then pick where its text goes.
Include attribute definitions when you create the block with `BLOCK`. Each
reference of the block then carries its own values:

- **Inserting** a block with attributes asks for the values right away.
- **Double-click** a block reference, use `ATTEDIT` (`ATE`), or the Properties
  palette to change them.
- `ATTEXPORT` writes every block reference's attributes to a CSV file (block,
  position and one column per tag), for parts lists and bills of materials.
- `EXPLODE` turns attributes into plain text with their values.

Attributes are written to DXF/DWG as standard attribute definitions and values.

## Fields

Text, multiline text, leaders and attributes can contain fields that always
show the current value: `{{TITLE}}`, `{{COMPANY}}`, `{{DWG_NO}}`, `{{REV}}`,
`{{SCALE}}`, `{{SIZE}}`, `{{DATE}}`, `{{TODAY}}`, `{{FILENAME}}`,
`{{MATERIAL}}`… and object fields such as `{{LENGTH:12}}`, `{{AREA:12}}`,
`{{DIAMETER:12}}` (object id from `LIST`) or `{{COUNT:BOLT}}`. In the text
editor, **Insert field ▾** lists them. Exports show the values; ImagineCAD keeps
the field codes in DXF/DWG.

## Sheet, scale and title block

Text, dimensions and arrows are sized in paper millimetres and scaled by the
sheet (annotation) scale, so they print at their real size and keep their
proportion to the geometry at any zoom; choose the scale from the status bar.

**Annotate → Title Block** (or **Page Setup…** under the status-bar scale): paper size,
orientation, plot scale, **Fit sheet to drawing**, and the title block fields
(company, title, drawing number, revision, drawn/checked/approved by, date,
material, finish, sheet, units, general tolerances, projection).
**Use company & drawn-by as defaults** applies them to new drawings.

**Your own title block.** Under *Layout*, choose a block with attributes
instead of the built-in title block: it is placed at the frame's bottom-right
corner (its base point) at the sheet scale, and each attribute whose tag names
a title field (COMPANY, TITLE, DWG_NO, REV, SHEET, SIZE, MATERIAL, FINISH,
SCALE, UNITS, DRAWN, CHECKED, APPROVED, DATE, TOLERANCE_1/2) is filled in
automatically. **Create editable copy** adds the built-in title block as such a
block to start from.
`UNITS` sets the unit label and precision.

## External references

`XATTACH` (`XA`, Insert → Attach Drawing) shows another DWG, DXF or `.icad`
drawing inside this one, placed like a block, and keeps it up to date with its
file. Its layers appear as `NAME|LAYER` so you can turn them off or recolour
them without touching the source. Opening a drawing reloads its references (a
moved file is also looked for next to the drawing). `XREF` (`XR`) lists them
with their status: **Reload**, **Path…** (choose the file again), **Open**,
**Bind** (make it an ordinary block of this drawing) and **Detach**.

Raster images (`IMAGEATTACH`) and external references are kept in DXF and DWG
files.

## Layouts, viewports and plotting

The tabs at the bottom left switch between **Model**, **Layout1** (the main
sheet, drawn over the model) and any number of **paper layouts**:

- **+** (or `LAYOUTNEW`) adds a layout: its own paper, title block and plot
  style, with one viewport showing the model at a standard scale that fits.
- `MVIEW` (`MV`) on a layout adds a viewport by two corners (**F** fills the
  drawable area). Click a viewport to set its scale, view centre and size in
  the Properties palette, lock it, fit the model, or delete it. The title
  block shows the main viewport's scale.
- Right-click a layout tab to rename, page-set-up, plot or delete it. Draw and
  edit in Model; layouts present the model.
- **Plot styles** (Page Setup, Export, or the layout's properties):
  monochrome, colour as displayed, grayscale, or screened 50 %.
- `PLOT` writes the sheet or layout on screen as a true-scale PDF. `PUBLISH`
  batch-plots any sheets and layouts of the open drawings into one multi-page
  PDF or one PDF per sheet.

Layouts are saved in `.icad` drawings; DXF/DWG exports contain the main sheet.

## Sheet sets

`SHEETSET` (`SSM`, Output → Sheet Set) opens the **Sheet Set Manager**: a
named, ordered set of sheets from any number of drawings, saved as an
`.icadset` file next to them (double-click it to open the manager).

- **+ Current drawing** / **+ Drawings…** add each drawing's main sheet and its
  layouts. Give every sheet a number and a title; **Renumber** numbers them in
  order; ▲ ▼ reorder, × removes, **Open** shows the sheet.
- The **project values** (company, drawn, checked and approved by, a drawing
  number prefix such as `GB-`) are shared by every title block.
- **Publish set to PDF…** plots every sheet into one PDF with its set number,
  title and `n OF N` in its title block, without changing the drawings
  (`imaginecad publish SET.icadset -o OUT.pdf` does the same from a script).
- **Update title blocks** writes the numbering and values into the drawings
  themselves (open drawings change as one undo step; closed ones are saved).
- **Insert sheet list** places a sheet index table in the current drawing.

## Markup and parts lists

- **Revision clouds** (`REVCLOUD`, `RC`; Annotate → Markup): two corners for a
  rectangular cloud, **P** for a polygonal one, **O** to turn a closed
  polyline, circle or ellipse into a cloud, **A** for the arc length (scaled to
  the sheet by default). Grips move the cloud's corners.
- **Bill of materials** (`BOM`): a parts list built from the block references
  in the drawing, one item per block and set of attribute values, with the
  quantity counted and columns for `PART_NO`, `DESCRIPTION`, `MATERIAL` and
  any other attributes (the block name describes blocks without attributes).
  Run `BOM` again after changes to refresh it in place.
- **Balloons** (`BALLOON`): Enter balloons every BOM item at once, round the
  drawing, each in the direction of its part and spread so they keep clear of
  each other; or pick one part and place its balloon.

## Keeping dimension text clear

Dimension text moves itself to the nearest clear place when it would overlap
other text or run through the part: along its dimension line first, then one
row further out (radius and diameter text swings round the circle). This
happens when a dimension is placed and when the points it measures move;
text you drag on purpose stays where you put it. `DIMARRANGE` (`DARR`) tidies
selected dimensions, or all of them, for drawings made elsewhere.

## Parametric drafting

Constraints keep relationships true while you edit (Parametric tab and menu).

**Geometric constraints** apply to lines, polyline segments, circles and arcs:
`GCHORIZONTAL`, `GCVERTICAL`, `GCPARALLEL`, `GCPERPENDICULAR`, `GCCOLLINEAR`,
`GCEQUAL` (lengths or radii), `GCCOINCIDENT` (two points), `GCCONCENTRIC`,
`GCTANGENT` (line and circle, or two circles), `GCFIX` (hold a point).
`AUTOCONSTRAIN` (`AC`) adds the obvious ones to a selection: horizontal and
vertical edges, joined end points, tangent arcs, concentric circles.

**Driving dimensions** set sizes by name: `DCHORIZONTAL`, `DCVERTICAL`,
`DCALIGNED`, `DCRADIUS`, `DCDIAMETER`, `DCANGULAR`. Each is named `d1`, `d2`,
… and takes a value or an expression of the others, for example `d2 = d1 / 2`
or `d3 = sqrt(d1^2 + d2^2)` (operators `+ - * / ^`, `sqrt`, `sin`, `cos`,
`tan` in degrees, `abs`, `pi`). Type it when placing the dimension, or edit it
in the **Parameters** manager (`PARAMETERS`, `PAR`); the geometry follows, and
grows from the dimension's first point.

After every edit the constraints are solved again: what you drag or move
stays exactly where you put it when the constraints allow, and the rest
adapts. A constraint that contradicts the others is refused. The log reports
how many degrees of freedom remain (0 = fully constrained).

Teal glyphs mark the constraints (H, V, //, ⊥, =, T, ◎, F; a dot for
coincident points) and driving dimensions show as `d1=50`; neither is plotted.
`CONSTRAINTBAR` (`CBAR`) hides or shows the glyphs; `DELCONSTRAINT` removes the
constraints of the selected objects (or all). Constraints are saved in `.icad`,
DXF and DWG drawings (other programs see the geometry).

## Files

| Action | Formats |
|---|---|
| Open | `.icad`, `.dwg`, `.dxf`; sketch images open Image to CAD. Also by double-clicking a drawing in Finder, Explorer or a Linux file manager, *Open With*, or dropping files on the window or the Dock icon |
| Save / Save As | `.icad`, `.dwg`, `.dxf` (a drawing opened from DWG saves back to DWG) |
| Export | DWG, DXF (2000, or R12 in Export options), PDF, SVG |
| Plot | `PLOT` / `Ctrl+P`: true-scale vector PDF of the sheet |

Typing `EXPORT` with no argument opens the export options. Layers with
*Plot* off (e.g. `AI-REVIEW`, `CONSTRUCTION`) are not plotted.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `Esc` | cancel / clear selection |
| `Enter`, `Space`, right-click | confirm / repeat last command |
| `Delete` | erase selection |
| `F1` | help |
| `F3` `F7` `F8` `F9` `F10` `F12` | OSNAP, grid, ortho, snap, polar, dynamic input |
| `Ctrl+N` `O` `S` `Shift+S` | new, open, save, save as |
| `Ctrl+Z` / `Ctrl+Y` | undo / redo |
| `Ctrl+C` `X` `V` | copy, cut, paste |
| `Ctrl+A` | select all |
| `Ctrl+P` | plot to PDF |
| `Ctrl+I` | Image to CAD |

(`Ctrl` is `Cmd` on macOS.)

## Settings

Stored in `ImagineCAD/settings.json` in the platform configuration folder
(`~/Library/Application Support`, `%APPDATA%`, `~/.config`). The AI API key is
never written there: it lives in the macOS Keychain, Windows Credential
Manager, or `ImagineCAD/credentials` (owner-only permissions) on Linux.
