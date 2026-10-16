# Changelog

All notable changes to ImagineCAD are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.9.0] - 2026-10-17

### Added
- **Translated dialogs:** every dialog, palette, hint, table header and colour and plot-style name in German, French, Spanish, Italian, Portuguese, Simplified Chinese and Japanese.
- **Paper-space content on layouts**, drawn and plotted with the viewports; the DWG reader imports layouts with their paper size, viewports and paper-space content.
- **Assembly sections:** SECTIONVIEW cuts each selected part on its own and hatches it in its own direction.

## [0.8.0] - 2026-10-17

### Added
- **DWG 2007** drawings open without a converter (the Reed–Solomon coded container and its compression); the built-in reader now covers DWG 2000 to 2018.
- The DWG reader reads raster images, external reference paths, arc-length dimensions and the class section, and ImagineCAD's own extended data and records (exact symbols, tables, regions, constraints, dynamic block parameters), so drawings saved by ImagineCAD reopen fully without the converter.
- **Stepped section lines:** SECTIONVIEW takes a cutting line of several points — offset sections (jogs add no width) and aligned sections (angled legs unfolded) — with thick strokes at the bends.
- **Broken-out sections** (`BROKENOUT`): the material inside a freehand boundary is hatched and a thin break line drawn across the part.
- **Interface languages:** Simplified Chinese and Japanese, with an installed font for the script loaded automatically.
- **Plugin previews:** a plugin command with `"preview": true` shows what it would add while its last point is picked.

### Changed
- Plugin commands show their own name at the command prompt.

## [0.7.0] - 2026-10-16

### Added
- **A DWG reader written in Rust.** DWG 2000, 2004, 2010, 2013 and 2018 drawings open without a converter: layers, linetypes, blocks, lines, circles, arcs, ellipses, points, polylines with bulges, solids, hatches, splines, text, multiline text, block references and arrays with attribute values, attribute definitions, leaders, tolerance frames, construction lines and rays, and every dimension type as a native dimension, at the drawing's own annotation scale. 3D objects are listed as not read. The converter is still used for DWG 2007, damaged objects and drawings saved by ImagineCAD (their extended data); `IMAGINECAD_DWG_READER` forces either.
- **Interface languages:** Italian and Portuguese.

## [0.6.0] - 2026-10-16

### Added
- **Command scripts:** `SCRIPT` runs `.scr` files of commands and their input in the editor; `imaginecad script SCRIPT.scr [DRAWING] -o OUT` runs them without a window. Points typed where objects are asked for pick the object there; `L` selects the last object.
- **Plugins:** folders with a `plugin.json` add commands that run a command script, or a program in any language which reads a JSON request (inputs, selection, the drawing) and answers with AI Chat's edit operations, applied as one undo step; failures and time-outs leave the drawing unchanged. *Tools → Plugins…*, `PLUGINS RELOAD`, an example plugin and docs/PLUGINS.md.
- **Interface languages:** English, German, French and Spanish for the menus, ribbon, Properties palette, context menu and command line (`LANGUAGE`, *Tools → Language…*, or the system language). Command names stay in English.
- **Revolved sections:** SECTIONVIEW's *Revolved* option draws the round cross-section of a turned part.
- **Dynamic blocks:** rotation (`BROTATE`) and array (`BARRAY`) parameters.

## [0.5.0] - 2026-10-16

### Added
- **Dynamic blocks:** a block editor (`BEDIT`, `BSAVE`, `BCLOSE`) with stretch (`BSTRETCH`, with optional limits), flip (`BFLIP`), visibility state (`BVSTATE`) and lookup table (`BLOOKUP`) parameters. Each block reference sets its own values in the Properties palette, and drawing, picking, snapping, explode and export all follow them.
- **Section views** (`SECTIONVIEW`): the hatched cut faces of a part of constant thickness along a cutting line, with the cutting-line symbol, view letters and title.
- **Detail views** (`DETAILVIEW`): a circled area clipped, enlarged at any scale and titled, with the circle and letter on the original view.
- **Plot stamps** (`PLOTSTAMP`): drawing, layout, date and time, user, paper, scale and free text in the page margin of every PDF and SVG plot.
- **PDF/A-2b** output for archiving (text as outlines, sRGB output intent, XMP metadata), in Export options; `imaginecad export` and `imaginecad publish` take `--pdfa` and `--stamp [TEXT]`.
- **Image to CAD:** threaded shaft steps (thin thread roots at the minor diameter, dimensioned with the designation) and tapped holes (tap drill with a thin three-quarter circle); surface finish values become finish symbols on the nearest edge; sketches of several parts are drawn side by side with balloons and a parts list, each part validated and every written value required to be used by one of them.

### Changed
- Constraints and driving dimensions round-trip through DXF and DWG (`IMAGINECAD_CONSTRAINTS` XRECORD), as do dynamic block parameters (`IMAGINECAD_DYNBLOCKS`).

## [0.4.0] - 2026-10-16

### Added
- **Parametric drafting:** geometric constraints on lines, polyline segments, circles and arcs (`GCHORIZONTAL`, `GCVERTICAL`, `GCPARALLEL`, `GCPERPENDICULAR`, `GCCOLLINEAR`, `GCEQUAL`, `GCCOINCIDENT`, `GCCONCENTRIC`, `GCTANGENT`, `GCFIX`), `AUTOCONSTRAIN`, and named driving dimensions (`DCHORIZONTAL`, `DCVERTICAL`, `DCALIGNED`, `DCRADIUS`, `DCDIAMETER`, `DCANGULAR`) whose values may be expressions of each other, edited in the **Parameters** manager. A least-squares solve keeps every constraint true after each edit, holds what you drag exactly where you put it when possible, refuses conflicting constraints and reports the remaining degrees of freedom. Constraint glyphs and driving dimensions on the canvas (`CONSTRAINTBAR`, `DELCONSTRAINT`); a Parametric ribbon tab and menu.
- **Revision clouds** (`REVCLOUD`): rectangular, polygonal or from an object, with a settable arc length; saved to DXF/DWG as bulged polylines.
- **Bill of materials** (`BOM`) built from the block references and their attributes, refreshed in place; **balloons** (`BALLOON`) numbered to match, placed automatically round the drawing or one at a time.
- **Sheet sets** (`SHEETSET`): sheets from many drawings with numbers, titles and shared title block values; publish the set to one PDF (also `imaginecad publish`), update the drawings' title blocks, insert a sheet list. `.icadset` files open by double-click.
- **Dimension text keeps clear** of other text and of the part when placed or when its geometry moves; `DIMARRANGE` tidies existing drawings.
- **Image to CAD:** spur gears with true involute teeth and gear data, compression springs with spring data; hidden lines, centre lines and section hatching recognised when tracing.

### Changed
- Redesigned icon set: one visual language throughout (neutral line work, blue for what a tool creates, red for what it removes, violet for AI), crisp at every size; new icons for the new tools.

## [0.3.0] - 2026-10-16

### Added
- **Image to CAD, validated part reconstruction for any drawing:** besides shafts and plates, the AI can describe round parts (rings, washers, bushes, flanges with hub and bolt circle, wheels and pulleys with rim, web, hub, V-grooves and bore keyway), drawn as an end view and a hatched section, and any other part as an exact profile (lines, arcs, circles, polylines in projected views) with its dimensions. Every description is checked against the values written on the sketch, for missing sizes, and (for profiles) by measuring each dimension on its own geometry; a description with problems gets one correction round, and one that still does not check out is not drawn: the sketch is traced instead and the reason reported.
- **Image to CAD, exact slanted and angular dimensions:** slanted lengths, angles between edges, arc and fillet radii and fillet tangency are enforced by a Levenberg–Marquardt refinement after the per-axis solve; angle labels become angular dimensions.
- **Image to CAD, multiple views:** projected views are recognised and their edges snapped to each other, so hand-drawn misalignment between views does not carry over.
- **Image to CAD, review overlay:** the sketch can be laid under the result, faded, on layer `AI-SKETCH-IMAGE` (not plotted).
- **Menu bar** with every command and its alias (File, Edit, View, Insert, Format, Draw, Dimension, Modify, Tools, AI, Help).
- **Annotation scale** picker on the status bar; `ZOOM` accepts a factor (`2X`, `0.5X`).
- **AI Chat** panel (`AICHAT`, AI tab): describe changes in plain language and the AI edits the drawing — add, change, move, copy, rotate, scale, mirror and delete objects, layers, title block, sheet and dimension style. Parts created by Image to CAD are regenerated from their parameters. Each answer is one undo step; questions are answered without changes. Works with every AI engine.
- `imaginecad ask DRAWING "CHANGE" -o OUT`: the same from the command line.
- Drawings remember the parametric part they were generated from (`.icad`).
- **Dimensions:** ordinate (`DIMORDINATE`) and arc-length (`DIMARC`) dimensions; `QDIM` quick dimensioning (continuous, staggered, baseline, ordinate, radius, diameter); **associative dimensions** that follow moved, stretched or resized geometry (`DIMREASSOCIATE`, `DIMDISASSOCIATE`). Ordinate dimensions are native DXF/DWG dimensions; arc-length dimensions round-trip through DXF/DWG.
- **Engineering symbols:** geometric tolerance feature control frames (`TOLERANCE`, ISO 1101: all 14 characteristics, Ø zone, material modifiers, datums, composite rows, optional leader), datum feature symbols (`DATUM`), surface texture symbols (`SURFFINISH`, ISO 1302/21920) and weld symbols (`WELD`, ISO 2553). Dialogs with live preview; double-click to edit. Feature control frames are native DXF/DWG tolerance entities and plain ones from other programs are read.
- **Block attributes:** `ATTDEF`, values per block reference (prompted on insert, `ATTEDIT`, double-click, Properties palette), `ATTEXPORT` to CSV; standard DXF/DWG attribute entities.
- **Dynamic title blocks:** any block with attributes can be the title block, filled automatically from the title fields; the built-in title block can be copied as an editable block.
- **Fields** in text (`{{TITLE}}`, `{{SCALE}}`, `{{DATE}}`, `{{FILENAME}}`, `{{LENGTH:id}}`, `{{AREA:id}}`, `{{COUNT:block}}`…) with an Insert field menu.
- **Tables:** `TABLE` (empty, from CSV/spreadsheet data, or a parts list from block attributes), merged title rows, alignment, column widths, a cell editor (`TABLEDIT`, double-click); fields in cells.
- **Multileader styles:** `MLEADER`, `MLEADERSTYLE` with arrowhead type, sizes, landing and box/balloon frames; *Standard* and *Balloon* styles built in.
- **Text styles:** `STYLE` with any installed TrueType/OpenType font, width factor, oblique angle and fixed height; styled text is drawn from font outlines on screen and in PDF/SVG; DXF/DWG STYLE table and text style references.
- **Spell check:** `SPELL` over text, multiline text, leaders, dimension text, attributes and table cells, with suggestions, Change All, Ignore All and a personal dictionary.
- **Regions:** `BOUNDARY`, `REGION` (from closed objects or chained lines and arcs), `UNION`, `SUBTRACT`, `INTERSECT`, and `MASSPROP` (area, perimeter, centroid, moments and products of inertia, radii of gyration, principal moments).
- **Object snap tracking** (`OTRACK`, F11) with acquired points, alignment and intersection tracking; **selection cycling** of overlapping objects.
- **Layouts:** paper-space layouts with their own paper, title block and plot style; viewports (`MVIEW`) with scale, view centre, size and lock; layout tabs with rename, page setup, plot and delete.
- **Plot styles:** monochrome, colour, grayscale and screened, per sheet or layout.
- **Batch plot:** `PUBLISH` to one multi-page PDF (each page its own paper size) or one PDF per sheet.
- **External references:** `XATTACH`, the `XREF` palette (reload, re-path, open, bind, detach), automatic reload on open, `NAME|LAYER` dependent layers; written to DXF/DWG as external blocks.
- **Raster images in DXF/DWG:** IMAGE entities with image definitions, size and fade, read and written.
- **Compact ribbon:** split buttons with variant lists (Circle ▾, Arc ▾, Dimension ▾, Trim ▾, Fillet ▾, Array ▾…), icon tool columns and panel-title menus (Draw ▾, Modify ▾, Annotation ▾, Layers ▾). On narrower windows panels collapse from the right into single buttons that drop the whole panel down as a menu, and any that still do not fit go under **More**; the ribbon never scrolls.
- The ImagineCAD logo opens the application menu.
- DXF/DWG: objects without a DXF 2000 equivalent are written as their graphics plus ImagineCAD extended data, so other programs show them and ImagineCAD reads them back exactly.

### Changed
- Ctrl+Z / Ctrl+Y in a text box undo the typing, not the drawing.
- The AI reads a sharper, contrast-stretched copy of the sketch (up to 2400 px), which reads handwritten digits markedly better.
- Windows: ImagineCAD is listed under *Open with* for `.icad`, `.dxf`, `.dwg` and sketch images. Linux: `.icad` MIME type registered; several selected files open at once.

### Fixed
- Opening a drawing from Finder (double-click, *Open With*, Dock) failed with "ImagineCAD cannot open files in the DWG/DXF Drawing format".
- Reopening a DXF or DWG changed its annotation scale (the sheet was re-fitted to extents that included an exported border), making dimension text far too big and overlapping.
- Image to CAD: obtuse corners and a straight run meeting a fillet were fitted as one large arc; a label next to an outline edge took that edge away as a dimension line; a printed sheet border was traced as the part.

## [0.2.0] - 2026-10-15

### Added
- **DWG**: open, edit and save DWG files using bundled GNU LibreDWG tools (patched so dimensions survive), with the ODA File Converter used automatically when installed. File associations for .dwg/.dxf/.icad.
- **DXF 2000** writer (default) with native LWPOLYLINE, ELLIPSE, SPLINE, MTEXT, HATCH, XLINE/RAY, INSERT/BLOCK, lineweights and angular dimensions; DXF reader for blocks, bulged polylines, hatches, ellipses, splines and leaders. R12 remains available.
- Entities: ellipse, spline, point, construction line, ray, multiline text, leader, hatch, block reference, raster image; per-entity lineweight.
- Commands: ELLIPSE, SPLINE, POINT, XLINE, RAY, MTEXT, TEXTEDIT, LEADER, HATCH, BLOCK, INSERT, IMAGEATTACH, DIMANGULAR, DIMBASELINE, DIMCONTINUE, STRETCH, BREAK, BREAKATPOINT, JOIN, LENGTHEN, ARRAYPOLAR, DIVIDE, MEASURE, AREA, ID, LIST, OVERKILL, PURGE, OOPS, QSELECT, UNITS, ZOOM Previous, layer tools (LAYISO, LAYUNISO, LAYOFF, LAYON, LAYLCK, LAYULK, LAYMCUR); Blocks palette; polar tracking (F10); double-click text editing; insertion object snap.
- AI: connect **any provider's API key** with automatic provider and vision-model detection (OpenAI, Anthropic, Gemini, OpenRouter, Groq, Mistral, xAI, Together, Hugging Face, NVIDIA, Fireworks, custom endpoints); first-use setup dialog; secure key storage in the OS keychain.
- PDF: Flate compression and embedded images; SVG: embedded images.
- Licensing: GPL-3.0 with commercial dual licensing by Proteen Das; CLA, trademark policy, NOTICE, third-party license report bundled with installers; legal notices in About and `--version`.
- New ImagineCAD logo.
- Release pipeline: LibreDWG built and DWG-tested on every platform, optional macOS signing/notarisation and Windows code signing, SHA-256 checksums.
- AI Settings and first-use setup: a searchable **model dropdown** listing the provider's image-capable models with **FREE** / **PAID** badges and a *Free only* filter. The list loads when the dialog opens, and an empty model is filled with the provider's best choice.
- Image to CAD **part reconstruction**: shafts (steps, keyways, chamfers) and plates (holes) recognised by the AI are drawn exactly from the written dimensions, with a scale-aware dimension layout.

### Fixed
- AI requests failing with `404` when the selected model cannot read images (for example a text-only OpenRouter model). Only vision models are offered now, and a text-only model is replaced automatically by a vision model from the same provider.
- AI error messages now include the provider's explanation, not only the status code.
- OpenRouter `402` (not enough credit): requests cap the reply length so a small balance suffices, keys without credit get a free (`:free`) vision model at setup, and a 402 switches to a free vision model automatically.
- AI answers that were not strict JSON (code fences, comments, trailing commas) or were cut off at the length limit are now read; a cut-off answer keeps its complete values and is reported, and an unusable answer is saved for troubleshooting instead of being dumped into the dialog.
- Windows release build of LibreDWG failing on newer MinGW GCC warnings; macOS/Windows LibreDWG builds no longer need autotools.
- Arrows and some symbols showed as empty boxes in the interface.
- Slender parts were placed on sheets too small for their dimensions.
- Linux build failure (`ACCOUNT` unused) under `-D warnings`.

## [0.1.0] - 2026-10-15

### Added
- Desktop editor (Rust, egui) with ribbon, command line with aliases and autocomplete, properties palette, status bar toggles, drawing tabs, model and layout views.
- Draw, modify and annotation commands: LINE, PLINE, CIRCLE, ARC, RECTANG, POLYGON, TEXT, MOVE, COPY, ROTATE, SCALE, MIRROR, OFFSET, TRIM, EXTEND, FILLET, CHAMFER, ARRAYRECT, EXPLODE, ERASE, MATCHPROP, DIMLINEAR, DIMALIGNED, DIMRADIUS, DIMDIAMETER, CENTERMARK, DIST, ZOOM.
- Object snaps, ortho, grid snap, typed absolute/relative/polar coordinates, direct distance entry, grips, window/crossing selection, undo/redo, clipboard.
- Layers with colour, linetype, lineweight, visibility, lock and plot flags; mechanical layer template.
- Sheet setup with paper sizes, ISO scales, grid reference zones and an ISO 7200-style title block with projection symbol.
- Export to PDF (true scale), SVG, DXF R12 (with dimension entities), DWG via free converters; DXF/DWG import; native `.icad` format.
- Image to CAD: computer-vision vectorisation, free local AI (Ollama, OpenAI-compatible servers) and Tesseract dimension reading, dimension-driven least-squares rectification, review layers.
- Headless CLI: `imaginecad convert` and `imaginecad export`.
- Installers: macOS `.dmg`, Windows setup `.exe` and portable `.zip`, Linux `.deb` and `.tar.gz`; GitHub Actions CI and release workflows.
