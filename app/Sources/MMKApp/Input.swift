#if os(macOS)
import AppKit
#endif
import CMmk
import GameController

/// Menu navigation buttons (pause menu, overlays).
struct MenuInput: OptionSet {
    let rawValue: UInt16
    static let up = MenuInput(rawValue: 1)
    static let down = MenuInput(rawValue: 2)
    static let confirm = MenuInput(rawValue: 4)
    static let back = MenuInput(rawValue: 8)
    static let left = MenuInput(rawValue: 16)
    static let right = MenuInput(rawValue: 32)
    /// Shoulder buttons: L / R change tabs (and the Mods page's page), ZL / ZR switch Challenge Modes.
    static let l = MenuInput(rawValue: 64)
    static let r = MenuInput(rawValue: 128)
    static let zl = MenuInput(rawValue: 256)
    static let zr = MenuInput(rawValue: 512)
}

/// Keyboard and game controllers (Switch Pro, Joy-Con, DualSense, Xbox via GameController) →
/// engine button mask, using the bindings from Settings. On macOS only keys pressed while
/// `window` is the key window count, so typing in other windows (Settings) doesn't move the
/// player. On iOS hardware keyboards are read through GameController (`GCKeyboard`, the same
/// bindings mapped to HID codes) and the on-screen controls (`TouchInput`) are added in.
final class InputState {
    /// The front end has a text field open (macOS: keys go to `typed` instead of the game).
    nonisolated(unsafe) static var typing = false
    /// Typed keys waiting to be sent, one per tick (Unicode scalars; 8 erase, 10 finish, 27 cancel).
    nonisolated(unsafe) static var typed: [UInt32] = []

    private var observers: [NSObjectProtocol] = []
    private var keys = KeyBindings.load()
    private var pad = PadBindings.load()

    /// Return / keypad Enter always confirm in menus.
    private static let confirmKeys: Set<UInt16> = [36, 76]
    /// Menu-only keys (macOS key codes): Q / E are L / R, 1 / 3 are ZL / ZR.
    private static let shoulderKeys: [(UInt16, MenuInput)] = [(12, .l), (14, .r), (18, .zl), (20, .zr)]
    /// The Course Maker's keys (U, Y, Z, Q, ], [, T, E, G, M, C), swallowed like the game's.
    private static let makerKeyCodes: Set<UInt16> = [32, 16, 6, 12, 30, 33, 17, 14, 5, 46, 8]

    #if os(macOS)
    private var keysDown = Set<UInt16>()
    private var monitor: Any?
    weak var window: NSWindow?

    /// Modifier key code → its flag (modifiers arrive as flagsChanged, not keyDown).
    private static let modifierFlags: [UInt16: NSEvent.ModifierFlags] = [
        56: .shift, 60: .shift, 59: .control, 62: .control, 58: .option, 61: .option, 55: .command, 54: .command,
    ]

    init() {
        let m = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .keyUp, .flagsChanged]) { [weak self] event in
            guard let self else { return event }
            // only the game window; keep ⌘-shortcuts working
            guard event.window != nil, event.window === self.window else { return event }
            if event.type == .keyDown && event.modifierFlags.contains(.command) { return event }
            // a text field of the front end is open: keys type (Return finishes, Esc cancels,
            // Delete erases) instead of playing
            if Self.typing && event.type == .keyDown {
                switch event.keyCode {
                case 36, 76: Self.typed.append(10)
                case 51: Self.typed.append(8)
                case 53: Self.typed.append(27)
                default:
                    for u in (event.characters ?? "").unicodeScalars where u.value >= 32 && u.value != 127 {
                        Self.typed.append(u.value)
                    }
                }
                return nil
            }
            switch event.type {
            case .keyDown: self.keysDown.insert(event.keyCode)
            case .keyUp: self.keysDown.remove(event.keyCode)
            case .flagsChanged:
                if let flag = Self.modifierFlags[event.keyCode] {
                    if event.modifierFlags.contains(flag) { self.keysDown.insert(event.keyCode) } else { self.keysDown.remove(event.keyCode) }
                }
                return event
            default: break
            }
            // swallow game keys so the window doesn't beep
            let menuKey = Self.shoulderKeys.contains { $0.0 == event.keyCode }
                || Self.makerKeyCodes.contains(event.keyCode)
            return self.isGameKey(event.keyCode) || menuKey ? nil : event
        }
        monitor = m
        observeBindings()
        // keys released while another app or window is in front never arrive: forget them
        observers.append(NotificationCenter.default.addObserver(forName: NSApplication.didResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
            self?.keysDown.removeAll()
        })
        observers.append(NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: nil, queue: .main) { [weak self] note in
            if let self, note.object as? NSWindow === self.window { self.keysDown.removeAll() }
        })
    }

    deinit {
        if let monitor { NSEvent.removeMonitor(monitor) }
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    private func anyDown(_ codes: some Sequence<UInt16>) -> Bool { !keysDown.isDisjoint(with: codes) }
    #else
    init() { observeBindings() }

    deinit { observers.forEach(NotificationCenter.default.removeObserver) }

    private func anyDown(_ codes: some Sequence<UInt16>) -> Bool {
        guard let kb = GCKeyboard.coalesced?.keyboardInput else { return false }
        return codes.contains { code in
            KeyName.hid[code].map { kb.button(forKeyCode: GCKeyCode(rawValue: $0))?.isPressed == true } ?? false
        }
    }
    #endif

    private func observeBindings() {
        observers.append(NotificationCenter.default.addObserver(forName: .mmkBindingsChanged, object: nil, queue: .main) { [weak self] _ in
            self?.keys = KeyBindings.load()
            self?.pad = PadBindings.load()
        })
    }

    private func isGameKey(_ code: UInt16) -> Bool {
        Self.confirmKeys.contains(code) || keys.keys.values.contains { $0.contains(code) }
    }

    private func held(_ action: GameAction) -> Bool {
        if let codes = keys.keys[action], anyDown(codes) { return true }
        return false
    }

    private func padHeld(_ action: GameAction, _ g: GCExtendedGamepad) -> Bool {
        (pad.buttons[action] ?? []).contains { $0.input(g)?.isPressed == true }
    }

    /// Engine buttons. The pause action is reported as MMK_BTN_START (the shell consumes it).
    var current: MmkInput { current(skipping: nil) }

    /// Engine buttons without controller `skipping` (its index in `padButtons`).
    func current(skipping: Int?) -> MmkInput {
        var b: UInt32 = 0
        if held(.left) { b |= UInt32(MMK_BTN_LEFT) }
        if held(.right) { b |= UInt32(MMK_BTN_RIGHT) }
        if held(.up) { b |= UInt32(MMK_BTN_UP) }
        if held(.down) { b |= UInt32(MMK_BTN_DOWN) }
        if held(.jump) { b |= UInt32(MMK_BTN_JUMP) }
        if held(.run) { b |= UInt32(MMK_BTN_RUN) }
        if held(.spin) { b |= UInt32(MMK_BTN_SPIN) }
        if held(.pause) { b |= UInt32(MMK_BTN_START) }

        for (k, p) in padButtons.enumerated() where k != skipping { b |= p }
        #if os(iOS)
        b |= TouchInput.buttons
        #endif
        return MmkInput(buttons: b)
    }

    /// The device's own controls (keyboard / touch), without the pads: player 1's in co-op.
    var ownButtons: UInt32 {
        var b: UInt32 = 0
        if held(.left) { b |= UInt32(MMK_BTN_LEFT) }
        if held(.right) { b |= UInt32(MMK_BTN_RIGHT) }
        if held(.up) { b |= UInt32(MMK_BTN_UP) }
        if held(.down) { b |= UInt32(MMK_BTN_DOWN) }
        if held(.jump) { b |= UInt32(MMK_BTN_JUMP) }
        if held(.run) { b |= UInt32(MMK_BTN_RUN) }
        if held(.spin) { b |= UInt32(MMK_BTN_SPIN) }
        #if os(iOS)
        b |= TouchInput.buttons
        #endif
        return b
    }

    /// Each connected game controller's engine buttons, in connection order (co-op players).
    var padButtons: [UInt32] {
        GCController.controllers().compactMap { c -> UInt32? in
            guard let g = c.extendedGamepad else { return nil }
            var b: UInt32 = 0
            let x = g.leftThumbstick.xAxis.value, y = g.leftThumbstick.yAxis.value
            if g.dpad.left.isPressed || x < -0.4 { b |= UInt32(MMK_BTN_LEFT) }
            if g.dpad.right.isPressed || x > 0.4 { b |= UInt32(MMK_BTN_RIGHT) }
            if g.dpad.up.isPressed || y > 0.4 { b |= UInt32(MMK_BTN_UP) }
            if g.dpad.down.isPressed || y < -0.4 { b |= UInt32(MMK_BTN_DOWN) }
            if padHeld(.jump, g) { b |= UInt32(MMK_BTN_JUMP) }
            if padHeld(.run, g) { b |= UInt32(MMK_BTN_RUN) }
            if padHeld(.spin, g) { b |= UInt32(MMK_BTN_SPIN) }
            if padHeld(.pause, g) { b |= UInt32(MMK_BTN_START) }
            return b
        }
    }

    /// The Course Maker's keys held (`mmk_front::maker::keys`, as the portable shell's): U / ⌃Z
    /// undo, Y / ⌃⇧Z redo, Q the part window, ] / [ the palette slots, T test play from the
    /// cursor, E the eraser, G the other area, M held multigrab, C copy the selection.
    var makerKeys: UInt32 {
        let ctrl = anyDown([59, 62]), shift = anyDown([56, 60])
        let z = ctrl && anyDown([6])
        let keys: [(Bool, UInt32)] = [
            (anyDown([32]) || (z && !shift), 1),
            (anyDown([16]) || (z && shift), 2),
            (anyDown([12]), 4),
            (anyDown([30]), 8),
            (anyDown([33]), 16),
            (anyDown([17]), 32),
            (anyDown([14]), 64),
            (anyDown([5]), 128),
            (anyDown([46]), 256),
            (anyDown([8]) && !ctrl, 512),
        ]
        return keys.reduce(0) { $1.0 ? $0 | $1.1 : $0 }
    }

    /// The 100 Mario skip key is held (Settings: "Skip course", default − / Tab, controller −).
    var skipHeld: Bool {
        if held(.skip) { return true }
        #if os(iOS)
        if TouchInput.skip { return true }
        #endif
        return GCController.controllers().contains { c in c.extendedGamepad.map { padHeld(.skip, $0) } ?? false }
    }

    /// Buttons held for menu navigation.
    var menu: MenuInput { menu(skipping: nil) }

    /// Buttons held for menu navigation, without controller `skipping` (its index in
    /// `padButtons`: co-op making's player 2, whose buttons go to the front end on their own).
    func menu(skipping: Int?) -> MenuInput {
        var m: MenuInput = []
        if held(.up) { m.insert(.up) }
        if held(.down) { m.insert(.down) }
        if held(.left) { m.insert(.left) }
        if held(.right) { m.insert(.right) }
        if anyDown(Self.confirmKeys) || held(.jump) { m.insert(.confirm) }
        if held(.pause) { m.insert(.back) }
        for (code, b) in Self.shoulderKeys where anyDown([code]) { m.insert(b) }
        let pads = GCController.controllers().compactMap { $0.extendedGamepad }
        for (k, g) in pads.enumerated() where k != skipping {
            let x = g.leftThumbstick.xAxis.value, y = g.leftThumbstick.yAxis.value
            if g.dpad.up.isPressed || y > 0.5 { m.insert(.up) }
            if g.dpad.down.isPressed || y < -0.5 { m.insert(.down) }
            if g.dpad.left.isPressed || x < -0.5 { m.insert(.left) }
            if g.dpad.right.isPressed || x > 0.5 { m.insert(.right) }
            if g.buttonA.isPressed { m.insert(.confirm) }
            if g.buttonB.isPressed || padHeld(.pause, g) { m.insert(.back) }
            if g.leftShoulder.isPressed { m.insert(.l) }
            if g.rightShoulder.isPressed { m.insert(.r) }
            if g.leftTrigger.isPressed { m.insert(.zl) }
            if g.rightTrigger.isPressed { m.insert(.zr) }
        }
        #if os(iOS)
        // on-screen controls: the d-pad moves, Jump confirms, Run and + go back
        let t = TouchInput.buttons
        if t & UInt32(MMK_BTN_UP) != 0 { m.insert(.up) }
        if t & UInt32(MMK_BTN_DOWN) != 0 { m.insert(.down) }
        if t & UInt32(MMK_BTN_LEFT) != 0 { m.insert(.left) }
        if t & UInt32(MMK_BTN_RIGHT) != 0 { m.insert(.right) }
        if t & UInt32(MMK_BTN_JUMP) != 0 { m.insert(.confirm) }
        if t & UInt32(MMK_BTN_RUN | MMK_BTN_START) != 0 { m.insert(.back) }
        #endif
        // a key bound to both (e.g. Return as pause) confirms
        if m.contains(.confirm) { m.remove(.back) }
        return m
    }
}
