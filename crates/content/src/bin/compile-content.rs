//! Writes compiled route content packs to content-pack/routes/.

use std::path::PathBuf;

use content::compile::{Variant, compile_layout_pack, compile_route_pack};
use content::layouts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content-pack")
        .join("routes");
    std::fs::create_dir_all(&out_dir)?;

    for variant in [Variant::LeagueStart, Variant::Standard] {
        let pack = compile_route_pack(variant)?;
        let path = out_dir.join(format!("{}.json", variant.file_stem()));
        std::fs::write(&path, serde_json::to_string_pretty(&pack)?)?;
        let steps: usize = pack.acts.iter().map(|a| a.steps.len()).sum();
        println!("wrote {} ({} steps)", path.display(), steps);
    }

    let layout_pack = compile_layout_pack()?;
    let layouts_path = out_dir.parent().unwrap().join("layouts.json");
    std::fs::write(&layouts_path, serde_json::to_string_pretty(&layout_pack)?)?;
    println!(
        "wrote {} ({} entries)",
        layouts_path.display(),
        layout_pack.entries.len()
    );

    let asset_src = layouts::layouts_dir().join("assets");
    let asset_dst = out_dir.parent().unwrap().join("assets");
    std::fs::create_dir_all(&asset_dst)?;
    let mut copied = 0usize;
    for f in std::fs::read_dir(&asset_src)? {
        let p = f?.path();
        if p.extension().and_then(|e| e.to_str()) == Some("png") {
            std::fs::copy(&p, asset_dst.join(p.file_name().unwrap()))?;
            copied += 1;
        }
    }
    println!("copied {copied} layout assets");

    Ok(())
}
