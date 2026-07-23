import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_HOTKEYS } from "./hotkeys";
import { SettingsPage } from "./SettingsPage";
import type { AppConfig, PobSummary } from "./types";

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

function noop() {}

describe("SettingsPage", () => {
  it("renders the current config values", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(
      screen.getByText("/Users/exile/Documents/My Games/Path of Exile/Client.txt"),
    ).toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveValue("league-start");
  });

  it("shows 'not set' when no log path is configured", () => {
    render(
      <SettingsPage
        config={config({ client_log_path: null })}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByText(/not set/i)).toBeInTheDocument();
  });

  it("calls onPick when Browse is clicked", () => {
    const onPick = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={onPick}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /browse/i }));
    expect(onPick).toHaveBeenCalledTimes(1);
  });

  it("calls onImportPreview with the textarea contents when previewing", () => {
    const onImportPreview = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={onImportPreview}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.change(screen.getByLabelText(/path of building/i), {
      target: { value: "https://pobb.in/abc123" },
    });
    fireEvent.click(screen.getByRole("button", { name: /preview import/i }));
    expect(onImportPreview).toHaveBeenCalledWith("https://pobb.in/abc123");
  });

  it("renders the preview card with class, ascendancy, milestone count, and a reliability badge", () => {
    const preview: PobSummary = {
      class_name: "Witch",
      ascend_name: "Necromancer",
      milestone_count: 5,
      reliability: "structured",
    };
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={preview}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByText(/witch/i)).toBeInTheDocument();
    expect(screen.getByText(/necromancer/i)).toBeInTheDocument();
    expect(screen.getByText(/5/)).toBeInTheDocument();
    const badge = screen.getByText(/structured/i);
    expect(badge).toHaveClass("reliability-structured");
  });

  it("renders unsupported reliability with the muted badge class", () => {
    const preview: PobSummary = {
      class_name: "Marauder",
      ascend_name: null,
      milestone_count: 0,
      reliability: "unsupported",
    };
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={preview}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    const badge = screen.getByText(/unsupported/i);
    expect(badge).toHaveClass("reliability-unsupported");
  });

  it("renders a preview error instead of a preview card", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError="invalid share code"
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByText(/invalid share code/i)).toBeInTheDocument();
  });

  it("passes the original config through to onSave untouched when nothing is edited", () => {
    // Regression guard for the "seeded from a placeholder config" bug: a
    // populated, non-default config (variant "standard", a real pob_code)
    // rendered once and Saved with no edits must round-trip exactly —
    // proving the form's initial state comes from the `config` prop's
    // actual values, not from defaults that happen to get overwritten
    // later.
    const onSave = vi.fn();
    const populated = config({ variant: "standard", pob_code: "https://pobb.in/existing" });
    render(
      <SettingsPage
        config={populated}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledWith(populated);
  });

  it("passes the edited variant and PoB text to onSave", () => {
    const onSave = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "standard" },
    });
    fireEvent.change(screen.getByLabelText(/path of building/i), {
      target: { value: "https://pobb.in/abc123" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledWith({
      client_log_path: "/Users/exile/Documents/My Games/Path of Exile/Client.txt",
      variant: "standard",
      pob_code: "https://pobb.in/abc123",
      overlay_opacity: 1,
      hotkeys: DEFAULT_HOTKEYS,
      show_run_timer: true,
    });
  });

  it("disables the Save button and shows a saving label while saving", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={true}
        savedAt={null}
      />,
    );
    expect(screen.getByRole("button", { name: /saving/i })).toBeDisabled();
  });

  it("renders the opacity slider seeded from config and shows the percentage", () => {
    render(
      <SettingsPage
        config={config({ overlay_opacity: 0.6 })}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByLabelText(/overlay opacity/i)).toHaveValue("60");
    expect(screen.getByText("60%")).toBeInTheDocument();
  });

  it("previews opacity live and includes the edited value in onSave", () => {
    const onSave = vi.fn();
    const onOpacityPreview = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
        onOpacityPreview={onOpacityPreview}
      />,
    );
    fireEvent.change(screen.getByLabelText(/overlay opacity/i), {
      target: { value: "45" },
    });
    expect(onOpacityPreview).toHaveBeenCalledWith(0.45);
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ overlay_opacity: 0.45 }),
    );
  });

  it("cannot represent an opacity below the 20% floor", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByLabelText(/overlay opacity/i)).toHaveAttribute("min", "20");
  });

  it("renders one hotkey input per action, seeded from config", () => {
    render(
      <SettingsPage
        config={config({
          hotkeys: { ...DEFAULT_HOTKEYS, settings: "ctrl+shift+o" },
        })}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByLabelText(/setup mode/i)).toHaveValue("alt+shift+s");
    expect(screen.getByLabelText(/hide\/show overlay/i)).toHaveValue("alt+shift+h");
    expect(screen.getByLabelText(/open settings/i)).toHaveValue("ctrl+shift+o");
    expect(screen.getByLabelText(/compact mode/i)).toHaveValue("alt+shift+c");
    expect(screen.getByLabelText(/zoom/i)).toHaveValue("alt+shift+z");
  });

  it("passes edited hotkeys to onSave in normalized form", () => {
    const onSave = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.change(screen.getByLabelText(/open settings/i), {
      target: { value: "Ctrl+Shift+P" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        hotkeys: { ...DEFAULT_HOTKEYS, settings: "ctrl+shift+p" },
      }),
    );
  });

  it("shows a clear error and disables Save for an invalid hotkey", () => {
    const onSave = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.change(screen.getByLabelText(/setup mode/i), {
      target: { value: "not a combo" },
    });
    expect(screen.getByText(/invalid hotkey/i)).toBeInTheDocument();
    const save = screen.getByRole("button", { name: /^save$/i });
    expect(save).toBeDisabled();
    fireEvent.click(save);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("shows conflict errors on both actions and disables Save when two hotkeys collide", () => {
    const onSave = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.change(screen.getByLabelText(/setup mode/i), {
      target: { value: "alt+shift+h" },
    });
    expect(screen.getAllByText(/conflict/i).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByRole("button", { name: /^save$/i })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).not.toHaveBeenCalled();
  });

  it("shows a saved confirmation once savedAt is set", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={Date.now()}
      />,
    );
    expect(screen.getByText(/saved/i)).toBeInTheDocument();
  });

  it("renders the run timer checkbox checked from config", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByLabelText(/show run timer/i)).toBeChecked();
  });

  it("round-trips an unchecked run timer through Save", () => {
    const onSave = vi.fn();
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={onSave}
        saving={false}
        savedAt={null}
      />,
    );
    fireEvent.click(screen.getByLabelText(/show run timer/i));
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ show_run_timer: false }),
    );
  });

  it("renders a hotkey input for the run timer", () => {
    render(
      <SettingsPage
        config={config()}
        onPick={noop}
        onImportPreview={noop}
        preview={null}
        previewError={null}
        onSave={noop}
        saving={false}
        savedAt={null}
      />,
    );
    expect(screen.getByLabelText(/start\/stop run timer/i)).toHaveValue("alt+shift+t");
  });
});
