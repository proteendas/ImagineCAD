// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The classic menu bar: every command, grouped, with its alias. The ribbon
//! shows the common tools; these menus are the complete list (a test keeps
//! them in step with the command table).

use crate::commands::COMMANDS;
use eframe::egui::{self, Ui};

/// (label, command); a label of "-" is a separator.
type Entry = (&'static str, &'static str);

pub const MENUS: &[(&str, &[Entry])] = &[
    (
        "File",
        &[
            ("New", "NEW"),
            ("Open…", "OPEN"),
            ("Save", "SAVE"),
            ("Save As…", "SAVEAS"),
            ("Close", "CLOSE"),
            ("-", ""),
            ("Export…", "EXPORT"),
            ("Export DWG…", "EXPORT DWG"),
            ("Export DXF…", "EXPORT DXF"),
            ("Export PDF…", "EXPORT PDF"),
            ("Export SVG…", "EXPORT SVG"),
            ("-", ""),
            ("Page Setup…", "PAGESETUP"),
            ("Plot to PDF…", "PLOT"),
            ("Publish (batch plot)…", "PUBLISH"),
            ("Plot Stamp…", "PLOTSTAMP"),
            ("Sheet Set Manager…", "SHEETSET"),
            ("-", ""),
            ("Exit", "EXIT"),
        ],
    ),
    (
        "Edit",
        &[
            ("Undo", "UNDO"),
            ("Redo", "REDO"),
            ("-", ""),
            ("Cut", "CUTCLIP"),
            ("Copy", "COPYCLIP"),
            ("Paste", "PASTECLIP"),
            ("-", ""),
            ("Erase", "ERASE"),
            ("Oops (restore erased)", "OOPS"),
            ("Select All", "SELECTALL"),
            ("Quick Select…", "QSELECT"),
            ("-", ""),
            ("Delete Duplicates (Overkill)", "OVERKILL"),
            ("Purge Unused", "PURGE"),
        ],
    ),
    (
        "View",
        &[
            ("Zoom Extents", "ZOOM E"),
            ("Zoom Window", "ZOOM W"),
            ("Zoom Previous", "ZOOM P"),
            ("Zoom Whole Sheet", "ZOOM A"),
            ("Pan", "PANINFO"),
            ("Regenerate", "REGEN"),
            ("-", ""),
            ("Grid", "GRID"),
            ("Lineweights", "LWT"),
            ("Sheet && Title Block", "SHEET"),
            ("Paper Layout", "LAYOUT"),
            ("Model", "MODEL"),
            ("-", ""),
            ("Parametric Ribbon", "RIBBON PARAMETRIC"),
            ("Properties Palette", "PROPERTIES"),
            ("AI Chat Panel", "AICHAT"),
        ],
    ),
    (
        "Insert",
        &[
            ("Block…", "INSERT"),
            ("Blocks Palette", "BLOCKS"),
            ("Create Block", "BLOCK"),
            ("Block Editor (dynamic blocks)", "BEDIT"),
            ("Stretch Parameter", "BSTRETCH"),
            ("Flip Parameter", "BFLIP"),
            ("Rotation Parameter", "BROTATE"),
            ("Array Parameter", "BARRAY"),
            ("Visibility State", "BVSTATE"),
            ("Lookup Table…", "BLOOKUP"),
            ("Save Block", "BSAVE"),
            ("Close Block Editor", "BCLOSE"),
            ("-", ""),
            ("Define Attributes…", "ATTDEF"),
            ("Edit Attributes", "ATTEDIT"),
            ("Export Attributes (CSV)…", "ATTEXPORT"),
            ("-", ""),
            ("Attach Drawing (External Reference)…", "XATTACH"),
            ("External References…", "XREF"),
            ("Attach Image…", "IMAGEATTACH"),
            ("-", ""),
            ("Table…", "TABLE"),
            ("Bill of Materials", "BOM"),
            ("Image to CAD (AI)…", "IMAGETOCAD"),
            ("-", ""),
            ("New Layout", "LAYOUTNEW"),
            ("Layout Viewport", "MVIEW"),
        ],
    ),
    (
        "Format",
        &[
            ("Layers…", "LAYER"),
            ("Text Style…", "STYLE"),
            ("Dimension Style…", "DIMSTYLE"),
            ("Multileader Style…", "MLEADERSTYLE"),
            ("Units…", "UNITS"),
            ("Linetype Scale", "LTSCALE"),
            ("-", ""),
            ("Title Block…", "TITLEBLOCK"),
            ("Page Setup…", "PAGESETUP"),
        ],
    ),
    (
        "Draw",
        &[
            ("Line", "LINE"),
            ("Polyline", "PLINE"),
            ("Construction Line", "XLINE"),
            ("Ray", "RAY"),
            ("-", ""),
            ("Circle", "CIRCLE"),
            ("Circle, Diameter", "CIRCLE D"),
            ("Circle, 2 Points", "CIRCLE 2P"),
            ("Circle, 3 Points", "CIRCLE 3P"),
            ("Arc, 3 Points", "ARC"),
            ("Arc, Center Start End", "ARC C"),
            ("Ellipse", "ELLIPSE"),
            ("Spline", "SPLINE"),
            ("-", ""),
            ("Rectangle", "RECTANG"),
            ("Polygon", "POLYGON"),
            ("Point", "POINT"),
            ("Divide", "DIVIDE"),
            ("Measure", "MEASURE"),
            ("-", ""),
            ("Hatch", "HATCH"),
            ("Boundary", "BOUNDARY"),
            ("Region", "REGION"),
            ("-", ""),
            ("Multiline Text", "MTEXT"),
            ("Single Line Text", "TEXT"),
            ("-", ""),
            ("Revision Cloud", "REVCLOUD"),
            ("-", ""),
            ("Section View", "SECTIONVIEW"),
            ("Detail View", "DETAILVIEW"),
            ("Broken-out Section", "BROKENOUT"),
        ],
    ),
    (
        "Dimension",
        &[
            ("Quick Dimension", "QDIM"),
            ("-", ""),
            ("Linear", "DIMLINEAR"),
            ("Aligned", "DIMALIGNED"),
            ("Angular", "DIMANGULAR"),
            ("Arc Length", "DIMARC"),
            ("Radius", "DIMRADIUS"),
            ("Diameter", "DIMDIAMETER"),
            ("Ordinate", "DIMORDINATE"),
            ("Baseline", "DIMBASELINE"),
            ("Continue", "DIMCONTINUE"),
            ("-", ""),
            ("Multileader", "MLEADER"),
            ("Leader", "LEADER"),
            ("Center Mark", "CENTERMARK"),
            ("Balloons", "BALLOON"),
            ("-", ""),
            ("Geometric Tolerance…", "TOLERANCE"),
            ("Datum Symbol", "DATUM"),
            ("Surface Texture", "SURFFINISH"),
            ("Weld Symbol", "WELD"),
            ("-", ""),
            ("Tidy Dimension Text", "DIMARRANGE"),
            ("Reassociate", "DIMREASSOCIATE"),
            ("Disassociate", "DIMDISASSOCIATE"),
            ("Dimension Style…", "DIMSTYLE"),
        ],
    ),
    (
        "Modify",
        &[
            ("Properties", "PROPERTIES"),
            ("Match Properties", "MATCHPROP"),
            ("-", ""),
            ("Move", "MOVE"),
            ("Copy", "COPY"),
            ("Rotate", "ROTATE"),
            ("Scale", "SCALE"),
            ("Mirror", "MIRROR"),
            ("Offset", "OFFSET"),
            ("Stretch", "STRETCH"),
            ("Lengthen", "LENGTHEN"),
            ("-", ""),
            ("Trim", "TRIM"),
            ("Extend", "EXTEND"),
            ("Break", "BREAK"),
            ("Break at Point", "BREAKATPOINT"),
            ("Join", "JOIN"),
            ("Fillet", "FILLET"),
            ("Chamfer", "CHAMFER"),
            ("-", ""),
            ("Rectangular Array", "ARRAYRECT"),
            ("Polar Array", "ARRAYPOLAR"),
            ("Explode", "EXPLODE"),
            ("-", ""),
            ("Edit Text", "TEXTEDIT"),
            ("Edit Table", "TABLEDIT"),
            ("-", ""),
            ("Union (regions)", "UNION"),
            ("Subtract (regions)", "SUBTRACT"),
            ("Intersect (regions)", "INTERSECT"),
        ],
    ),
    (
        "Tools",
        &[
            ("Distance", "DIST"),
            ("Area", "AREA"),
            ("Point Coordinates", "ID"),
            ("List", "LIST"),
            ("Mass Properties", "MASSPROP"),
            ("Check Spelling", "SPELL"),
            ("-", ""),
            ("Run Script…", "SCRIPT"),
            ("Plugins…", "PLUGINS"),
            ("Language…", "LANGUAGE"),
            ("-", ""),
            ("Isolate Layers", "LAYISO"),
            ("Unisolate Layers", "LAYUNISO"),
            ("Layer Off (pick)", "LAYOFF"),
            ("All Layers On", "LAYON"),
            ("Lock Layer (pick)", "LAYLCK"),
            ("Unlock Layer (pick)", "LAYULK"),
            ("Make Layer Current (pick)", "LAYMCUR"),
            ("-", ""),
            ("Snap", "SNAP"),
            ("Ortho", "ORTHO"),
            ("Polar Tracking", "POLAR"),
            ("Object Snap", "OSNAP"),
            ("Object Snap Tracking", "OTRACK"),
            ("Dynamic Input", "DYNMODE"),
        ],
    ),
    (
        "AI",
        &[
            ("Image to CAD…", "IMAGETOCAD"),
            ("AI Chat", "AICHAT"),
            ("-", ""),
            ("Connect API Key…", "AISETUP"),
            ("AI Settings…", "AISETTINGS"),
            ("Tips for Sketches", "AIHELP"),
        ],
    ),
    (
        "Parametric",
        &[
            ("Auto Constrain", "AUTOCONSTRAIN"),
            ("-", ""),
            ("Horizontal", "GCHORIZONTAL"),
            ("Vertical", "GCVERTICAL"),
            ("Parallel", "GCPARALLEL"),
            ("Perpendicular", "GCPERPENDICULAR"),
            ("Collinear", "GCCOLLINEAR"),
            ("Equal", "GCEQUAL"),
            ("Coincident", "GCCOINCIDENT"),
            ("Concentric", "GCCONCENTRIC"),
            ("Tangent", "GCTANGENT"),
            ("Fix", "GCFIX"),
            ("-", ""),
            ("Driving Horizontal Dimension", "DCHORIZONTAL"),
            ("Driving Vertical Dimension", "DCVERTICAL"),
            ("Driving Aligned Dimension", "DCALIGNED"),
            ("Driving Radius", "DCRADIUS"),
            ("Driving Diameter", "DCDIAMETER"),
            ("Driving Angle", "DCANGULAR"),
            ("-", ""),
            ("Parameters Manager…", "PARAMETERS"),
            ("Delete Constraints", "DELCONSTRAINT"),
            ("Show/Hide Constraints", "CONSTRAINTBAR"),
        ],
    ),
    ("Help", &[("Help", "HELP"), ("About ImagineCAD", "ABOUT")]),
];

fn alias(cmd: &str) -> &'static str {
    COMMANDS.iter().find(|(c, _)| *c == cmd).map(|(_, a)| *a).unwrap_or("")
}

/// The menu bar; chosen commands are pushed to `out`.
pub fn menu_bar(ui: &mut Ui, out: &mut Vec<String>) {
    for (name, entries) in MENUS {
        ui.menu_button(crate::i18n::tr(name), |ui| {
            ui.set_min_width(210.0);
            for (label, cmd) in *entries {
                if *label == "-" {
                    ui.separator();
                    continue;
                }
                let b = egui::Button::new(crate::i18n::tr(label).replace("&&", "&")).shortcut_text(alias(cmd));
                if ui.add(b).on_hover_text(*cmd).clicked() {
                    out.push(cmd.to_string());
                    ui.close_menu();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_in_a_menu() {
        let listed: Vec<&str> = MENUS.iter().flat_map(|m| m.1.iter()).map(|e| e.1.split(' ').next().unwrap()).collect();
        let missing: Vec<&str> = COMMANDS.iter().map(|c| c.0).filter(|c| !listed.contains(c)).collect();
        assert!(missing.is_empty(), "not in any menu: {missing:?}");
    }
}
