use content::route_dsl::Fragment;
use content::walk::CompiledStep;
use route_engine::StepStatus;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PendingKind {
    Portal,
    Trial,
    QuestReward,
    VendorReward,
    Crafting,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingTask {
    pub id: String,
    pub kind: PendingKind,
    pub label: String,
    pub zone_area_id: String,
    pub step_id: String,
}

#[derive(Default)]
pub struct TaskEngine {
    pending: Vec<PendingTask>,
}

impl TaskEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> &[PendingTask] {
        &self.pending
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn town_reminders(&self) -> Vec<&PendingTask> {
        self.pending
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    PendingKind::QuestReward | PendingKind::VendorReward | PendingKind::Crafting
                )
            })
            .collect()
    }

    pub fn on_step_passed(&mut self, step: &CompiledStep, status: StepStatus) {
        let mut ordinal = 0usize;
        let all_fragments = step.fragments.iter().chain(step.sub_steps.iter().flatten());

        for fragment in all_fragments {
            let created = match fragment {
                Fragment::PortalSet => Some((
                    PendingKind::Portal,
                    format!("Portal placed in {}", step.area_context),
                )),
                Fragment::PortalUse => {
                    self.pending.retain(|p| p.kind != PendingKind::Portal);
                    None
                }
                Fragment::Trial => Some((
                    PendingKind::Trial,
                    format!("Trial of Ascendancy in {}", step.area_context),
                )),
                Fragment::Crafting { .. } if status == StepStatus::Skipped => Some((
                    PendingKind::Crafting,
                    format!("Crafting recipe in {}", step.area_context),
                )),
                Fragment::RewardQuest { item } if status == StepStatus::Skipped => Some((
                    PendingKind::QuestReward,
                    format!("Claim quest reward: {item}"),
                )),
                Fragment::RewardVendor { item, .. } if status == StepStatus::Skipped => Some((
                    PendingKind::VendorReward,
                    format!("Buy from vendor: {item}"),
                )),
                _ => None,
            };
            if let Some((kind, label)) = created {
                let kind_slug = match kind {
                    PendingKind::Portal => "portal",
                    PendingKind::Trial => "trial",
                    PendingKind::QuestReward => "questreward",
                    PendingKind::VendorReward => "vendorreward",
                    PendingKind::Crafting => "crafting",
                };
                self.pending.push(PendingTask {
                    id: format!("{}:{kind_slug}:{ordinal}", step.id),
                    kind,
                    label,
                    zone_area_id: step.area_context.clone(),
                    step_id: step.id.clone(),
                });
                ordinal += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::route_dsl::Fragment;
    use content::walk::CompiledStep;
    use route_engine::StepStatus;

    fn step(id: &str, ctx: &str, frags: Vec<Fragment>) -> CompiledStep {
        CompiledStep {
            id: id.into(),
            act: 1,
            section: "Act 1".into(),
            area_context: ctx.into(),
            fragments: frags,
            sub_steps: vec![],
        }
    }

    #[test]
    fn portal_set_pends_and_portal_use_clears() {
        let mut t = TaskEngine::new();
        t.on_step_passed(
            &step("a1-s001", "1_1_4_1", vec![Fragment::PortalSet]),
            StepStatus::Done,
        );
        assert_eq!(t.pending_count(), 1);
        assert_eq!(t.pending()[0].kind, PendingKind::Portal);
        assert!(t.pending()[0].label.contains("1_1_4_1"));

        t.on_step_passed(
            &step("a1-s005", "1_1_town", vec![Fragment::PortalUse]),
            StepStatus::Done,
        );
        assert_eq!(t.pending_count(), 0);
    }

    #[test]
    fn rewards_pend_only_when_skipped() {
        let mut t = TaskEngine::new();
        let s = step(
            "a1-s010",
            "1_1_town",
            vec![
                Fragment::RewardQuest {
                    item: "Quicksilver Flask".into(),
                },
                Fragment::RewardVendor {
                    item: "Frostblink".into(),
                    cost: None,
                },
            ],
        );
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 0);
        t.on_step_passed(&s, StepStatus::Skipped);
        assert_eq!(t.pending_count(), 2);
        assert_eq!(t.town_reminders().len(), 2);
        assert!(t.pending()[0].label.contains("Quicksilver Flask"));
    }

    #[test]
    fn trial_pends_regardless_of_status_and_ids_are_unique() {
        let mut t = TaskEngine::new();
        let s = step(
            "a2-s020",
            "1_2_4",
            vec![Fragment::Trial, Fragment::PortalSet],
        );
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 2);
        let ids: std::collections::BTreeSet<_> = t.pending().iter().map(|p| p.id.clone()).collect();
        assert_eq!(ids.len(), 2);
        // Trials are not town reminders; portals aren't either.
        assert!(t.town_reminders().is_empty());
    }

    #[test]
    fn sub_step_fragments_are_scanned_too() {
        let mut t = TaskEngine::new();
        let mut s = step("a1-s030", "1_1_7_1", vec![]);
        s.sub_steps = vec![vec![Fragment::Trial]];
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 1);
    }

    #[test]
    fn crafting_pends_only_when_skipped() {
        let mut t = TaskEngine::new();
        let s = step(
            "a2-s040",
            "1_2_5",
            vec![Fragment::Crafting { area_id: None }],
        );
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 0);
        t.on_step_passed(&s, StepStatus::Skipped);
        assert_eq!(t.pending_count(), 1);
        assert_eq!(t.town_reminders().len(), 1);
    }
}
