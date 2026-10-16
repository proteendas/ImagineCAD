// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Opening drawings from Finder (double-click, "Open With", drag onto the
//! Dock icon). macOS delivers those as an "open documents" Apple event, not
//! as command-line arguments; without a handler AppKit answers it with "cannot
//! open files in the … format". The handler queues the paths and the app
//! opens them on its next frame.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{define_class, msg_send, sel, AllocAnyThread};
use objc2_foundation::NSString;
use std::path::PathBuf;
use std::sync::Mutex;

static QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// To wake the app when a request arrives while it is idle.
static CTX: std::sync::OnceLock<eframe::egui::Context> = std::sync::OnceLock::new();

pub fn set_context(ctx: &eframe::egui::Context) {
    let _ = CTX.set(ctx.clone());
    ctx.request_repaint();
}

/// Four-character Apple event codes.
const fn code(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ImagineCADOpenHandler"]
    struct OpenHandler;

    impl OpenHandler {
        /// NSApplicationWillFinishLaunchingNotification: AppKit has installed
        /// its own handlers by now, so ours replaces its "open documents" one
        /// before the launch event is delivered.
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _note: *mut AnyObject) {
            install(self);
        }

        #[unsafe(method(handleOpen:withReply:))]
        fn handle_open(&self, event: *mut AnyObject, _reply: *mut AnyObject) {
            if event.is_null() {
                return;
            }
            unsafe {
                let list: *mut AnyObject = msg_send![event, paramDescriptorForKeyword: code(b"----")];
                if list.is_null() {
                    return;
                }
                let n: isize = msg_send![list, numberOfItems];
                // A single file may come as the descriptor itself (count 0).
                let items: Vec<*mut AnyObject> =
                    if n <= 0 { vec![list] } else { (1..=n).map(|i| msg_send![list, descriptorAtIndex: i]).collect() };
                for d in items {
                    if d.is_null() {
                        continue;
                    }
                    let url: *mut AnyObject = msg_send![d, fileURLValue];
                    if url.is_null() {
                        continue;
                    }
                    let path: *mut NSString = msg_send![url, path];
                    if let Some(p) = path.as_ref() {
                        QUEUE.lock().unwrap().push(PathBuf::from(p.to_string()));
                    }
                }
                if let Some(ctx) = CTX.get() {
                    ctx.request_repaint();
                }
            }
        }
    }
);

fn install(handler: &OpenHandler) {
    unsafe {
        let Some(cls) = objc2::runtime::AnyClass::get(c"NSAppleEventManager") else { return };
        let mgr: *mut AnyObject = msg_send![cls, sharedAppleEventManager];
        if mgr.is_null() {
            return;
        }
        let s: Sel = sel!(handleOpen:withReply:);
        let _: () = msg_send![mgr, setEventHandler: handler, andSelector: s, forEventClass: code(b"aevt"), andEventID: code(b"odoc")];
    }
}

/// Register for Finder's open requests. Call before the event loop starts.
pub fn register() {
    let handler: Retained<OpenHandler> = unsafe { msg_send![OpenHandler::alloc(), init] };
    unsafe {
        let Some(cls) = objc2::runtime::AnyClass::get(c"NSNotificationCenter") else { return };
        let center: *mut AnyObject = msg_send![cls, defaultCenter];
        let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
        let none: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![center, addObserver: &*handler, selector: sel!(willFinishLaunching:), name: &*name, object: none];
    }
    // Also cover the case where launching has already finished.
    install(&handler);
    // Lives for the whole run.
    std::mem::forget(handler);
}

/// Paths Finder asked to open since the last call.
pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *QUEUE.lock().unwrap())
}
