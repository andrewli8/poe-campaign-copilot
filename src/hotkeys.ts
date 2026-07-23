// Hotkey parsing/validation shared by the settings form. The canonical
// string format ("alt+shift+s": lowercase, modifiers in a fixed order,
// exactly one non-modifier key) is what gets persisted in config.json and
// what the backend feeds to tauri-plugin-global-shortcut, which parses the
// same "mod+mod+key" grammar. The backend re-validates on Save
// (src-tauri/src/hotkeys.rs); this module exists so the form can reject
// bad combos and conflicts before a round-trip.

import type { HotkeyConfig } from "./types";

export const DEFAULT_HOTKEYS: HotkeyConfig = {
  zoom: "alt+shift+z",
  compact: "alt+shift+c",
  hide: "alt+shift+h",
  setup: "alt+shift+s",
  settings: "alt+shift+o",
};

export type HotkeyAction = keyof HotkeyConfig;

export const HOTKEY_ACTIONS: { key: HotkeyAction; label: string }[] = [
  { key: "setup", label: "Toggle setup mode" },
  { key: "hide", label: "Hide/show overlay" },
  { key: "settings", label: "Open settings" },
  { key: "compact", label: "Toggle compact mode" },
  { key: "zoom", label: "Toggle zoom" },
];

// Canonical modifier order for the normalized string. Aliases cover the
// spellings users actually type ("Option" on macOS, "Cmd", "Control").
const CANONICAL_MODIFIERS = ["ctrl", "alt", "shift", "super"] as const;

const MODIFIER_ALIASES: Record<string, (typeof CANONICAL_MODIFIERS)[number]> = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  super: "super",
  cmd: "super",
  command: "super",
  meta: "super",
  win: "super",
};

// Non-modifier keys the backend parser is known to accept. Deliberately
// conservative: letters, digits, F-keys, and a small set of named keys.
const NAMED_KEYS = new Set([
  "space",
  "enter",
  "tab",
  "home",
  "end",
  "pageup",
  "pagedown",
  "insert",
  "delete",
  "backspace",
  "up",
  "down",
  "left",
  "right",
]);

function isValidKey(token: string): boolean {
  return (
    /^[a-z0-9]$/.test(token) ||
    /^f([1-9]|1[0-9]|2[0-4])$/.test(token) ||
    NAMED_KEYS.has(token)
  );
}

/**
 * Normalizes a user-typed combo ("Alt + Shift + S") to canonical form
 * ("alt+shift+s"), or returns null if it isn't a usable global shortcut:
 * it must have at least one modifier (a bare "S" would fire while typing
 * in game chat), exactly one non-modifier key, and no duplicate modifiers.
 */
export function normalizeHotkey(input: string): string | null {
  const tokens = input.split("+").map((t) => t.trim().toLowerCase());
  if (tokens.some((t) => t === "")) {
    return null;
  }

  const modifiers = new Set<string>();
  const keys: string[] = [];
  for (const token of tokens) {
    const modifier = MODIFIER_ALIASES[token];
    if (modifier) {
      if (modifiers.has(modifier)) {
        return null; // duplicate modifier, e.g. "alt+alt+z"
      }
      modifiers.add(modifier);
    } else {
      keys.push(token);
    }
  }

  if (modifiers.size === 0 || keys.length !== 1 || !isValidKey(keys[0])) {
    return null;
  }

  const orderedModifiers = CANONICAL_MODIFIERS.filter((m) => modifiers.has(m));
  return [...orderedModifiers, keys[0]].join("+");
}

export type HotkeyErrors = Partial<Record<HotkeyAction, string>>;

/**
 * Validates a full hotkey config: every combo must normalize, and no two
 * actions may share the same normalized chord. Returns a per-action error
 * map — empty object means valid. Never mutates its input.
 */
export function validateHotkeyConfig(hotkeys: HotkeyConfig): HotkeyErrors {
  const errors: HotkeyErrors = {};
  const normalized = new Map<HotkeyAction, string>();

  for (const { key } of HOTKEY_ACTIONS) {
    const canonical = normalizeHotkey(hotkeys[key]);
    if (canonical === null) {
      errors[key] = 'Invalid hotkey — use a form like "Alt+Shift+S"';
    } else {
      normalized.set(key, canonical);
    }
  }

  const byChord = new Map<string, HotkeyAction[]>();
  for (const [action, chord] of normalized) {
    byChord.set(chord, [...(byChord.get(chord) ?? []), action]);
  }
  for (const [chord, actions] of byChord) {
    if (actions.length > 1) {
      for (const action of actions) {
        const others = actions
          .filter((a) => a !== action)
          .map((a) => HOTKEY_ACTIONS.find((h) => h.key === a)?.label ?? a);
        errors[action] = `Conflict: "${chord}" is also bound to ${others.join(", ")}`;
      }
    }
  }

  return errors;
}

/**
 * Returns a copy of the config with every combo in canonical form.
 * Callers must have validated first; an unparseable combo is left as-is
 * (the backend will reject it on Save with its own error).
 */
export function normalizeHotkeyConfig(hotkeys: HotkeyConfig): HotkeyConfig {
  const result = { ...hotkeys };
  for (const { key } of HOTKEY_ACTIONS) {
    const canonical = normalizeHotkey(hotkeys[key]);
    if (canonical !== null) {
      result[key] = canonical;
    }
  }
  return result;
}
