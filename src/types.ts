// Mirrors the Rust UiModel/OverlayModel JSON payloads (snake_case) exactly.
// See src-tauri/src/pipeline.rs (UiModel, UiImage) and
// crates/composer/src/lib.rs (OverlayModel, NoteView, ImageView).

export interface NoteView {
  text: string;
  stale: boolean;
}

export interface UiImage {
  file: string;
  stale: boolean;
  data_url: string;
}

export interface OverlayModel {
  zone_name: string;
  area_id: string;
  act: number;
  off_route_zone: string | null;
  layout_images: { file: string; stale: boolean }[];
  layout_notes: NoteView[];
  steps_in_zone: string[];
  sub_hints: string[];
  primary: string;
  next_zone: string | null;
  pending_count: number;
  town_reminders: string[];
  build_reminders: string[];
  is_town: boolean;
  route_complete: boolean;
}

export interface UiModel {
  overlay: OverlayModel;
  images: UiImage[];
  waiting_for_log: boolean;
}
