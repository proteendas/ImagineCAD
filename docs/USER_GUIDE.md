# ImagineCAD User Guide

## The window

```
┌ IC ▾  New Open Save Undo Redo Plot      ImagineCAD — Drawing1          Image to CAD  ? ┐  title bar / quick access
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

- **IC menu**: new, open, save, export, plot, recent drawings, About, exit.
- **Ribbon**: every button runs the same command you could type; hover for the command name and alias.
- **Properties palette** (`PR`): layer, colour, linetype and exact geometry of the selection: coordinates, lengths, angles, radii, ellipse axes, spline fit points, text, hatch pattern/scale/angle, block scale/rotation, image width/fade.
- **Command line**: start typing anywhere on the canvas. Suggestions appear as you type; `Tab` completes.
- **Status bar**: Model/Layout, coordinates, drafting aids, sheet scale. Right-click **POLAR** to set the angle increment, **OSNAP** to choose snap modes, **SNAP** to set the spacing.

## Navigating

| Action | How |
|---|---|
| Zoom | Mouse wheel (at the cursor), trackpad pinch |
| Pan | Middle-button drag, `Alt`/`Option` + drag, or `PAN` |
| Zoom extents | Double-click middle button, `ZOOM E` |
| Zoom window / previous / sheet | `ZOOM W`, `ZOOM P`, `ZOOM A` |
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

## Sheet, scale and title block

**Annotate → Title Block** (or click the scale in the status bar): paper size,
orientation, plot scale, **Fit sheet to drawing**, and the title block fields
(company, title, drawing number, revision, drawn/checked/approved by, date,
material, finish, sheet, units, general tolerances, projection).
**Use company & drawn-by as defaults** applies them to new drawings.
`UNITS` sets the unit label and precision.

## Files

| Action | Formats |
|---|---|
| Open | `.icad`, `.dwg`, `.dxf`; sketch images open Image to CAD |
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
