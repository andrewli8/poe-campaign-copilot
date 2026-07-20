//! Writes compiled route content packs to content-pack/routes/.

use std::path::PathBuf;

use content::compile::{Variant, compile_route_pack};

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

    Ok(())
}
