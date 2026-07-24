use std::collections::BTreeSet;

use content::game_data::load_vendored;
use content::layouts::{AuditStatus, EXPECTED_ENTRY_COUNT_RANGE, layouts_dir, load_all_layouts};

#[test]
fn all_layout_entries_are_valid() {
    let (areas, _) = load_vendored().unwrap();
    let entries = load_all_layouts().expect("layouts load");

    assert!(
        EXPECTED_ENTRY_COUNT_RANGE.contains(&entries.len()),
        "unexpected entry count {}",
        entries.len()
    );

    let mut acts = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    for e in &entries {
        let area = areas
            .get(&e.area_id)
            .unwrap_or_else(|| panic!("{}: unknown area id", e.area_id));
        assert_eq!(area.act, e.act, "{}: act mismatch", e.area_id);
        assert_eq!(area.name, e.display_name, "{}: name mismatch", e.area_id);
        assert!(
            seen_ids.insert(e.area_id.clone()),
            "{}: duplicate entry",
            e.area_id
        );
        assert!(
            !e.notes.is_empty() || !e.descriptions.is_empty(),
            "{}: entry has no text at all",
            e.area_id
        );
        // Content is audited zone-by-zone (see the audit metadata in each
        // layout JSON). Any status is allowed, but a `corrected` note or
        // description MUST supply the replacement text the composer shows in
        // place of the original — a corrected item with no correction would
        // silently blank the guidance.
        for item in e.descriptions.iter().chain(e.notes.iter()) {
            if item.audit.status == AuditStatus::Corrected {
                assert!(
                    item.audit
                        .correction
                        .as_deref()
                        .is_some_and(|c| !c.trim().is_empty()),
                    "{}: a corrected note/description must supply a non-empty correction",
                    e.area_id
                );
            }
        }
        for img in &e.images {
            let p = layouts_dir().join("assets").join(&img.file);
            assert!(p.is_file(), "{}: missing image {}", e.area_id, img.file);
            assert!(
                img.file.ends_with(".png"),
                "{}: non-png image {}",
                e.area_id,
                img.file
            );
        }
        acts.insert(e.act);
    }
    assert_eq!(
        acts.into_iter().collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>(),
        "every act must have layout entries"
    );

    let with_images = entries.iter().filter(|e| !e.images.is_empty()).count();
    assert!(with_images >= 80, "only {with_images} entries have images");
}

/// Issue #11: Navali was removed from the game in 3.17 (with the prophecy
/// system). Any note/description that still references her must be audited —
/// `corrected` (replaced with accurate guidance) or `outdated` (struck
/// through) — never left as active `unaudited`/`verified` guidance telling
/// players to free her.
#[test]
fn no_active_note_references_the_removed_navali() {
    let entries = load_all_layouts().expect("layouts load");
    for e in &entries {
        for item in e.descriptions.iter().chain(e.notes.iter()) {
            if item.text.contains("Navali") {
                assert!(
                    matches!(
                        item.audit.status,
                        AuditStatus::Corrected | AuditStatus::Outdated
                    ),
                    "{}: note references the removed NPC Navali but is still active ({:?}); \
                     mark it corrected or outdated",
                    e.area_id,
                    item.audit.status,
                );
            }
        }
    }
}
