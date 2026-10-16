# Roadmap

ImagineCAD's goal is complete day-to-day 2D mechanical drafting at the level
of mature commercial CAD suites, plus AI sketch conversion. This page is an
honest map of what exists and what is next.

## 0.7 (current)

Everything in 0.6, plus:

**DWG:** a DWG reader written in Rust: DWG 2000, 2004, 2010, 2013 and 2018
drawings open without a converter, with every 2D drafting entity and
native dimensions.

**Languages:** Italian and Portuguese.

## 0.6

Everything in 0.5, plus:

**Automation:** command scripts (`SCRIPT`, `imaginecad script`) and plugins in
any programming language that add commands and edit the drawing through the
same operations as AI Chat.

**Languages:** the interface in English, German, French and Spanish.

**Views:** revolved sections of turned parts.

**Blocks:** rotation and array parameters for dynamic blocks.

## 0.5

Everything in 0.4, plus:

**Blocks:** dynamic blocks with stretch, flip, visibility and lookup
parameters, edited in a block editor and set per reference.

**Views:** section views from a cutting line (`SECTIONVIEW`) and enlarged
detail views (`DETAILVIEW`).

**Output:** plot stamps; PDF/A-2b for archiving.

**Interoperability:** constraints and dynamic blocks round-trip through DXF
and DWG.

**AI:** threads (shaft steps and tapped holes) and surface finish recognised;
sketches of several parts drawn side by side with balloons and a parts list.

## 0.4

Everything in 0.3, plus:

**Parametric drafting:** geometric constraints (horizontal, vertical,
parallel, perpendicular, collinear, equal, coincident, concentric, tangent,
fix) and named driving dimensions with expressions, solved after every edit;
AUTOCONSTRAIN; the Parameters manager.

**Markup:** revision clouds, an automatic bill of materials and numbered
balloons.

**Sheets:** sheet sets with numbering, shared title block values, set
publishing and sheet lists.

**Annotation:** dimension text that moves itself clear of other text and of
the part.

**AI:** gears (true involute teeth) and compression springs as part types;
hidden lines, centre lines and section hatching recognised when tracing.

**Interface:** a redesigned, consistent icon set.

## 0.3

Everything in 0.2, plus:

**Annotation:** ordinate and arc-length dimensions, `QDIM`, associative
dimensions; GD&T feature control frames, datum, surface-texture and weld
symbols; block attributes and dynamic title blocks; multileader styles,
tables and fields; TrueType text styles and spell check.

**Precision & geometry:** object snap tracking, selection cycling, `BOUNDARY`,
`REGION` with union/subtract/intersect and `MASSPROP`.

**Output:** multiple paper layouts with viewports, plot styles (monochrome,
colour, grayscale, screened), batch plot to multi-page PDF, annotation scale
picker.

**References:** external references (`XATTACH`, `XREF`) and raster images,
round-tripped through DXF/DWG.

**Interface:** a ribbon that never scrolls (panels collapse into dropdowns), a
full menu bar listing every command, opening drawings from Finder, Explorer and
Linux file managers.

**AI:** Image to CAD enforces horizontal, vertical, slanted and angular
dimensions, radii and fillet tangency; keeps projected views aligned; overlays
the sketch for review; rebuilds shafts, round parts (rings, bushes, flanges,
pulleys), plates and any other profile from the written dimensions, validated
against those values and against its own geometry before it is drawn. AI Chat
edits drawings in plain language.

## 0.2

Core drafting, modify and dimension commands, layers, blocks, hatching, DWG via
bundled LibreDWG, DXF 2000/R12, PDF/SVG, Image to CAD with any provider's key,
installers for macOS, Windows and Linux.

## Next (0.8)

- [ ] DWG reader: DWG 2007 files, ImagineCAD's extended data and paper-space layouts, so the converter is needed only for writing
- [ ] Interface languages with their own scripts (Hindi, Chinese, Japanese) and translated dialogs
- [ ] Broken-out sections and aligned section views from a stepped cutting line
- [ ] Plugin commands with live previews

## Later

- [ ] Native DWG writer in Rust (removing the external converter entirely)
- [ ] 3D modelling (solids, extrude/revolve, 2D views from 3D)

## Principles

- Interoperability first: DWG/DXF fidelity is tested in CI.
- AI is optional and provider-neutral; local free models are always supported.
- ImagineCAD follows common CAD conventions (command names, layers, linetypes,
  DXF/DWG) but has its own name, logo and artwork, and uses no third-party trademarks.
