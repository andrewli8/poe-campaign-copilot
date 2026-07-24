import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_HOTKEYS } from "./hotkeys";
import { SettingsPage, type SettingsPageProps } from "./SettingsPage";
import type { AppConfig } from "./types";

function config(overrides: Partial<AppConfig> = {}): AppConfig {
  return {
    client_log_path: "/Users/exile/Documents/My Games/Path of Exile/Client.txt",
    variant: "league-start",
    pob_code: null,
    overlay_opacity: 1,
    hotkeys: DEFAULT_HOTKEYS,
    show_run_timer: true,
    ...overrides,
  };
}

// Renders SettingsPage with sane defaults for every prop; tests override only
// what they exercise. Returns the props (with any spies the test passed) for
// convenient assertions.
function renderPage(overrides: Partial<SettingsPageProps> = {}) {
  const props: SettingsPageProps = {
    config: config(),
    onPick: () => Promise.resolve(null),
    onImportPreview: () => {},
    preview: null,
    previewError: null,
    onSave: () => {},
    onReset: () => {},
    onResetTimer: () => {},
    saving: false,
    savedAt: null,
    ...overrides,
  };
  render(<SettingsPage {...props} /> as ReactElement);
  return props;
}

describe("SettingsPage", () => {
  it("renders the current config values", () => {
    renderPage();
    expect(
      screen.getByText("/Users/exile/Documents/My Games/Path of Exile/Client.txt"),
    ).toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveValue("league-start");
  });

  it("shows 'not set' when no log path is configured", () => {
    renderPage({ config: config({ client_log_path: null }) });
    expect(screen.getByText(/not set/i)).toBeInTheDocument();
  });

  it("calls onPick when Browse is clicked", () => {
    const onPick = vi.fn().mockResolvedValue(null);
    renderPage({ onPick });
    fireEvent.click(screen.getByRole("button", { name: /browse/i }));
    expect(onPick).toHaveBeenCalledTimes(1);
  });

  it("autosaves the picked log path", async () => {
    const onSave = vi.fn();
    const onPick = vi.fn().mockResolvedValue("/new/Client.txt");
    renderPage({ onPick, onSave });
    fireEvent.click(screen.getByRole("button", { name: /browse/i }));
    // handlePick awaits onPick before committing.
    await Promise.resolve();
    await Promise.resolve();
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ client_log_path: "/new/Client.txt" }),
    );
  });

  it("calls onImportPreview with the textarea contents when previewing", () => {
    const onImportPreview = vi.fn();
    renderPage({ onImportPreview });
    fireEvent.change(screen.getByLabelText(/path of building/i), {
      target: { value: "https://pobb.in/abc123" },
    });
    fireEvent.click(screen.getByRole("button", { name: /preview import/i }));
    expect(onImportPreview).toHaveBeenCalledWith("https://pobb.in/abc123");
  });

  it("renders the preview card with class, ascendancy, milestone count, and a reliability badge", () => {
    renderPage({
      preview: {
        class_name: "Witch",
        ascend_name: "Necromancer",
        milestone_count: 5,
        reliability: "structured",
      },
    });
    expect(screen.getByText(/witch/i)).toBeInTheDocument();
    expect(screen.getByText(/necromancer/i)).toBeInTheDocument();
    expect(screen.getByText(/5/)).toBeInTheDocument();
    expect(screen.getByText(/structured/i)).toHaveClass("reliability-structured");
  });

  it("renders unsupported reliability with the muted badge class", () => {
    renderPage({
      preview: {
        class_name: "Marauder",
        ascend_name: null,
        milestone_count: 0,
        reliability: "unsupported",
      },
    });
    expect(screen.getByText(/unsupported/i)).toHaveClass("reliability-unsupported");
  });

  it("renders a preview error instead of a preview card", () => {
    renderPage({ previewError: "invalid share code" });
    expect(screen.getByText(/invalid share code/i)).toBeInTheDocument();
  });

  it("round-trips the config unchanged when a field is committed with no edit", () => {
    // Blurring the PoB field with no change still autosaves — the full config
    // must round-trip exactly (proving the form's state came from the actual
    // `config` prop values, not overwritten defaults).
    const onSave = vi.fn();
    const populated = config({ variant: "standard", pob_code: "https://pobb.in/existing" });
    renderPage({ config: populated, onSave });
    fireEvent.blur(screen.getByLabelText(/path of building/i));
    expect(onSave).toHaveBeenCalledWith(populated);
  });

  it("autosaves the edited variant immediately", () => {
    const onSave = vi.fn();
    renderPage({ onSave });
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "standard" } });
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ variant: "standard" }),
    );
  });

  it("autosaves the edited PoB text on blur, with the current variant", () => {
    const onSave = vi.fn();
    renderPage({ onSave });
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "standard" } });
    fireEvent.change(screen.getByLabelText(/path of building/i), {
      target: { value: "https://pobb.in/abc123" },
    });
    fireEvent.blur(screen.getByLabelText(/path of building/i));
    expect(onSave).toHaveBeenLastCalledWith({
      client_log_path: "/Users/exile/Documents/My Games/Path of Exile/Client.txt",
      variant: "standard",
      pob_code: "https://pobb.in/abc123",
      overlay_opacity: 1,
      hotkeys: DEFAULT_HOTKEYS,
      show_run_timer: true,
    });
  });

  it("shows a saving status while a save is in flight", () => {
    renderPage({ saving: true });
    expect(screen.getByText(/saving/i)).toBeInTheDocument();
  });

  it("renders the opacity slider seeded from config and shows the percentage", () => {
    renderPage({ config: config({ overlay_opacity: 0.6 }) });
    expect(screen.getByLabelText(/overlay opacity/i)).toHaveValue("60");
    expect(screen.getByText("60%")).toBeInTheDocument();
  });

  it("previews opacity live on drag and autosaves it on release", () => {
    const onSave = vi.fn();
    const onOpacityPreview = vi.fn();
    renderPage({ onSave, onOpacityPreview });
    const slider = screen.getByLabelText(/overlay opacity/i);
    fireEvent.change(slider, { target: { value: "45" } });
    expect(onOpacityPreview).toHaveBeenCalledWith(0.45);
    // Live preview must NOT persist mid-drag.
    expect(onSave).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ overlay_opacity: 0.45 }),
    );
  });

  it("cannot represent an opacity below the 20% floor", () => {
    renderPage();
    expect(screen.getByLabelText(/overlay opacity/i)).toHaveAttribute("min", "20");
  });

  it("renders one hotkey input per action, seeded from config", () => {
    renderPage({
      config: config({ hotkeys: { ...DEFAULT_HOTKEYS, settings: "ctrl+shift+o" } }),
    });
    expect(screen.getByLabelText(/setup mode/i)).toHaveValue("alt+shift+s");
    expect(screen.getByLabelText(/hide\/show overlay/i)).toHaveValue("alt+shift+h");
    expect(screen.getByLabelText(/open settings/i)).toHaveValue("ctrl+shift+o");
    expect(screen.getByLabelText(/compact mode/i)).toHaveValue("alt+shift+c");
    expect(screen.getByLabelText(/zoom/i)).toHaveValue("alt+shift+z");
  });

  it("autosaves edited hotkeys on blur, in normalized form", () => {
    const onSave = vi.fn();
    renderPage({ onSave });
    fireEvent.change(screen.getByLabelText(/open settings/i), {
      target: { value: "Ctrl+Shift+P" },
    });
    fireEvent.blur(screen.getByLabelText(/open settings/i));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        hotkeys: { ...DEFAULT_HOTKEYS, settings: "ctrl+shift+p" },
      }),
    );
  });

  it("shows an error and does not autosave an invalid hotkey", () => {
    const onSave = vi.fn();
    renderPage({ onSave });
    const input = screen.getByLabelText(/setup mode/i);
    fireEvent.change(input, { target: { value: "not a combo" } });
    expect(screen.getByText(/invalid hotkey/i)).toBeInTheDocument();
    fireEvent.blur(input);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("shows conflict errors and does not autosave when two hotkeys collide", () => {
    const onSave = vi.fn();
    renderPage({ onSave });
    const input = screen.getByLabelText(/setup mode/i);
    fireEvent.change(input, { target: { value: "alt+shift+h" } });
    expect(screen.getAllByText(/conflict/i).length).toBeGreaterThanOrEqual(2);
    fireEvent.blur(input);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("shows a saved confirmation once savedAt is set", () => {
    renderPage({ savedAt: 1234 });
    expect(screen.getByText(/saved ✓/i)).toBeInTheDocument();
  });

  it("renders the run timer checkbox checked from config", () => {
    renderPage();
    expect(screen.getByLabelText(/show run timer/i)).toBeChecked();
  });

  it("autosaves the run timer checkbox on toggle", () => {
    const onSave = vi.fn();
    renderPage({ onSave });
    fireEvent.click(screen.getByLabelText(/show run timer/i));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ show_run_timer: false }),
    );
  });

  it("renders a hotkey input for the run timer", () => {
    renderPage();
    expect(screen.getByLabelText(/start\/stop run timer/i)).toHaveValue("alt+shift+t");
  });

  it("resets only the run timer via its own button, without touching progress", () => {
    const onResetTimer = vi.fn();
    const onReset = vi.fn();
    renderPage({ onResetTimer, onReset });
    fireEvent.click(screen.getByRole("button", { name: /^reset run timer$/i }));
    expect(onResetTimer).toHaveBeenCalledTimes(1);
    // The campaign "Reset progress" flow is untouched by the timer reset.
    expect(onReset).not.toHaveBeenCalled();
  });

  it("requires confirmation before resetting progress", () => {
    const onReset = vi.fn();
    renderPage({ onReset });
    // Clicking the reset button reveals a confirm; it does NOT reset yet.
    fireEvent.click(screen.getByRole("button", { name: /reset progress/i }));
    expect(onReset).not.toHaveBeenCalled();
    // Confirming fires the reset.
    fireEvent.click(screen.getByRole("button", { name: /yes, reset/i }));
    expect(onReset).toHaveBeenCalledTimes(1);
  });

  it("cancels a reset without firing onReset", () => {
    const onReset = vi.fn();
    renderPage({ onReset });
    fireEvent.click(screen.getByRole("button", { name: /reset progress/i }));
    fireEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onReset).not.toHaveBeenCalled();
    // The confirm is dismissed; the reset button is back.
    expect(screen.getByRole("button", { name: /reset progress/i })).toBeInTheDocument();
  });
});
