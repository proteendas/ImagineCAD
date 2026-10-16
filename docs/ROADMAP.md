# Roadmap

ImagineCAD's goal is complete day-to-day 2D mechanical drafting at the level
of mature commercial CAD suites, plus AI sketch conversion. This page is an
honest map of what exists and what is next.

## 0.3 (current)

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

## Next (0.4)

- [ ] Geometric and dimensional constraints (parametric drafting)
- [ ] Revision clouds, balloons and an automatic bill of materials
- [ ] Image to CAD: section hatching and hidden lines recognised when tracing; gears and springs as part types
- [ ] Sheet sets
- [ ] Dimension text that moves itself clear of other annotation when edited

## Later

- [ ] Native DWG reader/writer in Rust (removing the external converter)
- [ ] 3D modelling (solids, extrude/revolve, 2D views from 3D)
- [ ] Scripting / plugin API, localisation

## Principles

- Interoperability first: DWG/DXF fidelity is tested in CI.
- AI is optional and provider-neutral; local free models are always supported.
- ImagineCAD follows common CAD conventions (command names, layers, linetypes,
  DXF/DWG) but has its own name, logo and artwork, and uses no third-party trademarks.
