// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

fn main() {
    // Embed the application icon in the Windows executable.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("ProductName", "ImagineCAD");
        res.set("FileDescription", "ImagineCAD");
        let _ = res.compile();
    }
}
