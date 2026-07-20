//! Content pipeline: route DSL parsing, game data, and content-pack
//! compilation for PoE Campaign Copilot.

pub fn crate_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() {
        assert!(super::crate_ready());
    }
}
