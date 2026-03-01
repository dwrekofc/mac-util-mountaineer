//! Native macOS dialog helpers using NSAlert with accessory views.
//!
//! All unsafe ObjC calls are isolated in this module. Dialogs run modal
//! on the main thread via NSAlert::runModal, which blocks until dismissed
//! but allows the GPUI system event pump to continue processing events.

use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::CStr;
use std::path::{Path, PathBuf};

/// Result from the "Add Favorite" dialog.
pub struct AddFavoriteInput {
    pub share_name: String,
    pub tb_host: String,
    pub fallback_host: String,
    pub username: String,
    /// None means "same as share_name".
    pub remote_share: Option<String>,
}

/// Result from the "Remove Favorite" confirmation dialog.
pub struct RemoveFavoriteChoice {
    pub confirmed: bool,
    pub cleanup: bool,
}

/// Result from the "Add Alias" dialog.
pub struct AddAliasInput {
    pub alias_name: String,
}

// --- ObjC geometry structs ---

#[repr(C)]
#[derive(Copy, Clone)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

// --- ObjC helpers ---

/// Create an NSString from a Rust &str.
unsafe fn nsstring(s: &str) -> *mut Object {
    let ns: *mut Object = msg_send![class!(NSString), alloc];
    msg_send![ns, initWithBytes:s.as_ptr() length:s.len() encoding:4u64]
}

/// Extract the string value from an NSTextField.
unsafe fn get_field_string(field: *mut Object) -> String {
    let value: *mut Object = msg_send![field, stringValue];
    let utf8: *const i8 = msg_send![value, UTF8String];
    unsafe { CStr::from_ptr(utf8) }
        .to_string_lossy()
        .into_owned()
}

/// Create a non-editable label NSTextField.
unsafe fn make_label(text: &str, frame: NSRect) -> *mut Object {
    let label: *mut Object = msg_send![class!(NSTextField), alloc];
    let label: *mut Object = msg_send![label, initWithFrame: frame];
    let _: () = msg_send![label, setStringValue: unsafe { nsstring(text) }];
    let _: () = msg_send![label, setBezeled: false];
    let _: () = msg_send![label, setDrawsBackground: false];
    let _: () = msg_send![label, setEditable: false];
    let _: () = msg_send![label, setSelectable: false];
    label
}

/// Create an editable NSTextField with placeholder text.
unsafe fn make_text_field(placeholder: &str, frame: NSRect) -> *mut Object {
    let field: *mut Object = msg_send![class!(NSTextField), alloc];
    let field: *mut Object = msg_send![field, initWithFrame: frame];
    let _: () = msg_send![field, setPlaceholderString: unsafe { nsstring(placeholder) }];
    field
}

/// Show a native macOS form dialog to collect "Add Favorite" fields.
///
/// Uses NSAlert with an accessory view containing labeled text fields.
/// Returns `None` if the user clicked Cancel.
///
/// # Safety
/// Must be called from the main thread (AppKit requirement).
pub fn show_add_favorite_dialog() -> Option<AddFavoriteInput> {
    unsafe {
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText: nsstring("Add Favorite")];
        let _: () = msg_send![alert, setInformativeText:
            nsstring("Enter the details for the new network share.")];
        // NSAlertStyleInformational = 1
        let _: () = msg_send![alert, setAlertStyle: 1i64];

        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Add")];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Cancel")];

        // Layout constants
        let field_width: f64 = 300.0;
        let field_height: f64 = 24.0;
        let label_height: f64 = 17.0;
        let gap: f64 = 2.0;
        let spacing: f64 = 8.0;
        let pair_height = label_height + gap + field_height;
        let num_fields: usize = 5;
        let total_height = (pair_height + spacing) * num_fields as f64;

        // Container view
        let frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: field_width,
                height: total_height,
            },
        };
        let container: *mut Object = msg_send![class!(NSView), alloc];
        let container: *mut Object = msg_send![container, initWithFrame: frame];

        let labels = [
            "Share Name:",
            "Thunderbolt Host:",
            "Fallback Host:",
            "Username:",
            "Remote Share Name (optional):",
        ];
        let placeholders = [
            "e.g. CORE",
            "e.g. 10.0.0.1",
            "e.g. 192.168.1.1",
            "e.g. admin",
            "defaults to share name",
        ];

        let mut fields: Vec<*mut Object> = Vec::new();

        for (i, (label_text, placeholder)) in labels.iter().zip(placeholders.iter()).enumerate() {
            let y = total_height - (i as f64 + 1.0) * (pair_height + spacing) + spacing;

            let label = make_label(
                label_text,
                NSRect {
                    origin: NSPoint {
                        x: 0.0,
                        y: y + field_height + gap,
                    },
                    size: NSSize {
                        width: field_width,
                        height: label_height,
                    },
                },
            );
            let _: () = msg_send![container, addSubview: label];

            let field = make_text_field(
                placeholder,
                NSRect {
                    origin: NSPoint { x: 0.0, y },
                    size: NSSize {
                        width: field_width,
                        height: field_height,
                    },
                },
            );
            let _: () = msg_send![container, addSubview: field];
            fields.push(field);
        }

        let _: () = msg_send![alert, setAccessoryView: container];
        let _: () = msg_send![alert, layout];

        // Focus the first field
        let window: *mut Object = msg_send![alert, window];
        let _: () = msg_send![window, makeFirstResponder: fields[0]];

        // Run modal — blocks until user dismisses
        let response: i64 = msg_send![alert, runModal];

        // NSAlertFirstButtonReturn = 1000
        if response != 1000 {
            return None;
        }

        let share_name = get_field_string(fields[0]);
        let tb_host = get_field_string(fields[1]);
        let fallback_host = get_field_string(fields[2]);
        let username = get_field_string(fields[3]);
        let remote_share_raw = get_field_string(fields[4]);

        let remote_share = if remote_share_raw.trim().is_empty() {
            None
        } else {
            Some(remote_share_raw)
        };

        Some(AddFavoriteInput {
            share_name,
            tb_host,
            fallback_host,
            username,
            remote_share,
        })
    }
}

/// Show a native macOS confirmation dialog for removing a favorite.
///
/// Displays the share name, affected alias count, and offers
/// a cleanup checkbox (default: checked).
///
/// # Safety
/// Must be called from the main thread (AppKit requirement).
pub fn show_remove_favorite_dialog(
    share_name: &str,
    affected_alias_count: usize,
) -> RemoveFavoriteChoice {
    unsafe {
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText:
            nsstring(&format!("Remove '{}'?", share_name))];

        let info = if affected_alias_count > 0 {
            format!(
                "This will remove '{}' from your favorites.\n\n\
                 {} alias(es) reference this share and will need to be \
                 updated or removed separately.",
                share_name, affected_alias_count
            )
        } else {
            format!("This will remove '{}' from your favorites.", share_name)
        };
        let _: () = msg_send![alert, setInformativeText: nsstring(&info)];
        // NSAlertStyleCritical = 2
        let _: () = msg_send![alert, setAlertStyle: 2i64];

        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Remove")];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Cancel")];

        // Cleanup checkbox as accessory view
        let checkbox_frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 300.0,
                height: 24.0,
            },
        };
        let checkbox: *mut Object = msg_send![class!(NSButton), alloc];
        let checkbox: *mut Object = msg_send![checkbox, initWithFrame: checkbox_frame];
        // NSSwitchButton = 3
        let _: () = msg_send![checkbox, setButtonType: 3i64];
        let _: () = msg_send![checkbox, setTitle:
            nsstring("Also unmount and remove mount point symlink")];
        // Default: checked (NSControlStateValueOn = 1)
        let _: () = msg_send![checkbox, setState: 1i64];
        let _: () = msg_send![alert, setAccessoryView: checkbox];

        let response: i64 = msg_send![alert, runModal];
        let cleanup_state: i64 = msg_send![checkbox, state];

        RemoveFavoriteChoice {
            confirmed: response == 1000, // NSAlertFirstButtonReturn
            cleanup: cleanup_state == 1,
        }
    }
}

/// Show a simple error alert with OK button.
pub fn show_error_dialog(title: &str, message: &str) {
    unsafe {
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText: nsstring(title)];
        let _: () = msg_send![alert, setInformativeText: nsstring(message)];
        // NSAlertStyleCritical = 2
        let _: () = msg_send![alert, setAlertStyle: 2i64];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("OK")];
        let _: () = msg_send![alert, runModal];
    }
}

// --- Alias management dialogs (spec 16) ---

/// Show a share selection dialog with a dropdown (NSPopUpButton).
///
/// Returns the selected share name, or `None` if cancelled.
pub fn show_select_share_dialog(shares: &[String]) -> Option<String> {
    unsafe {
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText: nsstring("Select Share")];
        let _: () = msg_send![alert, setInformativeText:
            nsstring("Choose which share to browse for the alias target folder.")];
        // NSAlertStyleInformational = 1
        let _: () = msg_send![alert, setAlertStyle: 1i64];

        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Browse...")];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Cancel")];

        // NSPopUpButton dropdown
        let frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 300.0,
                height: 28.0,
            },
        };
        let popup: *mut Object = msg_send![class!(NSPopUpButton), alloc];
        let popup: *mut Object = msg_send![popup, initWithFrame:frame pullsDown:false];

        for share in shares {
            let _: () = msg_send![popup, addItemWithTitle: nsstring(share)];
        }

        let _: () = msg_send![alert, setAccessoryView: popup];
        let _: () = msg_send![alert, layout];

        let response: i64 = msg_send![alert, runModal];
        // NSAlertFirstButtonReturn = 1000
        if response != 1000 {
            return None;
        }

        let idx: i64 = msg_send![popup, indexOfSelectedItem];
        if idx >= 0 && (idx as usize) < shares.len() {
            Some(shares[idx as usize].clone())
        } else {
            None
        }
    }
}

/// Show a native macOS folder picker (NSOpenPanel) rooted at the given path.
///
/// Returns the selected folder path, or `None` if cancelled.
pub fn show_folder_picker(root_path: &Path) -> Option<PathBuf> {
    unsafe {
        let panel: *mut Object = msg_send![class!(NSOpenPanel), openPanel];
        let _: () = msg_send![panel, setCanChooseDirectories: true];
        let _: () = msg_send![panel, setCanChooseFiles: false];
        let _: () = msg_send![panel, setAllowsMultipleSelection: false];
        let _: () = msg_send![panel, setMessage:
            nsstring("Select a folder for the alias target")];
        let _: () = msg_send![panel, setPrompt: nsstring("Select")];

        // Set initial directory via NSURL
        let path_str = format!("file://{}", root_path.display());
        let url: *mut Object = msg_send![class!(NSURL), URLWithString: nsstring(&path_str)];
        let _: () = msg_send![panel, setDirectoryURL: url];

        // NSModalResponseOK = 1
        let response: i64 = msg_send![panel, runModal];
        if response != 1 {
            return None;
        }

        let url: *mut Object = msg_send![panel, URL];
        if url.is_null() {
            return None;
        }
        let path_obj: *mut Object = msg_send![url, path];
        if path_obj.is_null() {
            return None;
        }
        let utf8: *const i8 = msg_send![path_obj, UTF8String];
        let path_str = CStr::from_ptr(utf8).to_string_lossy().into_owned();
        Some(PathBuf::from(path_str))
    }
}

/// Show a dialog to name an alias after a folder has been selected.
///
/// Displays the share name and target subpath (read-only). The user enters an alias name.
/// Returns `None` if cancelled.
pub fn show_add_alias_dialog(share_name: &str, target_subpath: &str) -> Option<AddAliasInput> {
    unsafe {
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText: nsstring("Create Alias")];

        let info = format!(
            "Create an alias for folder '{}' in share '{}'.\n\n\
             The alias will be a symlink at ~/Shares/Links/<name>.",
            if target_subpath.is_empty() {
                "(root)"
            } else {
                target_subpath
            },
            share_name
        );
        let _: () = msg_send![alert, setInformativeText: nsstring(&info)];
        // NSAlertStyleInformational = 1
        let _: () = msg_send![alert, setAlertStyle: 1i64];

        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Create")];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Cancel")];

        // Accessory view: label + text field for alias name
        let field_width: f64 = 300.0;
        let field_height: f64 = 24.0;
        let label_height: f64 = 17.0;
        let total_height = label_height + 2.0 + field_height;

        let frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: field_width,
                height: total_height,
            },
        };
        let container: *mut Object = msg_send![class!(NSView), alloc];
        let container: *mut Object = msg_send![container, initWithFrame: frame];

        let label = make_label(
            "Alias Name:",
            NSRect {
                origin: NSPoint {
                    x: 0.0,
                    y: field_height + 2.0,
                },
                size: NSSize {
                    width: field_width,
                    height: label_height,
                },
            },
        );
        let _: () = msg_send![container, addSubview: label];

        let name_field = make_text_field(
            "e.g. projects",
            NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: field_width,
                    height: field_height,
                },
            },
        );
        let _: () = msg_send![container, addSubview: name_field];

        let _: () = msg_send![alert, setAccessoryView: container];
        let _: () = msg_send![alert, layout];

        let window: *mut Object = msg_send![alert, window];
        let _: () = msg_send![window, makeFirstResponder: name_field];

        let response: i64 = msg_send![alert, runModal];
        // NSAlertFirstButtonReturn = 1000
        if response != 1000 {
            return None;
        }

        let alias_name = get_field_string(name_field);
        if alias_name.trim().is_empty() {
            return None;
        }

        Some(AddAliasInput { alias_name })
    }
}

/// Show a confirmation dialog for removing an alias.
///
/// Returns `true` if the user confirmed removal.
pub fn show_remove_alias_dialog(alias_name: &str, target_path: &str) -> bool {
    unsafe {
        let alert: *mut Object = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setMessageText:
            nsstring(&format!("Remove alias '{}'?", alias_name))];
        let _: () = msg_send![alert, setInformativeText:
            nsstring(&format!("This will remove the alias symlink pointing to:\n{}", target_path))];
        // NSAlertStyleCritical = 2
        let _: () = msg_send![alert, setAlertStyle: 2i64];

        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Remove")];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("Cancel")];

        let response: i64 = msg_send![alert, runModal];
        response == 1000 // NSAlertFirstButtonReturn
    }
}
