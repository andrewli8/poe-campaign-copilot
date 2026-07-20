//! Locations of vendored exile-leveling data (see tools/sync-exile-leveling.sh).

use std::path::PathBuf;

pub fn vendor_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("vendor")
        .join("exile-leveling")
}

pub fn read_act_route(act: u8) -> std::io::Result<String> {
    std::fs::read_to_string(vendor_dir().join("routes").join(format!("act-{act}.txt")))
}
