use std::collections::BTreeSet;

use content::game_data::load_vendored;
use content::layouts::{AuditStatus, layouts_dir, load_all_layouts};

#[test]
fn all_layout_entries_are_valid() {
    let (areas, _) = load_vendored().unwrap();
    let entries = load_all_layouts().expect("layouts load");

    assert!(
        (120..=132).contains(&entries.len()),
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
        assert_eq!(
            e.audit.status,
            AuditStatus::Unaudited,
            "{}: initial content must be unaudited",
            e.area_id
        );
        assert!(
            !e.notes.is_empty() || !e.descriptions.is_empty(),
            "{}: entry has no text at all",
            e.area_id
        );
        for img in &e.images {
            let p = layouts_dir().join("assets").join(img);
            assert!(p.is_file(), "{}: missing image {}", e.area_id, img);
            assert!(
                img.ends_with(".png"),
                "{}: non-png image {}",
                e.area_id,
                img
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
