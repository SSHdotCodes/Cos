//! Accessibility-first computer control, ported from the Swift runtime.
//! Resolves a fresh accessibility tree before every indexed action so stale UI
//! references are never reused.

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFHash, CFRelease, CFTypeRef};
use core_foundation_sys::string::CFStringRef;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSArray, NSString};
use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

#[allow(non_camel_case_types)]
type pid_t = i32;
#[allow(non_camel_case_types)]
type AXError = i32;
const AX_ERROR_SUCCESS: AXError = 0;
#[allow(non_camel_case_types)]
type CGKeyCode = u16;
#[allow(non_camel_case_types)]
type CGEventFlags = u64;
const K_CG_HID_EVENT_TAP: u32 = 0;
const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
const K_CG_SCROLL_EVENT_UNIT_PIXEL: u32 = 0;
const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x100000;
const K_CG_EVENT_FLAG_MASK_SHIFT: CGEventFlags = 0x20000;
const K_CG_EVENT_FLAG_MASK_ALTERNATE: CGEventFlags = 0x80000;
const K_CG_EVENT_FLAG_MASK_CONTROL: CGEventFlags = 0x40000;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "HIServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation_sys::dictionary::CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: pid_t) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: CFTypeRef) -> AXError;
    fn AXUIElementPerformAction(element: CFTypeRef, action: CFStringRef) -> AXError;
    fn AXUIElementCopyActionNames(element: CFTypeRef, names: *mut core_foundation_sys::array::CFArrayRef) -> AXError;

}

/// The kAX* names are CFSTR() macros in the modern SDK (not exported
/// symbols), so they are constructed directly with their literal values.
mod ax {
    use core_foundation::string::CFString;
    pub fn children() -> CFString { CFString::new("AXChildren") }
    pub fn role() -> CFString { CFString::new("AXRole") }
    pub fn subrole() -> CFString { CFString::new("AXSubrole") }
    pub fn title() -> CFString { CFString::new("AXTitle") }
    pub fn description() -> CFString { CFString::new("AXDescription") }
    pub fn help() -> CFString { CFString::new("AXHelp") }
    pub fn identifier() -> CFString { CFString::new("AXIdentifier") }
    pub fn enabled() -> CFString { CFString::new("AXEnabled") }
    pub fn focused() -> CFString { CFString::new("AXFocused") }
    pub fn value() -> CFString { CFString::new("AXValue") }
    pub fn press() -> CFString { CFString::new("AXPress") }
    pub fn text_field_role() -> CFString { CFString::new("AXTextField") }
    pub fn secure_text_field_subrole() -> CFString { CFString::new("AXSecureTextField") }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateKeyboardEvent(source: *const std::ffi::c_void, virtual_key: CGKeyCode, key_down: bool) -> CFTypeRef;
    fn CGEventKeyboardSetUnicodeString(event: CFTypeRef, string_length: usize, unicode_string: *const u16);
    fn CGEventCreateMouseEvent(
        source: *const std::ffi::c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CFTypeRef;
    fn CGEventCreateScrollWheelEvent(
        source: *const std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        ...
    ) -> CFTypeRef;
    fn CGEventPost(tap: u32, event: CFTypeRef);
    fn CGEventSetFlags(event: CFTypeRef, flags: CGEventFlags);
}

pub struct CosComputerUseAccess;

impl CosComputerUseAccess {
    pub fn is_granted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn request() -> bool {
        unsafe {
            let key = CFString::new("AXTrustedCheckOptionPrompt");
            let value = CFBoolean::true_value();
            let options = crate::cf::dictionary_from_raw_pairs(&[(
                key.as_concrete_TypeRef() as CFTypeRef,
                value.as_concrete_TypeRef() as CFTypeRef,
            )]);
            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
        }
    }
}

#[derive(Debug)]
pub enum ComputerUseError {
    MissingArgument(&'static str),
    ApplicationNotFound(String),
    StateUnavailable(String),
    StaleElement(i64),
    ActionFailed(&'static str, AXError),
    UnsupportedTool(String),
    UnknownKey(String),
    UnknownDirection(String),
    TextTooLarge,
    EventCreationFailed,
}

impl fmt::Display for ComputerUseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument(name) => write!(f, "Computer Use requires {name}."),
            Self::ApplicationNotFound(app) => write!(
                f,
                "Could not find or launch {app}. Use computer_list_apps to inspect available applications."
            ),
            Self::StateUnavailable(app) => write!(f, "Could not read the accessibility tree for {app}."),
            Self::StaleElement(index) => write!(
                f,
                "Element {index} is unavailable. Fetch a fresh computer_get_state and use its current index."
            ),
            Self::ActionFailed(action, error) => {
                write!(f, "The {action} accessibility action failed with code {error}.")
            }
            Self::UnsupportedTool(name) => write!(f, "Unknown Computer Use tool: {name}."),
            Self::UnknownKey(key) => write!(f, "Unknown keyboard key or shortcut: {key}."),
            Self::UnknownDirection(direction) => {
                write!(f, "Computer Use can scroll up or down, not {direction}.")
            }
            Self::TextTooLarge => write!(f, "Computer Use text is limited to 100 KB per action."),
            Self::EventCreationFailed => write!(f, "macOS could not create the requested input event."),
        }
    }
}

impl std::error::Error for ComputerUseError {}

type CUResult<T> = Result<T, ComputerUseError>;

struct Node {
    element: CFType,
    depth: usize,
}

struct RunningApp {
    name: String,
    bundle_id: String,
    pid: pid_t,
    handle: Retained<NSRunningApplication>,
}

pub struct CosComputerUseRuntime;

impl CosComputerUseRuntime {
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        name: &str,
        app: Option<&str>,
        element_index: Option<i64>,
        x: Option<f64>,
        y: Option<f64>,
        text: Option<&str>,
        key: Option<&str>,
        direction: Option<&str>,
        pages: Option<i64>,
    ) -> String {
        match Self::execute_inner(name, app, element_index, x, y, text, key, direction, pages) {
            Ok(result) => result,
            Err(error) => error.to_string(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_inner(
        name: &str,
        app: Option<&str>,
        element_index: Option<i64>,
        x: Option<f64>,
        y: Option<f64>,
        text: Option<&str>,
        key: Option<&str>,
        direction: Option<&str>,
        pages: Option<i64>,
    ) -> CUResult<String> {
        if name == "computer_list_apps" {
            return Ok(list_apps());
        }
        if let Some(message) = accessibility_permission_message() {
            return Ok(message);
        }
        let app = match app.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => value.to_string(),
            None => return Err(ComputerUseError::MissingArgument("app")),
        };
        let running = resolve_application(&app)?;
        activate(&running);

        match name {
            "computer_get_state" => state(&running),
            "computer_click" => {
                if let Some(index) = element_index {
                    press_element(index, &running)
                } else {
                    match (x, y) {
                        (Some(x), Some(y)) => click_coordinate(x, y),
                        _ => Err(ComputerUseError::MissingArgument("element_index or x/y")),
                    }
                }
            }
            "computer_set_value" => match (element_index, text) {
                (Some(index), Some(text)) => set_value(text, index, &running),
                _ => Err(ComputerUseError::MissingArgument("element_index and text")),
            },
            "computer_type_text" => {
                let Some(text) = text else {
                    return Err(ComputerUseError::MissingArgument("text"));
                };
                if let Some(index) = element_index {
                    focus_element(index, &running)?;
                }
                type_unicode(text)?;
                Ok(format!("Typed {} UTF-8 bytes into {}.", text.len(), running.name))
            }
            "computer_press_key" => {
                let Some(key) = key else {
                    return Err(ComputerUseError::MissingArgument("key"));
                };
                press_key(key)?;
                Ok(format!("Pressed {key} in {}.", running.name))
            }
            "computer_scroll" => {
                let direction = direction.unwrap_or("down");
                let pages = pages.unwrap_or(1).clamp(1, 8);
                scroll(direction, pages)?;
                Ok(format!("Scrolled {direction} {pages} page(s) in {}.", running.name))
            }
            _ => Err(ComputerUseError::UnsupportedTool(name.to_string())),
        }
    }
}

fn accessibility_permission_message() -> Option<String> {
    if CosComputerUseAccess::is_granted() {
        return None;
    }
    let _ = CosComputerUseAccess::request();
    Some(
        "macOS requires Accessibility access for computer control. The system permission panel has been opened; enable Cos in System Settings → Privacy & Security → Accessibility, then retry the task."
            .to_string(),
    )
}

fn list_apps() -> String {
    let workspace = NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    let mut regular: Vec<Retained<NSRunningApplication>> = apps
        .iter()
        .filter(|app| {
            app.activationPolicy() == objc2_app_kit::NSApplicationActivationPolicy::Regular && !app.isTerminated()
        })
        .collect();
    regular.sort_by_key(|app| {
        app.localizedName()
            .map(|name| name.to_string().to_lowercase())
            .unwrap_or_default()
    });
    if regular.is_empty() {
        return "No regular foreground applications are running.".into();
    }
    regular
        .iter()
        .take(160)
        .map(|app| {
            format!(
                "{} | {} | pid {}",
                app.localizedName().map(|v| v.to_string()).unwrap_or_else(|| "Unnamed".into()),
                app.bundleIdentifier().map(|v| v.to_string()).unwrap_or_else(|| "no.bundle.id".into()),
                app.processIdentifier()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve_application(identifier: &str) -> CUResult<RunningApp> {
    if let Some(app) = find_running_application(identifier) {
        return Ok(app);
    }
    let mut command = std::process::Command::new("/usr/bin/open");
    if identifier.starts_with('/') {
        command.arg(identifier);
    } else {
        command.arg("-a").arg(identifier);
    }
    let _ = command.status();
    std::thread::sleep(std::time::Duration::from_millis(650));
    find_running_application(identifier)
        .ok_or_else(|| ComputerUseError::ApplicationNotFound(identifier.to_string()))
}

fn find_running_application(identifier: &str) -> Option<RunningApp> {
    let lowered = identifier.to_lowercase();
    let workspace = NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    let alive: Vec<Retained<NSRunningApplication>> = apps.iter().filter(|app| !app.isTerminated()).collect();
    let matches = |app: &Retained<NSRunningApplication>, exact: bool| -> bool {
        let bundle = app.bundleIdentifier().map(|v| v.to_string().to_lowercase());
        let name = app.localizedName().map(|v| v.to_string().to_lowercase());
        let path = app.bundleURL().and_then(|url| url.path()).map(|v| v.to_string().to_lowercase());
        if exact {
            bundle.as_deref() == Some(lowered.as_str())
                || name.as_deref() == Some(lowered.as_str())
                || path.as_deref() == Some(lowered.as_str())
        } else {
            name.map(|value| value.starts_with(&lowered)).unwrap_or(false)
        }
    };
    let found = alive
        .iter()
        .find(|app| matches(app, true))
        .or_else(|| alive.iter().find(|app| matches(app, false)))?;
    Some(RunningApp {
        name: found.localizedName().map(|v| v.to_string()).unwrap_or_else(|| "application".into()),
        bundle_id: found.bundleIdentifier().map(|v| v.to_string()).unwrap_or_else(|| "unknown bundle".into()),
        pid: found.processIdentifier(),
        handle: found.clone(),
    })
}

fn activate(app: &RunningApp) {
    let _ = app.handle.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    std::thread::sleep(std::time::Duration::from_millis(120));
}

fn attribute(element: &CFType, name: &CFString) -> Option<CFType> {
    unsafe {
        let mut value: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeValue(element.as_concrete_TypeRef(), name.as_concrete_TypeRef(), &mut value) != AX_ERROR_SUCCESS {
            return None;
        }
        if value.is_null() {
            return None;
        }
        Some(CFType::wrap_under_create_rule(value))
    }
}

fn string_attribute(element: &CFType, name: &CFString) -> Option<String> {
    let value = attribute(element, name)?;
    if let Some(string) = value.downcast::<CFString>() {
        return Some(cleaned(&string.to_string()));
    }
    if let Some(number) = value.downcast::<CFNumber>() {
        if let Some(integer) = number.to_i64() {
            return Some(integer.to_string());
        }
        if let Some(float) = number.to_f64() {
            return Some(float.to_string());
        }
    }
    None
}

fn bool_attribute(element: &CFType, name: &CFString) -> Option<bool> {
    let value = attribute(element, name)?;
    if let Some(boolean) = value.downcast::<CFBoolean>() {
        return Some(boolean.into());
    }
    value.downcast::<CFNumber>().and_then(|number| number.to_i64()).map(|value| value != 0)
}

fn nodes(app: &RunningApp) -> CUResult<Vec<Node>> {
    unsafe {
        let root_ref = AXUIElementCreateApplication(app.pid);
        if root_ref.is_null() {
            return Err(ComputerUseError::StateUnavailable(app.name.clone()));
        }
        let root = CFType::wrap_under_create_rule(root_ref);
        let mut result: Vec<Node> = Vec::new();
        let mut visited: HashSet<usize> = HashSet::new();

        fn walk(
            element: &CFType,
            depth: usize,
            result: &mut Vec<Node>,
            visited: &mut HashSet<usize>,
        ) {
            unsafe {
                if depth > 9 || result.len() >= 650 {
                    return;
                }
                let hash = CFHash(element.as_concrete_TypeRef());
                if !visited.insert(hash) {
                    return;
                }
                result.push(Node { element: element.clone(), depth });
                let Some(children_value) = attribute(element, &ax::children()) else {
                    return;
                };
                let Some(children) = children_value.downcast::<CFArray>() else {
                    return;
                };
                for child_ref in children.iter() {
                    let child = CFType::wrap_under_get_rule(*child_ref as CFTypeRef);
                    walk(&child, depth + 1, result, visited);
                }
            }
        }

        walk(&root, 0, &mut result, &mut visited);
        if result.is_empty() {
            return Err(ComputerUseError::StateUnavailable(app.name.clone()));
        }
        Ok(result)
    }
}

fn state(app: &RunningApp) -> CUResult<String> {
    let elements = nodes(app)?;
    let mut lines = vec![
        format!("Application: {} ({})", app.name, app.bundle_id),
        "Fresh accessibility tree — use these element_index values only until the next action:".to_string(),
    ];
    for (index, node) in elements.iter().enumerate() {
        lines.push(describe(&node.element, index, node.depth));
        if lines.join("\n").len() > 56_000 {
            lines.push("… accessibility tree truncated".into());
            break;
        }
    }
    Ok(lines.join("\n"))
}

fn describe(element: &CFType, index: usize, depth: usize) -> String {
    unsafe {
        let role = string_attribute(element, &ax::role()).unwrap_or_else(|| "element".into());
        let subrole = string_attribute(element, &ax::subrole());
        let title = string_attribute(element, &ax::title());
        let description = string_attribute(element, &ax::description());
        let help = string_attribute(element, &ax::help());
        let identifier = string_attribute(element, &ax::identifier());
        let enabled = bool_attribute(element, &ax::enabled());
        let focused = bool_attribute(element, &ax::focused());
        let is_secure = role == "AXTextField" && subrole.as_deref() == Some("AXSecureTextField");
        let value = if is_secure {
            Some("••••".to_string())
        } else {
            string_attribute(element, &ax::value())
        };
        let mut names_ref: core_foundation_sys::array::CFArrayRef = std::ptr::null();
        let actions: Vec<String> =
            if AXUIElementCopyActionNames(element.as_concrete_TypeRef(), &mut names_ref) == AX_ERROR_SUCCESS
                && !names_ref.is_null()
            {
                let array: CFArray = CFArray::wrap_under_create_rule(names_ref);
                array
                    .iter()
                    .filter_map(|item| {
                        let value = CFType::wrap_under_get_rule(*item as CFTypeRef);
                        value.downcast::<CFString>().map(|s| s.to_string())
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let title = title.filter(|value| !value.is_empty());
        let description = description.filter(|value| !value.is_empty());
        let help = help.filter(|value| !value.is_empty());
        let identifier = identifier.filter(|value| !value.is_empty());
        let value = value.filter(|value| !value.is_empty());
        let subrole = subrole.filter(|value| !value.is_empty());
        let mut fields: Vec<String> = vec![format!("[{index}]"), role.replace("AX", "")];
        if let Some(subrole) = &subrole {
            fields.push(format!("subrole={}", quoted(&subrole.replace("AX", ""))));
        }
        if let Some(title) = &title {
            fields.push(format!("title={}", quoted(title)));
        }
        if let Some(description) = &description {
            if Some(description) != title.as_ref() {
                fields.push(format!("description={}", quoted(description)));
            }
        }
        if let Some(value) = &value {
            if Some(value) != title.as_ref() {
                fields.push(format!("value={}", quoted(value)));
            }
        }
        if let Some(identifier) = &identifier {
            fields.push(format!("id={}", quoted(identifier)));
        }
        if let Some(help) = &help {
            if Some(help) != description.as_ref() {
                fields.push(format!("help={}", quoted(help)));
            }
        }
        if enabled == Some(false) {
            fields.push("disabled".into());
        }
        if focused == Some(true) {
            fields.push("focused".into());
        }
        if !actions.is_empty() {
            let joined = actions
                .iter()
                .map(|action| action.replace("AX", ""))
                .collect::<Vec<_>>()
                .join(",");
            fields.push(format!("actions={joined}"));
        }
        format!("{}{}", "  ".repeat(depth.min(8)), fields.join(" "))
    }
}

fn press_element(index: i64, app: &RunningApp) -> CUResult<String> {
    let elements = nodes(app)?;
    let Some(node) = usize::try_from(index).ok().and_then(|index| elements.get(index)) else {
        return Err(ComputerUseError::StaleElement(index));
    };
    unsafe {
        let action = ax::press();
        let result = AXUIElementPerformAction(node.element.as_concrete_TypeRef(), action.as_concrete_TypeRef());
        if result != AX_ERROR_SUCCESS {
            return Err(ComputerUseError::ActionFailed("press", result));
        }
    }
    Ok(format!(
        "Pressed element {index} in {}. Fetch a fresh computer_get_state before the next indexed action.",
        app.name
    ))
}

fn focus_element(index: i64, app: &RunningApp) -> CUResult<()> {
    let elements = nodes(app)?;
    let Some(node) = usize::try_from(index).ok().and_then(|index| elements.get(index)) else {
        return Err(ComputerUseError::StaleElement(index));
    };
    unsafe {
        let attribute = ax::focused();
        let result = AXUIElementSetAttributeValue(
            node.element.as_concrete_TypeRef(),
            attribute.as_concrete_TypeRef(),
            CFBoolean::true_value().as_concrete_TypeRef() as CFTypeRef,
        );
        if result != AX_ERROR_SUCCESS {
            return Err(ComputerUseError::ActionFailed("focus", result));
        }
    }
    Ok(())
}

fn set_value(text: &str, index: i64, app: &RunningApp) -> CUResult<String> {
    if text.len() > 100_000 {
        return Err(ComputerUseError::TextTooLarge);
    }
    let elements = nodes(app)?;
    let Some(node) = usize::try_from(index).ok().and_then(|index| elements.get(index)) else {
        return Err(ComputerUseError::StaleElement(index));
    };
    unsafe {
        let attribute = ax::focused();
        let _ = AXUIElementSetAttributeValue(
            node.element.as_concrete_TypeRef(),
            attribute.as_concrete_TypeRef(),
            CFBoolean::true_value().as_concrete_TypeRef() as CFTypeRef,
        );
        let value = CFString::new(text);
        let attribute = ax::value();
        let result = AXUIElementSetAttributeValue(
            node.element.as_concrete_TypeRef(),
            attribute.as_concrete_TypeRef(),
            value.as_concrete_TypeRef() as CFTypeRef,
        );
        if result != AX_ERROR_SUCCESS {
            type_unicode(text)?;
        }
    }
    Ok(format!(
        "Set element {index} to {} UTF-8 bytes in {}. Fetch a fresh computer_get_state before the next indexed action.",
        text.len(),
        app.name
    ))
}

fn click_coordinate(x: f64, y: f64) -> CUResult<String> {
    unsafe {
        let point = CGPoint { x, y };
        let down = CGEventCreateMouseEvent(
            std::ptr::null(),
            K_CG_EVENT_LEFT_MOUSE_DOWN,
            point,
            K_CG_MOUSE_BUTTON_LEFT,
        );
        let up = CGEventCreateMouseEvent(std::ptr::null(), K_CG_EVENT_LEFT_MOUSE_UP, point, K_CG_MOUSE_BUTTON_LEFT);
        if down.is_null() || up.is_null() {
            if !down.is_null() {
                CFRelease(down);
            }
            if !up.is_null() {
                CFRelease(up);
            }
            return Err(ComputerUseError::EventCreationFailed);
        }
        CGEventPost(K_CG_HID_EVENT_TAP, down);
        CGEventPost(K_CG_HID_EVENT_TAP, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(format!("Clicked screen coordinate ({}, {}).", x as i64, y as i64))
}

fn type_unicode(text: &str) -> CUResult<()> {
    if text.len() > 100_000 {
        return Err(ComputerUseError::TextTooLarge);
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        for chunk in units.chunks(20) {
            let down = CGEventCreateKeyboardEvent(std::ptr::null(), 0, true);
            let up = CGEventCreateKeyboardEvent(std::ptr::null(), 0, false);
            if down.is_null() || up.is_null() {
                if !down.is_null() {
                    CFRelease(down);
                }
                if !up.is_null() {
                    CFRelease(up);
                }
                return Err(ComputerUseError::EventCreationFailed);
            }
            CGEventKeyboardSetUnicodeString(down, chunk.len(), chunk.as_ptr());
            CGEventKeyboardSetUnicodeString(up, chunk.len(), chunk.as_ptr());
            CGEventPost(K_CG_HID_EVENT_TAP, down);
            CGEventPost(K_CG_HID_EVENT_TAP, up);
            CFRelease(down);
            CFRelease(up);
        }
    }
    Ok(())
}

fn press_key(shortcut: &str) -> CUResult<()> {
    let parts: Vec<String> = shortcut.to_lowercase().split('+').map(str::to_string).collect();
    let Some(key_name) = parts.last() else {
        return Err(ComputerUseError::UnknownKey(shortcut.to_string()));
    };
    let Some(&key_code) = key_codes().get(key_name.as_str()) else {
        return Err(ComputerUseError::UnknownKey(shortcut.to_string()));
    };
    let mut flags: CGEventFlags = 0;
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        match modifier.as_str() {
            "command" | "cmd" | "super" => flags |= K_CG_EVENT_FLAG_MASK_COMMAND,
            "shift" => flags |= K_CG_EVENT_FLAG_MASK_SHIFT,
            "option" | "alt" => flags |= K_CG_EVENT_FLAG_MASK_ALTERNATE,
            "control" | "ctrl" => flags |= K_CG_EVENT_FLAG_MASK_CONTROL,
            _ => return Err(ComputerUseError::UnknownKey(shortcut.to_string())),
        }
    }
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null(), key_code, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null(), key_code, false);
        if down.is_null() || up.is_null() {
            if !down.is_null() {
                CFRelease(down);
            }
            if !up.is_null() {
                CFRelease(up);
            }
            return Err(ComputerUseError::EventCreationFailed);
        }
        CGEventSetFlags(down, flags);
        CGEventSetFlags(up, flags);
        CGEventPost(K_CG_HID_EVENT_TAP, down);
        CGEventPost(K_CG_HID_EVENT_TAP, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(())
}

fn scroll(direction: &str, pages: i64) -> CUResult<()> {
    let count = pages.clamp(1, 8) as i32;
    let delta: i32 = match direction.to_lowercase().as_str() {
        "up" | "u" => 720 * count,
        "down" | "d" => -720 * count,
        _ => return Err(ComputerUseError::UnknownDirection(direction.to_string())),
    };
    unsafe {
        let event = CGEventCreateScrollWheelEvent(std::ptr::null(), K_CG_SCROLL_EVENT_UNIT_PIXEL, 1, delta);
        if event.is_null() {
            return Err(ComputerUseError::EventCreationFailed);
        }
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event);
    }
    Ok(())
}

static WHITESPACE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\s+").unwrap());

fn cleaned(value: &str) -> String {
    WHITESPACE.replace_all(value, " ").chars().take(260).collect()
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", cleaned(value).replace('"', "'"))
}

fn key_codes() -> &'static std::collections::HashMap<&'static str, CGKeyCode> {
    static KEY_CODES: LazyLock<std::collections::HashMap<&'static str, CGKeyCode>> = LazyLock::new(|| {
        [
            ("a", 0), ("s", 1), ("d", 2), ("f", 3), ("h", 4), ("g", 5), ("z", 6), ("x", 7),
            ("c", 8), ("v", 9), ("b", 11), ("q", 12), ("w", 13), ("e", 14), ("r", 15), ("y", 16),
            ("t", 17), ("1", 18), ("2", 19), ("3", 20), ("4", 21), ("6", 22), ("5", 23), ("=", 24),
            ("9", 25), ("7", 26), ("-", 27), ("8", 28), ("0", 29), ("]", 30), ("o", 31), ("u", 32),
            ("[", 33), ("i", 34), ("p", 35), ("return", 36), ("enter", 36), ("l", 37), ("j", 38),
            ("'", 39), ("k", 40), (";", 41), ("\\", 42), (",", 43), ("/", 44), ("n", 45), ("m", 46),
            (".", 47), ("tab", 48), ("space", 49), ("delete", 51), ("backspace", 51), ("escape", 53),
            ("esc", 53), ("left", 123), ("right", 124), ("down", 125), ("up", 126), ("forwarddelete", 117),
        ]
        .into_iter()
        .collect()
    });
    &KEY_CODES
}

#[allow(unused)]
fn _unused(_: NSArray<NSString>) {}
