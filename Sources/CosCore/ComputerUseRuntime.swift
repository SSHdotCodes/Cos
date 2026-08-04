@preconcurrency import AppKit
import ApplicationServices
import Foundation

public enum CosComputerUseAccess {
    public static var isGranted: Bool { AXIsProcessTrusted() }

    @discardableResult
    public static func request() -> Bool {
        AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
    }
}

/// Accessibility-first computer control for the native Cos harness. It keeps no
/// background browser process and resolves a fresh accessibility tree before
/// every indexed action so stale UI references are never reused.
enum CosComputerUseRuntime {
    static func execute(
        name: String,
        app: String?,
        elementIndex: Int?,
        x: Double?,
        y: Double?,
        fromX: Double? = nil,
        fromY: Double? = nil,
        toX: Double? = nil,
        toY: Double? = nil,
        text: String?,
        key: String?,
        direction: String?,
        pages: Int?,
        mouseButton: String? = nil,
        clickCount: Int? = nil,
        action: String? = nil,
        prefix: String? = nil,
        suffix: String? = nil,
        selectionType: String? = nil,
        disableDiff: Bool? = nil
    ) throws -> String {
        if name == "computer_list_apps" { return listApps() }
        if let permissionMessage = accessibilityPermissionMessage() { return permissionMessage }
        guard let app, !app.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ComputerUseError.missingArgument("app")
        }
        let running = try resolveApplication(app)
        activate(running)

        switch name {
        case "computer_get_state":
            return try state(of: running)
        case "computer_click":
            if let elementIndex { return try pressElement(elementIndex, in: running) }
            guard let x, let y else { throw ComputerUseError.missingArgument("element_index or x/y") }
            return try clickCoordinate(x: x, y: y, button: mouseButton, count: clickCount)
        case "computer_drag":
            guard let fromX, let fromY, let toX, let toY else {
                throw ComputerUseError.missingArgument("from_x, from_y, to_x, and to_y")
            }
            return try drag(fromX: fromX, fromY: fromY, toX: toX, toY: toY)
        case "computer_set_value":
            guard let elementIndex, let text else { throw ComputerUseError.missingArgument("element_index and text") }
            return try setValue(text, on: elementIndex, in: running)
        case "computer_type_text":
            guard let text else { throw ComputerUseError.missingArgument("text") }
            if let elementIndex { try focusElement(elementIndex, in: running) }
            try typeUnicode(text)
            return "Typed \(text.utf8.count) UTF-8 bytes into \(running.localizedName ?? app)."
        case "computer_press_key":
            guard let key else { throw ComputerUseError.missingArgument("key") }
            try pressKey(key)
            return "Pressed \(key) in \(running.localizedName ?? app)."
        case "computer_scroll":
            try scroll(direction: direction ?? "down", pages: pages ?? 1)
            return "Scrolled \(direction ?? "down") \(max(1, min(8, pages ?? 1))) page(s) in \(running.localizedName ?? app)."
        case "computer_secondary_action":
            guard let elementIndex, let action else { throw ComputerUseError.missingArgument("element_index and action") }
            return try performSecondaryAction(action, on: elementIndex, in: running)
        case "computer_select_text":
            guard let elementIndex, let text else { throw ComputerUseError.missingArgument("element_index and text") }
            return try selectText(text, prefix: prefix, suffix: suffix, selectionType: selectionType, on: elementIndex, in: running)
        default:
            throw ComputerUseError.unsupportedTool(name)
        }
    }

    private static func accessibilityPermissionMessage() -> String? {
        guard !CosComputerUseAccess.isGranted else { return nil }
        _ = CosComputerUseAccess.request()
        return "macOS requires Accessibility access for computer control. The system permission panel has been opened; enable Cos in System Settings → Privacy & Security → Accessibility, then retry the task."
    }

    private static func listApps() -> String {
        let apps = NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular && !$0.isTerminated }
            .sorted { ($0.localizedName ?? "").localizedCaseInsensitiveCompare($1.localizedName ?? "") == .orderedAscending }
        guard !apps.isEmpty else { return "No regular foreground applications are running." }
        return apps.prefix(160).map { app in
            "\(app.localizedName ?? "Unnamed") | \(app.bundleIdentifier ?? "no.bundle.id") | pid \(app.processIdentifier)"
        }.joined(separator: "\n")
    }

    private static func resolveApplication(_ identifier: String) throws -> NSRunningApplication {
        if let running = findRunningApplication(identifier) { return running }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        process.arguments = identifier.hasPrefix("/") ? [identifier] : ["-a", identifier]
        try? process.run()
        process.waitUntilExit()
        Thread.sleep(forTimeInterval: 0.65)
        if let running = findRunningApplication(identifier) { return running }
        throw ComputerUseError.applicationNotFound(identifier)
    }

    private static func findRunningApplication(_ identifier: String) -> NSRunningApplication? {
        let lowered = identifier.lowercased()
        let apps = NSWorkspace.shared.runningApplications.filter { !$0.isTerminated }
        return apps.first {
            $0.bundleIdentifier?.lowercased() == lowered ||
            $0.localizedName?.lowercased() == lowered ||
            $0.bundleURL?.path.lowercased() == lowered
        } ?? apps.first {
            $0.localizedName?.lowercased().hasPrefix(lowered) == true
        }
    }

    private static func activate(_ app: NSRunningApplication) {
        _ = app.activate(options: [.activateAllWindows])
        Thread.sleep(forTimeInterval: 0.12)
    }

    private struct Node {
        let element: AXUIElement
        let depth: Int
    }

    private static func nodes(for app: NSRunningApplication) throws -> [Node] {
        let root = AXUIElementCreateApplication(app.processIdentifier)
        var result: [Node] = [.init(element: root, depth: 0)]
        var cursor = 0
        var visited = Set<CFHashCode>()
        visited.insert(CFHash(root))

        // Breadth-first traversal keeps a long chat transcript from consuming
        // the entire budget before sibling controls such as its composer.
        while cursor < result.count, result.count < 2_400 {
            let node = result[cursor]
            cursor += 1
            guard node.depth < 14,
                  let children = attribute(node.element, kAXChildrenAttribute as String) as? [AXUIElement] else { continue }
            for child in children where result.count < 2_400 {
                guard visited.insert(CFHash(child)).inserted else { continue }
                result.append(.init(element: child, depth: node.depth + 1))
            }
        }
        guard !result.isEmpty else { throw ComputerUseError.stateUnavailable(app.localizedName ?? "application") }
        return result
    }

    private static func state(of app: NSRunningApplication) throws -> String {
        let elements = try nodes(for: app)
        var lines = [
            "Application: \(app.localizedName ?? "Unnamed") (\(app.bundleIdentifier ?? "unknown bundle"))",
            "Fresh accessibility state — use these element_index values only until the next action:",
        ]
        var included = Set<Int>()

        let editable = elements.indices.filter { isEditableOrFocused(elements[$0].element) }
        if !editable.isEmpty {
            lines.append("Key focused and editable controls:")
            for index in editable.prefix(80) {
                included.insert(index)
                lines.append(describe(elements[index].element, index: index, depth: 0))
            }
        }

        let navigation = elements.indices.filter {
            !included.contains($0) && isNavigationControl(elements[$0].element)
        }
        if !navigation.isEmpty {
            lines.append("Key navigation and action controls:")
            for index in navigation.prefix(240) {
                included.insert(index)
                lines.append(describe(elements[index].element, index: index, depth: 0))
            }
        }

        lines.append("Bounded accessibility outline:")
        var byteCount = lines.joined(separator: "\n").utf8.count
        for (index, node) in elements.enumerated() {
            guard !included.contains(index) else { continue }
            let line = describe(node.element, index: index, depth: node.depth)
            if byteCount + line.utf8.count + 1 > 56_000 {
                lines.append("… accessibility tree truncated")
                break
            }
            lines.append(line)
            byteCount += line.utf8.count + 1
        }
        lines.append("If a control is absent, use the app's keyboard shortcuts plus computer_type_text without an element_index, then fetch fresh state. Do not assume the app is uncontrollable from one sparse tree.")
        return lines.joined(separator: "\n")
    }

    private static func isEditableOrFocused(_ element: AXUIElement) -> Bool {
        if boolAttribute(element, kAXFocusedAttribute as String) == true { return true }
        switch stringAttribute(element, kAXRoleAttribute as String) {
        case "AXTextArea", "AXTextField", "AXSearchField", "AXComboBox": return true
        default: return false
        }
    }

    private static func isNavigationControl(_ element: AXUIElement) -> Bool {
        switch stringAttribute(element, kAXRoleAttribute as String) {
        case "AXLink", "AXButton", "AXMenuItem", "AXPopUpButton", "AXCheckBox", "AXRadioButton", "AXTab": return true
        default: return false
        }
    }

    private static func describe(_ element: AXUIElement, index: Int, depth: Int) -> String {
        let role = stringAttribute(element, kAXRoleAttribute as String) ?? "element"
        let subrole = stringAttribute(element, kAXSubroleAttribute as String)
        let title = stringAttribute(element, kAXTitleAttribute as String)
        let description = stringAttribute(element, kAXDescriptionAttribute as String)
        let help = stringAttribute(element, kAXHelpAttribute as String)
        let identifier = stringAttribute(element, kAXIdentifierAttribute as String)
        let enabled = boolAttribute(element, kAXEnabledAttribute as String)
        let focused = boolAttribute(element, kAXFocusedAttribute as String)
        let isSecure = role == (kAXTextFieldRole as String) && subrole == (kAXSecureTextFieldSubrole as String)
        let value = isSecure ? "••••" : stringAttribute(element, kAXValueAttribute as String)
        var actionsRef: CFArray?
        let actions: [String]
        if AXUIElementCopyActionNames(element, &actionsRef) == .success, let raw = actionsRef as? [String] {
            actions = raw
        } else {
            actions = []
        }
        var fields: [String] = ["[\(index)]", role.replacingOccurrences(of: "AX", with: "")]
        if let subrole, !subrole.isEmpty { fields.append("subrole=\(quoted(subrole.replacingOccurrences(of: "AX", with: "")))") }
        if let title, !title.isEmpty { fields.append("title=\(quoted(title))") }
        if let description, description != title, !description.isEmpty { fields.append("description=\(quoted(description))") }
        if let value, value != title, !value.isEmpty { fields.append("value=\(quoted(value))") }
        if let identifier, !identifier.isEmpty { fields.append("id=\(quoted(identifier))") }
        if let help, help != description, !help.isEmpty { fields.append("help=\(quoted(help))") }
        if enabled == false { fields.append("disabled") }
        if focused == true { fields.append("focused") }
        if !actions.isEmpty { fields.append("actions=\(actions.map { $0.replacingOccurrences(of: "AX", with: "") }.joined(separator: ","))") }
        return String(repeating: "  ", count: min(depth, 8)) + fields.joined(separator: " ")
    }

    private static func pressElement(_ index: Int, in app: NSRunningApplication) throws -> String {
        let elements = try nodes(for: app)
        guard elements.indices.contains(index) else { throw ComputerUseError.staleElement(index) }
        let element = elements[index].element
        let result = AXUIElementPerformAction(element, kAXPressAction as CFString)
        guard result == .success else { throw ComputerUseError.actionFailed("press", result) }
        return "Pressed element \(index) in \(app.localizedName ?? "application"). Fetch a fresh computer_get_state before the next indexed action."
    }

    private static func focusElement(_ index: Int, in app: NSRunningApplication) throws {
        let elements = try nodes(for: app)
        guard elements.indices.contains(index) else { throw ComputerUseError.staleElement(index) }
        let result = AXUIElementSetAttributeValue(elements[index].element, kAXFocusedAttribute as CFString, kCFBooleanTrue)
        guard result == .success else { throw ComputerUseError.actionFailed("focus", result) }
    }

    private static func setValue(_ text: String, on index: Int, in app: NSRunningApplication) throws -> String {
        guard text.utf8.count <= 100_000 else { throw ComputerUseError.textTooLarge }
        let elements = try nodes(for: app)
        guard elements.indices.contains(index) else { throw ComputerUseError.staleElement(index) }
        let element = elements[index].element
        _ = AXUIElementSetAttributeValue(element, kAXFocusedAttribute as CFString, kCFBooleanTrue)
        let result = AXUIElementSetAttributeValue(element, kAXValueAttribute as CFString, text as CFTypeRef)
        if result != .success {
            try typeUnicode(text)
        }
        return "Set element \(index) to \(text.utf8.count) UTF-8 bytes in \(app.localizedName ?? "application"). Fetch a fresh computer_get_state before the next indexed action."
    }

    private static func clickCoordinate(x: Double, y: Double, button: String?, count: Int?) throws -> String {
        let point = CGPoint(x: x, y: y)
        let resolved = try mouseButton(button)
        let clicks = max(1, min(3, count ?? 1))
        for click in 1...clicks {
            let types: (CGEventType, CGEventType)
            switch resolved {
            case .left: types = (.leftMouseDown, .leftMouseUp)
            case .right: types = (.rightMouseDown, .rightMouseUp)
            case .center: types = (.otherMouseDown, .otherMouseUp)
            @unknown default: types = (.leftMouseDown, .leftMouseUp)
            }
            guard let down = CGEvent(mouseEventSource: nil, mouseType: types.0, mouseCursorPosition: point, mouseButton: resolved),
                  let up = CGEvent(mouseEventSource: nil, mouseType: types.1, mouseCursorPosition: point, mouseButton: resolved) else {
                throw ComputerUseError.eventCreationFailed
            }
            down.setIntegerValueField(.mouseEventClickState, value: Int64(click))
            up.setIntegerValueField(.mouseEventClickState, value: Int64(click))
            down.post(tap: .cghidEventTap)
            up.post(tap: .cghidEventTap)
        }
        return "Clicked screen coordinate (\(Int(x)), \(Int(y))) \(clicks) time(s)."
    }

    private static func mouseButton(_ value: String?) throws -> CGMouseButton {
        switch value?.lowercased() ?? "left" {
        case "left", "l": .left
        case "right", "r": .right
        case "middle", "m", "center": .center
        default: throw ComputerUseError.unknownMouseButton(value ?? "")
        }
    }

    private static func drag(fromX: Double, fromY: Double, toX: Double, toY: Double) throws -> String {
        let start = CGPoint(x: fromX, y: fromY)
        let end = CGPoint(x: toX, y: toY)
        guard let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: start, mouseButton: .left),
              let moved = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDragged, mouseCursorPosition: end, mouseButton: .left),
              let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: end, mouseButton: .left) else {
            throw ComputerUseError.eventCreationFailed
        }
        down.post(tap: .cghidEventTap)
        moved.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
        return "Dragged from (\(Int(fromX)), \(Int(fromY))) to (\(Int(toX)), \(Int(toY)))."
    }

    private static func performSecondaryAction(_ action: String, on index: Int, in app: NSRunningApplication) throws -> String {
        let elements = try nodes(for: app)
        guard elements.indices.contains(index) else { throw ComputerUseError.staleElement(index) }
        var actionsRef: CFArray?
        guard AXUIElementCopyActionNames(elements[index].element, &actionsRef) == .success,
              let actions = actionsRef as? [String],
              let exact = actions.first(where: {
                  $0.caseInsensitiveCompare(action) == .orderedSame ||
                  $0.replacingOccurrences(of: "AX", with: "").caseInsensitiveCompare(action) == .orderedSame
              }) else {
            throw ComputerUseError.actionUnavailable(action)
        }
        let result = AXUIElementPerformAction(elements[index].element, exact as CFString)
        guard result == .success else { throw ComputerUseError.actionFailed(action, result) }
        return "Performed \(action) on element \(index). Fetch fresh computer_get_state before another indexed action."
    }

    private static func selectText(
        _ text: String,
        prefix: String?,
        suffix: String?,
        selectionType: String?,
        on index: Int,
        in app: NSRunningApplication
    ) throws -> String {
        let elements = try nodes(for: app)
        guard elements.indices.contains(index) else { throw ComputerUseError.staleElement(index) }
        let element = elements[index].element
        guard let value = rawStringAttribute(element, kAXValueAttribute as String) else {
            throw ComputerUseError.selectionUnavailable
        }
        let source = value as NSString
        var searchRange = NSRange(location: 0, length: source.length)
        var match = source.range(of: text, options: [], range: searchRange)
        while match.location != NSNotFound {
            let before = source.substring(to: match.location)
            let after = source.substring(from: NSMaxRange(match))
            let prefixMatches = prefix == nil || before.hasSuffix(prefix!)
            let suffixMatches = suffix == nil || after.hasPrefix(suffix!)
            if prefixMatches && suffixMatches { break }
            let next = NSMaxRange(match)
            guard next < source.length else { match = NSRange(location: NSNotFound, length: 0); break }
            searchRange = NSRange(location: next, length: source.length - next)
            match = source.range(of: text, options: [], range: searchRange)
        }
        guard match.location != NSNotFound else { throw ComputerUseError.textNotFound(text) }
        let selected: CFRange
        switch selectionType?.lowercased() ?? "text" {
        case "text": selected = CFRange(location: match.location, length: match.length)
        case "cursor_before": selected = CFRange(location: match.location, length: 0)
        case "cursor_after": selected = CFRange(location: NSMaxRange(match), length: 0)
        default: throw ComputerUseError.unknownSelectionType(selectionType ?? "")
        }
        var mutableRange = selected
        guard let safeRangeValue = AXValueCreate(.cfRange, &mutableRange) else { throw ComputerUseError.selectionUnavailable }
        let result = AXUIElementSetAttributeValue(element, kAXSelectedTextRangeAttribute as CFString, safeRangeValue)
        guard result == .success else { throw ComputerUseError.actionFailed("select text", result) }
        return "Selected text in element \(index)."
    }

    private static func typeUnicode(_ text: String) throws {
        guard text.utf8.count <= 100_000 else { throw ComputerUseError.textTooLarge }
        let units = Array(text.utf16)
        for start in stride(from: 0, to: units.count, by: 20) {
            let chunk = Array(units[start..<min(units.count, start + 20)])
            guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
                  let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else {
                throw ComputerUseError.eventCreationFailed
            }
            chunk.withUnsafeBufferPointer { buffer in
                guard let base = buffer.baseAddress else { return }
                down.keyboardSetUnicodeString(stringLength: buffer.count, unicodeString: base)
                up.keyboardSetUnicodeString(stringLength: buffer.count, unicodeString: base)
            }
            down.post(tap: .cghidEventTap)
            up.post(tap: .cghidEventTap)
        }
    }

    private static func pressKey(_ shortcut: String) throws {
        let parts = shortcut.lowercased().split(separator: "+").map(String.init)
        guard let keyName = parts.last, let keyCode = keyCodes[keyName] else { throw ComputerUseError.unknownKey(shortcut) }
        var flags: CGEventFlags = []
        for modifier in parts.dropLast() {
            switch modifier {
            case "command", "cmd", "super": flags.insert(.maskCommand)
            case "shift": flags.insert(.maskShift)
            case "option", "alt": flags.insert(.maskAlternate)
            case "control", "ctrl": flags.insert(.maskControl)
            default: throw ComputerUseError.unknownKey(shortcut)
            }
        }
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: keyCode, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: keyCode, keyDown: false) else {
            throw ComputerUseError.eventCreationFailed
        }
        down.flags = flags
        up.flags = flags
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
    }

    private static func scroll(direction: String, pages: Int) throws {
        let count = max(1, min(8, pages))
        let delta: Int32
        switch direction.lowercased() {
        case "up", "u": delta = 720 * Int32(count)
        case "down", "d": delta = -720 * Int32(count)
        default: throw ComputerUseError.unknownDirection(direction)
        }
        guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: delta, wheel2: 0, wheel3: 0) else {
            throw ComputerUseError.eventCreationFailed
        }
        event.post(tap: .cghidEventTap)
    }

    private static func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value
    }

    private static func stringAttribute(_ element: AXUIElement, _ name: String) -> String? {
        guard let value = attribute(element, name) else { return nil }
        if let string = value as? String { return cleaned(string) }
        if let number = value as? NSNumber { return number.stringValue }
        return nil
    }

    private static func rawStringAttribute(_ element: AXUIElement, _ name: String) -> String? {
        attribute(element, name) as? String
    }

    private static func boolAttribute(_ element: AXUIElement, _ name: String) -> Bool? {
        (attribute(element, name) as? NSNumber)?.boolValue
    }

    private static func cleaned(_ value: String) -> String {
        String(value.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression).prefix(260))
    }

    private static func quoted(_ value: String) -> String {
        "\"" + cleaned(value).replacingOccurrences(of: "\"", with: "'") + "\""
    }

    private static let keyCodes: [String: CGKeyCode] = [
        "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9,
        "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19,
        "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25, "7": 26, "-": 27, "8": 28, "0": 29,
        "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35, "return": 36, "enter": 36, "l": 37,
        "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42, ",": 43, "/": 44, "n": 45, "m": 46,
        ".": 47, "tab": 48, "space": 49, "delete": 51, "backspace": 51, "escape": 53, "esc": 53,
        "left": 123, "right": 124, "down": 125, "up": 126, "forwarddelete": 117,
    ]
}

private enum ComputerUseError: LocalizedError {
    case missingArgument(String)
    case applicationNotFound(String)
    case stateUnavailable(String)
    case staleElement(Int)
    case actionFailed(String, AXError)
    case unsupportedTool(String)
    case unknownKey(String)
    case unknownDirection(String)
    case unknownMouseButton(String)
    case actionUnavailable(String)
    case selectionUnavailable
    case textNotFound(String)
    case unknownSelectionType(String)
    case textTooLarge
    case eventCreationFailed

    var errorDescription: String? {
        switch self {
        case .missingArgument(let name): "Computer Use requires \(name)."
        case .applicationNotFound(let app): "Could not find or launch \(app). Use computer_list_apps to inspect available applications."
        case .stateUnavailable(let app): "Could not read the accessibility tree for \(app)."
        case .staleElement(let index): "Element \(index) is unavailable. Fetch a fresh computer_get_state and use its current index."
        case .actionFailed(let action, let error): "The \(action) accessibility action failed with code \(error.rawValue)."
        case .unsupportedTool(let name): "Unknown Computer Use tool: \(name)."
        case .unknownKey(let key): "Unknown keyboard key or shortcut: \(key)."
        case .unknownDirection(let direction): "Computer Use can scroll up or down, not \(direction)."
        case .unknownMouseButton(let button): "Unknown mouse button: \(button)."
        case .actionUnavailable(let action): "The element does not expose the secondary action \(action). Fetch fresh state and use one of its listed actions."
        case .selectionUnavailable: "The selected element does not expose selectable text."
        case .textNotFound(let text): "Could not find \(text) in the selected element."
        case .unknownSelectionType(let type): "Unknown selection type: \(type). Use text, cursor_before, or cursor_after."
        case .textTooLarge: "Computer Use text is limited to 100 KB per action."
        case .eventCreationFailed: "macOS could not create the requested input event."
        }
    }
}
