//! Pure pipeline: log lines -> UiModel. No Tauri types here.
//! NOTE: images load from the repo's content/layouts/assets at runtime —
//! fine for development; distribution packaging revisits this path.

use std::collections::HashMap;
use std::path::{Component, Path};

use base64::Engine as _;
use composer::{BuildContext, OverlayModel, compose, layouts_by_area};
use content::compile::{Variant, compile_route_pack};
use content::game_data::{AreaMap, load_vendored};
use content::layouts::{LayoutEntry, layouts_dir, load_all_layouts};
use route_engine::RouteEngine;
use serde::Serialize;
use session::{SessionEvent, SessionTracker};
use task_engine::TaskEngine;
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct UiImage {
    pub file: String,
    pub stale: bool,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiModel {
    pub overlay: OverlayModel,
    pub images: Vec<UiImage>,
    pub waiting_for_log: bool,
    /// Short label for the imported leveling build, e.g. "Ranger (Deadeye)
    /// — 12 milestones". `None` when no build has been imported.
    pub build_summary: Option<String>,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("game data: {0}")]
    GameData(#[from] content::game_data::GameDataError),
    #[error("route compile: {0}")]
    Compile(#[from] content::compile::CompileError),
    #[error("layouts: {0}")]
    Layouts(#[from] content::layouts::LayoutError),
}

pub struct Pipeline {
    tracker: SessionTracker,
    engine: RouteEngine,
    tasks: TaskEngine,
    layouts: std::collections::BTreeMap<String, LayoutEntry>,
    areas: AreaMap,
    seen_area_event: bool,
    encoded: HashMap<String, String>,
    build: Option<pob_import::LevelingBuildPlan>,
}

impl Pipeline {
    /// `variant` selects the compiled route pack (league-start vs.
    /// standard); `build` is the imported leveling-build plan, if any —
    /// `None` when the user hasn't imported a Path of Building code.
    pub fn new(
        variant: Variant,
        build: Option<pob_import::LevelingBuildPlan>,
    ) -> Result<Self, PipelineError> {
        let (areas, _) = load_vendored()?;
        let pack = compile_route_pack(variant)?;
        let engine = RouteEngine::from_pack(&pack, areas.clone());
        let layouts = layouts_by_area(load_all_layouts()?);
        Ok(Self {
            tracker: SessionTracker::new(areas.clone()),
            engine,
            tasks: TaskEngine::new(areas.clone()),
            layouts,
            areas,
            seen_area_event: false,
            encoded: HashMap::new(),
            build,
        })
    }

    /// Drop all live session state back to a fresh start — the manual
    /// "Reset progress" action. The compiled route returns to Act 1
    /// (`engine.restart`), in-progress task reminders and the tracked
    /// player level are cleared (fresh `TaskEngine` / `SessionTracker`),
    /// and the overlay returns to waiting for the new character's first
    /// zone (`seen_area_event = false`). Everything config-derived — the
    /// route steps, imported `build`, `layouts`, and the content-addressed
    /// `encoded` image cache — is kept.
    pub fn reset(&mut self) {
        self.engine.restart();
        self.tasks = TaskEngine::new(self.areas.clone());
        self.tracker = SessionTracker::new(self.areas.clone());
        self.seen_area_event = false;
    }

    pub fn on_line(&mut self, line: &str) -> Option<UiModel> {
        let raw = event_parser::parse_line(line);
        let mut area_changed = false;
        for ev in self.tracker.on_raw(&raw) {
            if let SessionEvent::AreaEntered {
                area_id,
                new_instance,
                ..
            } = &ev
            {
                let advance = self.engine.on_area_entered(area_id, *new_instance);
                for &i in advance.newly_done.iter().chain(&advance.newly_skipped) {
                    let step = self.engine.steps()[i].clone();
                    let status = self.engine.statuses()[i];
                    self.tasks.on_step_passed(&step, status);
                }
                area_changed = true;
                self.seen_area_event = true;
            }
        }
        area_changed.then(|| self.current_model())
    }

    pub fn current_model(&mut self) -> UiModel {
        let player_level = self.tracker.player_level();
        let build_ctx = self
            .build
            .as_ref()
            .map(|plan| BuildContext { plan, player_level });
        let overlay = compose(
            &self.engine,
            &self.tasks,
            &self.layouts,
            &self.areas,
            build_ctx,
        );
        let images = overlay
            .layout_images
            .iter()
            .map(|iv| UiImage {
                file: iv.file.clone(),
                stale: iv.stale,
                data_url: self.data_url_for(&iv.file),
            })
            .collect();
        UiModel {
            overlay,
            images,
            waiting_for_log: !self.seen_area_event,
            build_summary: self.build.as_ref().map(build_summary),
        }
    }

    fn data_url_for(&mut self, file: &str) -> String {
        if let Some(hit) = self.encoded.get(file) {
            return hit.clone();
        }
        let url = if !is_safe_asset_name(file) {
            String::new()
        } else {
            let path = layouts_dir().join("assets").join(file);
            match std::fs::read(&path) {
                Ok(bytes) => format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ),
                Err(_) => String::new(),
            }
        };
        self.encoded.insert(file.to_string(), url.clone());
        url
    }

    #[cfg(test)]
    pub(crate) fn encoded_cache_len(&self) -> usize {
        self.encoded.len()
    }
}

/// True when `file` is exactly one normal path component — no `..`, no
/// root/prefix (absolute paths, UNC/`\\host\share`), no separators splitting
/// it into multiple components, and not empty. Guards `data_url_for` against
/// a content-pack-supplied `file` value escaping the layouts `assets/`
/// directory: `PathBuf::join` with an absolute path REPLACES the base
/// instead of nesting under it, so an unvalidated `file` could otherwise
/// read arbitrary local files.
///
/// The explicit `contains('\\')` check matters beyond what `Component`
/// parsing alone gives us: `\` is only a path separator on Windows (the
/// actual shipping target, via the NSIS installer), so on Unix
/// `Path::new("\\\\host\\share").components()` yields a single
/// `Normal(..)` component — `Component` parsing alone would treat a UNC-
/// style string as "safe" on non-Windows build/test hosts. Rejecting any
/// backslash outright keeps this check's result platform-independent.
fn is_safe_asset_name(file: &str) -> bool {
    if file.contains('\\') {
        return false;
    }
    let mut comps = Path::new(file).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// "Ranger (Deadeye) — 12 milestones", or "Ranger — 12 milestones" when the
/// build has no ascendancy recorded yet.
fn build_summary(plan: &pob_import::LevelingBuildPlan) -> String {
    let count = plan.milestones.len();
    match &plan.ascend_name {
        Some(ascend) => format!("{} ({ascend}) — {count} milestones", plan.class_name),
        None => format!("{} — {count} milestones", plan.class_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEN_STRAND: &str = r#"2026/07/21 19:00:01 1000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_1" with seed 101"#;
    const ENTER_STRAND: &str = "2026/07/21 19:00:03 3000 f22b6b26 [INFO Client 900] : You have entered The Twilight Strand.";
    const GEN_COAST: &str = r#"2026/07/21 19:04:00 240000 1186a0e0 [DEBUG Client 900] Generating level 2 area "1_1_2" with seed 8801"#;
    const ENTER_COAST: &str =
        "2026/07/21 19:04:02 242000 f22b6b26 [INFO Client 900] : You have entered The Coast.";
    const GEN_TOWN: &str = r#"2026/07/21 19:03:20 200000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_town" with seed 555"#;
    const ENTER_TOWN: &str =
        "2026/07/21 19:03:22 202000 f22b6b26 [INFO Client 900] : You have entered Lioneye's Watch.";

    #[test]
    fn initial_model_is_waiting() {
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        let m = p.current_model();
        assert!(m.waiting_for_log);
    }

    #[test]
    fn area_entry_produces_model_with_images() {
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        assert!(
            p.on_line(GEN_STRAND).is_none(),
            "generating alone changes nothing"
        );
        let m = p.on_line(ENTER_STRAND).expect("entered -> model");
        assert!(!m.waiting_for_log);
        assert_eq!(m.overlay.zone_name, "The Twilight Strand");

        p.on_line(GEN_TOWN);
        p.on_line(ENTER_TOWN);
        p.on_line(GEN_COAST);
        let m = p.on_line(ENTER_COAST).unwrap();
        assert_eq!(m.overlay.zone_name, "The Coast");
        assert!(!m.images.is_empty(), "The Coast has layout images");
        assert!(m.images[0].data_url.starts_with("data:image/png;base64,"));
        assert!(m.images[0].data_url.len() > 100);
    }

    #[test]
    fn non_area_lines_produce_none() {
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        assert!(p.on_line("garbage").is_none());
        assert!(p
            .on_line("2026/07/21 19:02:10 130000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 2")
            .is_none());
    }

    #[test]
    fn data_urls_are_memoized() {
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        p.on_line(GEN_COAST);
        let m1 = p.on_line(ENTER_COAST).unwrap();
        // Re-enter: same encoded strings (pointer equality is not observable;
        // assert cache hit via identical output and cache length).
        p.on_line(GEN_COAST);
        let m2 = p.on_line(ENTER_COAST).unwrap();
        assert_eq!(m1.images[0].data_url, m2.images[0].data_url);
        assert_eq!(p.encoded_cache_len(), m1.images.len());
    }

    fn hand_built_plan() -> pob_import::LevelingBuildPlan {
        pob_import::LevelingBuildPlan {
            class_name: "Ranger".into(),
            ascend_name: Some("Deadeye".into()),
            skill_sets: vec![],
            passive_spec_titles: vec![],
            notes: None,
            milestones: vec![pob_import::Milestone {
                level: 2,
                label: "Gem available: Frostblink".into(),
                reliability: pob_import::Reliability::Structured,
            }],
            reliability: pob_import::Reliability::Structured,
        }
    }

    #[test]
    fn pipeline_with_build_surfaces_summary_and_town_reminders() {
        let mut p = Pipeline::new(Variant::LeagueStart, Some(hand_built_plan())).unwrap();

        // No area entered yet: still waiting, but the build summary is
        // already available (it doesn't depend on session state).
        let m = p.current_model();
        assert!(m.waiting_for_log);
        assert_eq!(
            m.build_summary.as_deref(),
            Some("Ranger (Deadeye) — 1 milestones")
        );

        // Pin the player's level (LevelUp lines alone never yield a model).
        assert!(
            p.on_line(
                "2026/07/21 19:02:10 130000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 2"
            )
            .is_none()
        );

        // Enter town: the milestone (level 2) is within the reminder window
        // for a level-2 player.
        p.on_line(GEN_TOWN);
        let m = p.on_line(ENTER_TOWN).expect("entered town -> model");
        assert_eq!(
            m.build_summary.as_deref(),
            Some("Ranger (Deadeye) — 1 milestones")
        );
        assert_eq!(
            m.overlay.build_reminders,
            vec!["Gem available: Frostblink".to_string()]
        );
    }

    #[test]
    fn pipeline_without_build_has_no_summary() {
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        let m = p.current_model();
        assert_eq!(m.build_summary, None);
    }

    #[test]
    fn is_safe_asset_name_rejects_path_traversal_and_absolute_paths() {
        assert!(!is_safe_asset_name("../../etc/passwd"));
        assert!(!is_safe_asset_name("/etc/passwd"));
        assert!(!is_safe_asset_name("a/b.png"));
        assert!(!is_safe_asset_name("\\\\host\\share"));
        assert!(!is_safe_asset_name(""));
    }

    #[test]
    fn is_safe_asset_name_accepts_a_plain_filename() {
        assert!(is_safe_asset_name("image10.png"));
    }

    #[test]
    fn data_url_for_rejects_traversal_and_yields_empty_string() {
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        for unsafe_name in ["../../etc/passwd", "/etc/passwd", "a/b.png", ""] {
            assert_eq!(p.data_url_for(unsafe_name), String::new());
        }
    }
}
