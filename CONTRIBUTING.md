# Contributing to ImagineCAD

Thanks for helping! ImagineCAD is owned by Proteen Das and dual-licensed
(GPL-3.0 and commercial). **All contributions require agreeing to the
[Contributor License Agreement](CLA.md)**: add the CLA line to your pull
request description.

## Workflow

1. Fork and create a branch: `git checkout -b feature/my-change`.
2. Make your change with tests.
3. Run the same checks as CI:
   ```bash
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
4. Open a pull request describing *what* changed and *why*; add a screenshot
   for UI changes (`IMAGINECAD_SCREENSHOT=shot.png`, see [docs/BUILDING.md](docs/BUILDING.md)).

## Where things go

| Change | Location |
|---|---|
| Geometry, entity behaviour, edit operations | `crates/icad-core` |
| Import/export formats | `crates/icad-io` |
| Sketch-to-CAD pipeline, AI engines | `crates/icad-vision` |
| UI, commands, dialogs | `crates/imaginecad` |

Libraries must not depend on the GUI crate, and `icad-core` stays free of I/O.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Adding a command

1. Implement `Command` in `crates/imaginecad/src/commands.rs`: `prompt`,
   `wants`, the input handlers you need, and `preview` for rubber-banding.
   Call `cx.checkpoint()` before modifying the document.
2. Register the name and alias in `create()` and in `COMMANDS` (autocomplete and help).
3. Add a ribbon button in `ribbon.rs` (and an icon in `icons.rs` if needed).
4. Add a test using the `Harness` in `commands.rs`.
5. Document it in [docs/USER_GUIDE.md](docs/USER_GUIDE.md).

## Adding an entity type

Add a `Shape` variant in `icad-core/src/entity.rs` and implement `prims`,
`curves`, `snap_points`, `grips`/`set_grip`, `transform`; then DXF read/write
in `icad-io/src/dxf.rs` and fields in the properties palette.

## Conventions

- Angles are radians, counter-clockwise; Y points up; arcs run CCW from `start` to `end`.
- Drawing units are millimetres by convention; annotation sizes in `DimStyle` are paper millimetres scaled by the sheet scale.
- Prefer small pure functions with unit tests for geometry.
- Error messages shown to users say what to do next.
- AI features stay optional and provider-neutral: any provider's API key must work, and free local engines (Ollama, OpenAI-compatible servers) must always remain supported.
- Do not use third-party product names, logos or trademarks in the UI, code
  comments or documentation; describe behaviour in neutral CAD terms.
- `rustfmt.toml` sets a 140-column width.
- Every source file starts with:
  ```rust
  // SPDX-License-Identifier: GPL-3.0-only
  // Copyright (C) 2026 Proteen Das
  ```

## Reporting bugs

Please include your OS, ImagineCAD version (`imaginecad --version`), steps to
reproduce, and if relevant the drawing or sketch image (and the Image to CAD
log/warnings).
