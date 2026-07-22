import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SettingsPage } from "./SettingsPage";
import type { AppConfig, PobSummary } from "./types";

function config(overrides: Partial<AppConfig> = {}): AppConfig {
  return {
    client_log_path: "/Users/exile/Documents/My Games/Path of Exile/Client.txt",
    variant: "league-start",
    pob_code: null,
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
});
