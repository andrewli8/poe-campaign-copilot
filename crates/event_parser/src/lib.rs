//! Parses English Client.txt lines into typed events. Total: unknown
//! lines become fingerprinted Unknown events, never errors.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawEvent {
    AreaGenerated {
        area_id: String,
        area_level: u8,
        seed: u64,
        at: String,
    },
    AreaEnteredName {
        display_name: String,
        at: String,
    },
    LevelUp {
        character: String,
        class: String,
        level: u16,
        at: String,
    },
    Slain {
        character: String,
        at: String,
    },
    Unknown {
        fingerprint: String,
        at: String,
    },
}

pub fn fingerprint(message: &str) -> String {
    message
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

pub fn parse_line(line: &str) -> RawEvent {
    let at = line.get(..19).unwrap_or("").to_string();
    let unknown = |at: String| RawEvent::Unknown {
        fingerprint: fingerprint(line),
        at,
    };

    let Some(bracket_end) = line.find("] ") else {
        return unknown(at);
    };
    let msg = &line[bracket_end + 2..];

    if let Some(rest) = msg.strip_prefix(": ") {
        if let Some(name) = rest
            .strip_prefix("You have entered ")
            .and_then(|s| s.strip_suffix('.'))
        {
            return RawEvent::AreaEnteredName {
                display_name: name.to_string(),
                at,
            };
        }
        if let Some(idx) = rest.find(" is now level ") {
            let who = &rest[..idx];
            let level_str = &rest[idx + " is now level ".len()..];
            if let (Some(open), Some(close)) = (who.rfind(" ("), who.rfind(')'))
                && close == who.len() - 1
                && open + 2 < close
                && let Ok(level) = level_str.trim().parse::<u16>()
            {
                return RawEvent::LevelUp {
                    character: who[..open].to_string(),
                    class: who[open + 2..close].to_string(),
                    level,
                    at,
                };
            }
        }
        if let Some(character) = rest.strip_suffix(" has been slain.") {
            return RawEvent::Slain {
                character: character.to_string(),
                at,
            };
        }
        return unknown(at);
    }

    if let Some(rest) = msg.strip_prefix("Generating level ") {
        // rest: `1 area "1_1_1" with seed 1780194933`
        let parts: Option<RawEvent> = (|| {
            let (level_str, rest) = rest.split_once(" area \"")?;
            let (area_id, rest) = rest.split_once('"')?;
            let seed_str = rest.strip_prefix(" with seed ")?;
            Some(RawEvent::AreaGenerated {
                area_id: area_id.to_string(),
                area_level: level_str.trim().parse().ok()?,
                seed: seed_str.trim().parse().ok()?,
                at: at.clone(),
            })
        })();
        if let Some(ev) = parts {
            return ev;
        }
    }

    unknown(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEN: &str = r#"2023/12/09 20:15:19 76512546 1186a0e0 [DEBUG Client 22384] Generating level 1 area "1_1_1" with seed 1780194933"#;
    const ENTER: &str = "2023/12/09 20:15:22 76515625 f22b6b26 [INFO Client 22384] : You have entered The Twilight Strand.";
    const LEVEL: &str = "2023/12/09 20:18:40 76713640 f22b6b26 [INFO Client 22384] : Wanderer (Ranger) is now level 2";
    const SLAIN: &str =
        "2023/12/09 20:20:01 76794001 f22b6b26 [INFO Client 22384] : Wanderer has been slain.";

    #[test]
    fn parses_area_generated() {
        assert_eq!(
            parse_line(GEN),
            RawEvent::AreaGenerated {
                area_id: "1_1_1".into(),
                area_level: 1,
                seed: 1780194933,
                at: "2023/12/09 20:15:19".into(),
            }
        );
    }

    #[test]
    fn parses_area_entered_name() {
        assert_eq!(
            parse_line(ENTER),
            RawEvent::AreaEnteredName {
                display_name: "The Twilight Strand".into(),
                at: "2023/12/09 20:15:22".into(),
            }
        );
    }

    #[test]
    fn parses_level_up_including_multiword_names() {
        assert_eq!(
            parse_line(LEVEL),
            RawEvent::LevelUp {
                character: "Wanderer".into(),
                class: "Ranger".into(),
                level: 2,
                at: "2023/12/09 20:18:40".into(),
            }
        );
    }

    #[test]
    fn parses_slain() {
        assert_eq!(
            parse_line(SLAIN),
            RawEvent::Slain {
                character: "Wanderer".into(),
                at: "2023/12/09 20:20:01".into(),
            }
        );
    }

    #[test]
    fn unknown_lines_are_fingerprinted_with_digits_masked() {
        let line = "2023/12/09 20:21:00 76800000 f22b6b26 [INFO Client 22384] : Trade accepted.";
        match parse_line(line) {
            RawEvent::Unknown { fingerprint: f, at } => {
                assert_eq!(at, "2023/12/09 20:21:00");
                assert!(f.contains("Trade accepted."));
                assert!(!f.chars().any(|c| c.is_ascii_digit()));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn garbage_and_short_lines_never_panic() {
        for junk in ["", "x", ": ", "]] ", "no brackets here", "2023/12/09"] {
            let _ = parse_line(junk);
        }
    }

    #[test]
    fn area_names_with_apostrophes_survive() {
        let line = "2023/12/09 21:00:00 77000000 f22b6b26 [INFO Client 22384] : You have entered The Slaver's Pits.";
        assert_eq!(
            parse_line(line),
            RawEvent::AreaEnteredName {
                display_name: "The Slaver's Pits".into(),
                at: "2023/12/09 21:00:00".into(),
            }
        );
    }
}
