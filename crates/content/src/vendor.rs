//! Locations of vendored exile-leveling data (see tools/sync-exile-leveling.sh).

use std::path::PathBuf;

use crate::data_root::data_root;

pub fn vendor_dir() -> PathBuf {
    data_root().join("vendor").join("exile-leveling")
}

pub fn read_act_route(act: u8) -> std::io::Result<String> {
    std::fs::read_to_string(vendor_dir().join("routes").join(format!("act-{act}.txt")))
}
