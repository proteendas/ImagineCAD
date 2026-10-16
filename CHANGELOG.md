# Changelog

All notable changes to ImagineCAD are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
- **Compact ribbon:** split buttons with variant lists (Circle ▾, Arc ▾, Dimension ▾, Trim ▾, Fillet ▾, Array ▾…), icon tool columns and panel-title menus (Draw ▾, Modify ▾, Annotation ▾, Layers ▾), so each tab fits without scrolling.
- DXF/DWG: objects without a DXF 2000 equivalent are written as their graphics plus ImagineCAD extended data, so other programs show them and ImagineCAD reads them back exactly.

### Changed
- Ctrl+Z / Ctrl+Y in a text box undo the typing, not the drawing.

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
