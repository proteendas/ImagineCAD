# Roadmap

ImagineCAD's goal is complete day-to-day 2D mechanical drafting at the level
of mature commercial CAD suites, plus AI sketch conversion. This page is an
honest map of what exists and what is next.

## 0.2 (current)

**Drafting:** line, polyline, circle, arc, rectangle, polygon, ellipse, spline,
point, construction line, ray, hatch (with island detection, 9 patterns + solid),
single-line and multiline text, leaders, blocks (define, insert, explode,
nested, palette), raster image underlays.

**Modify:** move, copy, rotate, scale, mirror, stretch, offset, trim, extend,
fillet, chamfer, break, break-at-point, join, lengthen, rectangular and polar
arrays, explode, erase/oops, match properties, divide, measure, overkill, purge.

**Dimensions:** linear, aligned, angular, radius, diameter, baseline, continue,
centre marks, dimension style, text overrides.

**Precision & selection:** typed coordinates, direct distance, nine object
snaps, polar tracking, ortho, grid snap, window/crossing, grips, quick select,
double-click editing.

**Layers:** full manager plus isolate/unisolate, off/on, lock/unlock, make current.

**Files:** DWG open/save (bundled LibreDWG), DXF 2000 and R12 read/write, PDF,
SVG, native `.icad`.

**AI:** Image to CAD with any provider's API key (auto-configured), local
Ollama/OpenAI-compatible servers, or Tesseract; dimension-exact solving.

**Distribution:** GPL-3.0 + commercial dual license; installers for macOS,
Windows and Linux built and published by CI (code signing once certificates are
added).

## Next (0.3)

- [x] Block attributes (`ATTDEF`, `ATTEDIT`) and dynamic title blocks
- [x] Multileader styles, tables (`TABLE`), fields
- [ ] Text styles with TrueType fonts; spell check
- [x] Ordinate and arc-length dimensions, `QDIM`, dimension associativity
- [x] GD&T feature control frames, surface-finish and weld symbols
- [ ] Object snap tracking, selection cycling, `BOUNDARY`, `REGION`
- [ ] Multiple layouts with viewports; plot styles; batch plot
- [ ] External references (`XREF`) and DWG attached images round-trip
- [ ] Image to CAD: enforce slanted/angular dimensions and fillet radii, multiple aligned views, overlay review

## Later

- [ ] Geometric and dimensional constraints (parametric drafting)
- [ ] Native DWG reader/writer in Rust (removing the external converter)
- [ ] Sheet sets, revision clouds, BOM and balloons
- [ ] 3D modelling (solids, extrude/revolve, 2D views from 3D)
- [ ] Scripting / plugin API, localisation

## Principles

- Interoperability first: DWG/DXF fidelity is tested in CI.
- AI is optional and provider-neutral; local free models are always supported.
- ImagineCAD follows common CAD conventions (command names, layers, linetypes,
  DXF/DWG) but has its own name, logo and artwork, and uses no third-party trademarks.
