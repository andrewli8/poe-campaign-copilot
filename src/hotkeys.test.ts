import { describe, expect, it } from "vitest";
import {
  DEFAULT_HOTKEYS,
  HOTKEY_ACTIONS,
  normalizeHotkey,
  validateHotkeyConfig,
} from "./hotkeys";

describe("DEFAULT_HOTKEYS", () => {
  it("matches the backend defaults for the pre-existing shortcuts", () => {
    expect(DEFAULT_HOTKEYS.zoom).toBe("alt+shift+z");
    expect(DEFAULT_HOTKEYS.compact).toBe("alt+shift+c");
    expect(DEFAULT_HOTKEYS.hide).toBe("alt+shift+h");
  });

  it("assigns defaults to setup and settings", () => {
    expect(DEFAULT_HOTKEYS.setup).toBe("alt+shift+s");
    expect(DEFAULT_HOTKEYS.settings).toBe("alt+shift+o");
  });

  it("has no conflicts among its own bindings", () => {
    expect(validateHotkeyConfig(DEFAULT_HOTKEYS)).toEqual({});
  });

  it("covers every action exactly once", () => {
    const keys = HOTKEY_ACTIONS.map((a) => a.key).sort();
    expect(keys).toEqual(["compact", "hide", "settings", "setup", "timer", "zoom"]);
  });
});

describe("timer hotkey", () => {
  it("has a default binding that is valid and conflict-free", () => {
    expect(DEFAULT_HOTKEYS.timer).toBe("alt+shift+t");
    expect(validateHotkeyConfig(DEFAULT_HOTKEYS)).toEqual({});
  });

  it("appears in HOTKEY_ACTIONS so the settings UI renders it", () => {
    expect(HOTKEY_ACTIONS.map((a) => a.key)).toContain("timer");
  });

  it("participates in conflict detection", () => {
    const errors = validateHotkeyConfig({ ...DEFAULT_HOTKEYS, timer: "alt+shift+z" });
    expect(errors.timer).toMatch(/conflict/i);
    expect(errors.zoom).toMatch(/conflict/i);
  });
});

describe("normalizeHotkey", () => {
  it("lowercases and canonicalizes a friendly combo", () => {
    expect(normalizeHotkey("Alt+Shift+S")).toBe("alt+shift+s");
  });

  it("orders modifiers canonically regardless of input order", () => {
    expect(normalizeHotkey("shift+alt+S")).toBe("alt+shift+s");
    expect(normalizeHotkey("Shift+Ctrl+1")).toBe("ctrl+shift+1");
  });

  it("maps modifier aliases (option, cmd, control) to canonical names", () => {
    expect(normalizeHotkey("Option+Shift+X")).toBe("alt+shift+x");
    expect(normalizeHotkey("Control+F5")).toBe("ctrl+f5");
    // Canonical modifier order is ctrl, alt, shift, super.
    expect(normalizeHotkey("Cmd+Shift+P")).toBe("shift+super+p");
  });

  it("tolerates whitespace around tokens", () => {
    expect(normalizeHotkey(" alt + shift + z ")).toBe("alt+shift+z");
  });

  it("accepts function keys and named keys", () => {
    expect(normalizeHotkey("ctrl+F12")).toBe("ctrl+f12");
    expect(normalizeHotkey("alt+Space")).toBe("alt+space");
    expect(normalizeHotkey("alt+Home")).toBe("alt+home");
  });

  it("rejects a bare key with no modifier", () => {
    // A modifier-less global shortcut would swallow plain typing (e.g. in
    // PoE chat), so it is rejected outright.
    expect(normalizeHotkey("s")).toBeNull();
    expect(normalizeHotkey("F5")).toBeNull();
  });

  it("rejects modifiers with no key", () => {
    expect(normalizeHotkey("alt+shift")).toBeNull();
  });

  it("rejects two non-modifier keys", () => {
    expect(normalizeHotkey("alt+a+b")).toBeNull();
  });

  it("rejects duplicate modifiers", () => {
    expect(normalizeHotkey("alt+alt+z")).toBeNull();
  });

  it("rejects unknown keys, empty strings, and empty tokens", () => {
    expect(normalizeHotkey("alt+shift+bogus")).toBeNull();
    expect(normalizeHotkey("")).toBeNull();
    expect(normalizeHotkey("alt++z")).toBeNull();
  });
});

describe("validateHotkeyConfig", () => {
  it("returns no errors for the defaults", () => {
    expect(validateHotkeyConfig(DEFAULT_HOTKEYS)).toEqual({});
  });

  it("flags an invalid combo with a clear message", () => {
    const errors = validateHotkeyConfig({ ...DEFAULT_HOTKEYS, setup: "not a combo" });
    expect(errors.setup).toMatch(/invalid/i);
    expect(Object.keys(errors)).toEqual(["setup"]);
  });

  it("flags both actions when two share the same combo", () => {
    const errors = validateHotkeyConfig({ ...DEFAULT_HOTKEYS, setup: "alt+shift+h" });
    expect(errors.setup).toMatch(/conflict/i);
    expect(errors.hide).toMatch(/conflict/i);
  });

  it("detects conflicts across formatting differences", () => {
    // "Shift+Alt+H" and "alt+shift+h" are the same physical chord.
    const errors = validateHotkeyConfig({ ...DEFAULT_HOTKEYS, settings: "Shift+Alt+H" });
    expect(errors.settings).toMatch(/conflict/i);
    expect(errors.hide).toMatch(/conflict/i);
  });
});
