use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fragment {
    Text {
        value: String,
    },
    Kill {
        name: String,
    },
    Arena {
        name: String,
    },
    Area {
        area_id: String,
    },
    Enter {
        area_id: String,
    },
    Logout,
    Waypoint,
    WaypointUse {
        area_id: String,
    },
    WaypointGet,
    PortalSet,
    PortalUse,
    Quest {
        quest_id: String,
        reward_offer_ids: Vec<String>,
    },
    QuestText {
        text: String,
    },
    Generic {
        text: String,
    },
    RewardQuest {
        item: String,
    },
    RewardVendor {
        item: String,
        cost: Option<String>,
    },
    Trial,
    Ascend {
        version: String,
    },
    Crafting {
        area_id: Option<String>,
    },
    Dir {
        dir_index: u8,
    },
    Copy {
        text: String,
    },
}

#[derive(Debug, Error, PartialEq)]
pub enum FragmentError {
    #[error("line {line}: unknown fragment kind '{kind}'")]
    UnknownKind { line: usize, kind: String },
    #[error("line {line}: invalid arity for '{kind}'")]
    InvalidArity { line: usize, kind: String },
    #[error("line {line}: unterminated fragment")]
    Unterminated { line: usize },
    #[error("line {line}: invalid dir value '{value}'")]
    InvalidDir { line: usize, value: String },
}

pub fn parse_fragments(text: &str, line_no: usize) -> Result<Vec<Fragment>, FragmentError> {
    let mut fragments = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        if rest.starts_with('#') {
            break; // comment: rest of line ignored
        }
        if let Some(after_brace) = rest.strip_prefix('{') {
            let end = after_brace
                .find('}')
                .ok_or(FragmentError::Unterminated { line: line_no })?;
            let inner = &after_brace[..end];
            fragments.push(parse_one(inner, line_no)?);
            rest = &after_brace[end + 1..];
        } else {
            let stop = rest
                .char_indices()
                .find(|(_, c)| *c == '{' || *c == '#')
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            let (chunk, tail) = rest.split_at(stop);
            if !chunk.trim().is_empty() {
                fragments.push(Fragment::Text {
                    value: chunk.to_string(),
                });
            }
            rest = tail;
        }
    }

    Ok(fragments)
}

fn parse_one(inner: &str, line: usize) -> Result<Fragment, FragmentError> {
    let parts: Vec<&str> = inner.split('|').collect();
    let kind = parts[0];
    let arity = parts.len() - 1;

    let arity_err = || FragmentError::InvalidArity {
        line,
        kind: kind.to_string(),
    };

    let frag = match kind {
        "kill" if arity == 1 => Fragment::Kill {
            name: parts[1].to_string(),
        },
        "arena" if arity == 1 => Fragment::Arena {
            name: parts[1].to_string(),
        },
        "area" if arity == 1 => Fragment::Area {
            area_id: parts[1].to_string(),
        },
        "enter" if arity == 1 => Fragment::Enter {
            area_id: parts[1].to_string(),
        },
        "logout" if arity == 0 => Fragment::Logout,
        "waypoint" if arity == 0 => Fragment::Waypoint,
        "waypoint" if arity == 1 => Fragment::WaypointUse {
            area_id: parts[1].to_string(),
        },
        "waypoint_get" if arity == 0 => Fragment::WaypointGet,
        "portal" if arity == 1 && parts[1] == "set" => Fragment::PortalSet,
        "portal" if arity == 1 && parts[1] == "use" => Fragment::PortalUse,
        "quest" if arity >= 1 => Fragment::Quest {
            quest_id: parts[1].to_string(),
            reward_offer_ids: parts[2..].iter().map(|s| s.to_string()).collect(),
        },
        "quest_text" if arity == 1 => Fragment::QuestText {
            text: parts[1].to_string(),
        },
        "generic" if arity == 1 => Fragment::Generic {
            text: parts[1].to_string(),
        },
        "reward_quest" if arity == 1 => Fragment::RewardQuest {
            item: parts[1].to_string(),
        },
        "reward_vendor" if (1..=2).contains(&arity) => Fragment::RewardVendor {
            item: parts[1].to_string(),
            cost: parts.get(2).map(|s| s.to_string()),
        },
        "trial" if arity == 0 => Fragment::Trial,
        "ascend" if arity == 1 => Fragment::Ascend {
            version: parts[1].to_string(),
        },
        "crafting" if arity <= 1 => Fragment::Crafting {
            area_id: parts.get(1).map(|s| s.to_string()),
        },
        "dir" if arity == 1 => {
            let value = parts[1];
            let parsed: f64 = value.parse().map_err(|_| FragmentError::InvalidDir {
                line,
                value: value.to_string(),
            })?;
            let mut deg = parsed % 360.0;
            if deg < 0.0 {
                deg += 360.0;
            }
            if deg % 45.0 != 0.0 {
                return Err(FragmentError::InvalidDir {
                    line,
                    value: value.to_string(),
                });
            }
            Fragment::Dir {
                dir_index: (deg / 45.0) as u8,
            }
        }
        "copy" if arity >= 1 => Fragment::Copy {
            text: parts[1..].concat(),
        },
        "kill" | "arena" | "area" | "enter" | "logout" | "waypoint" | "waypoint_get" | "portal"
        | "quest" | "quest_text" | "generic" | "reward_quest" | "reward_vendor" | "trial"
        | "ascend" | "crafting" | "dir" | "copy" => return Err(arity_err()),
        _ => {
            return Err(FragmentError::UnknownKind {
                line,
                kind: kind.to_string(),
            });
        }
    };

    Ok(frag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enter_line_with_arrow_text_and_comment() {
        let frags = parse_fragments("➞ {enter|1_1_2} #The Coast", 1).unwrap();
        assert_eq!(
            frags,
            vec![
                Fragment::Text {
                    value: "➞ ".into()
                },
                Fragment::Enter {
                    area_id: "1_1_2".into()
                },
            ]
        );
    }

    #[test]
    fn parses_mixed_text_and_multiple_fragments() {
        let frags = parse_fragments(
            "Find and kill {kill|Hailrake}, take {quest_text|Medicine Chest}",
            3,
        )
        .unwrap();
        assert_eq!(
            frags,
            vec![
                Fragment::Text {
                    value: "Find and kill ".into()
                },
                Fragment::Kill {
                    name: "Hailrake".into()
                },
                Fragment::Text {
                    value: ", take ".into()
                },
                Fragment::QuestText {
                    text: "Medicine Chest".into()
                },
            ]
        );
    }

    #[test]
    fn waypoint_arity_selects_variant() {
        assert_eq!(
            parse_fragments("{waypoint}", 1).unwrap(),
            vec![Fragment::Waypoint]
        );
        assert_eq!(
            parse_fragments("{waypoint|1_1_2}", 1).unwrap(),
            vec![Fragment::WaypointUse {
                area_id: "1_1_2".into()
            }]
        );
    }

    #[test]
    fn quest_collects_reward_offer_ids() {
        assert_eq!(
            parse_fragments("{quest|a4q2|a4q2b}", 1).unwrap(),
            vec![Fragment::Quest {
                quest_id: "a4q2".into(),
                reward_offer_ids: vec!["a4q2b".into()],
            }]
        );
        assert_eq!(
            parse_fragments("{quest|a1q1}", 1).unwrap(),
            vec![Fragment::Quest {
                quest_id: "a1q1".into(),
                reward_offer_ids: vec![]
            }]
        );
    }

    #[test]
    fn dir_converts_degrees_to_index() {
        assert_eq!(
            parse_fragments("{dir|270}", 1).unwrap(),
            vec![Fragment::Dir { dir_index: 6 }]
        );
        assert_eq!(
            parse_fragments("{dir|45}", 1).unwrap(),
            vec![Fragment::Dir { dir_index: 1 }]
        );
        // -45 normalizes to 315 -> index 7
        assert_eq!(
            parse_fragments("{dir|-45}", 1).unwrap(),
            vec![Fragment::Dir { dir_index: 7 }]
        );
        assert!(matches!(
            parse_fragments("{dir|50}", 9),
            Err(FragmentError::InvalidDir { line: 9, .. })
        ));
    }

    #[test]
    fn portal_and_zero_arity_fragments() {
        assert_eq!(
            parse_fragments("{portal|set}", 1).unwrap(),
            vec![Fragment::PortalSet]
        );
        assert_eq!(
            parse_fragments("{portal|use}", 1).unwrap(),
            vec![Fragment::PortalUse]
        );
        assert_eq!(
            parse_fragments("{logout}", 1).unwrap(),
            vec![Fragment::Logout]
        );
        assert_eq!(
            parse_fragments("{waypoint_get}", 1).unwrap(),
            vec![Fragment::WaypointGet]
        );
        assert_eq!(
            parse_fragments("{trial}", 1).unwrap(),
            vec![Fragment::Trial]
        );
    }

    #[test]
    fn reward_vendor_optional_cost() {
        assert_eq!(
            parse_fragments("{reward_vendor|Frostblink|wisdom}", 1).unwrap(),
            vec![Fragment::RewardVendor {
                item: "Frostblink".into(),
                cost: Some("wisdom".into())
            }]
        );
        assert_eq!(
            parse_fragments("{reward_vendor|Frostblink}", 1).unwrap(),
            vec![Fragment::RewardVendor {
                item: "Frostblink".into(),
                cost: None
            }]
        );
    }

    #[test]
    fn crafting_optional_area() {
        assert_eq!(
            parse_fragments("{crafting}", 1).unwrap(),
            vec![Fragment::Crafting { area_id: None }]
        );
        assert_eq!(
            parse_fragments("{crafting|1_2_5}", 1).unwrap(),
            vec![Fragment::Crafting {
                area_id: Some("1_2_5".into())
            }]
        );
    }

    #[test]
    fn errors_on_unknown_kind_and_unterminated() {
        assert!(matches!(
            parse_fragments("{bogus|x}", 7),
            Err(FragmentError::UnknownKind { line: 7, .. })
        ));
        assert!(matches!(
            parse_fragments("{enter|1_1_2", 8),
            Err(FragmentError::Unterminated { line: 8 })
        ));
        assert!(matches!(
            parse_fragments("{logout|extra}", 2),
            Err(FragmentError::InvalidArity { line: 2, .. })
        ));
    }

    #[test]
    fn comment_only_and_empty_lines_yield_nothing() {
        assert_eq!(parse_fragments("#just a comment", 1).unwrap(), vec![]);
        assert_eq!(parse_fragments("   ", 1).unwrap(), vec![]);
        assert_eq!(parse_fragments("", 1).unwrap(), vec![]);
    }
}
