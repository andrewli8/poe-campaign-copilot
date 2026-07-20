use std::collections::BTreeSet;

use serde::Serialize;
use thiserror::Error;

use super::fragment::{Fragment, FragmentError, parse_fragments};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub line: usize,
    pub fragments: Vec<Fragment>,
    pub sub_steps: Vec<Vec<Fragment>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error(transparent)]
    Fragment(#[from] FragmentError),
    #[error("line {line}: unexpected #endif")]
    UnexpectedEndif { line: usize },
    #[error("line {line}: #sub without a preceding step")]
    OrphanSub { line: usize },
    #[error("unterminated #ifdef/#ifndef block")]
    UnterminatedConditional,
}

pub fn parse_route_file(
    source: &str,
    defines: &BTreeSet<String>,
) -> Result<Vec<Section>, ParseError> {
    let mut sections: Vec<Section> = Vec::new();
    let mut cond_stack: Vec<bool> = Vec::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        let active = cond_stack.iter().all(|v| *v);

        if let Some(name) = trimmed.strip_prefix("#section") {
            if !cond_stack.is_empty() {
                return Err(ParseError::UnterminatedConditional);
            }
            let name = name.trim();
            let name = if name.is_empty() { "Default" } else { name };
            sections.push(Section {
                name: name.to_string(),
                steps: Vec::new(),
            });
            continue;
        }
        if trimmed.starts_with("#endif") {
            if cond_stack.pop().is_none() {
                return Err(ParseError::UnexpectedEndif { line: line_no });
            }
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("#ifdef") {
            cond_stack.push(defines.contains(name.trim()));
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("#ifndef") {
            cond_stack.push(!defines.contains(name.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#sub") {
            if !active {
                continue;
            }
            let fragments = parse_fragments(rest.trim(), line_no)?;
            if fragments.is_empty() {
                continue;
            }
            let step = sections
                .last_mut()
                .and_then(|s| s.steps.last_mut())
                .ok_or(ParseError::OrphanSub { line: line_no })?;
            step.sub_steps.push(fragments);
            continue;
        }
        if trimmed.starts_with('#') {
            continue; // comment line
        }
        if !active {
            continue;
        }

        let fragments = parse_fragments(trimmed, line_no)?;
        if fragments.is_empty() {
            continue;
        }
        if sections.is_empty() {
            sections.push(Section {
                name: "Default".to_string(),
                steps: Vec::new(),
            });
        }
        sections
            .last_mut()
            .expect("section exists")
            .steps
            .push(Step {
                line: line_no,
                fragments,
                sub_steps: Vec::new(),
            });
    }

    if !cond_stack.is_empty() {
        return Err(ParseError::UnterminatedConditional);
    }

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_dsl::Fragment;
    use std::collections::BTreeSet;

    fn defines(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    const SAMPLE: &str = "\
#section Act 1
Find and kill {kill|Hillock}
➞ {enter|1_1_town} #Lioneye's Watch
#ifdef LEAGUE_START
    Get {waypoint_get}
#endif
#ifndef LEAGUE_START
    {waypoint|1_1_town}
#endif
Find bridge, place {portal|set}
    #sub Go {dir|270}
    #sub Recommended Level: 4
";

    #[test]
    fn splits_sections_and_steps() {
        let sections = parse_route_file(SAMPLE, &defines(&["LEAGUE_START"])).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "Act 1");
        // steps: Hillock, enter town, waypoint_get (ifdef active), portal set
        assert_eq!(sections[0].steps.len(), 4);
        assert_eq!(
            sections[0].steps[2].fragments,
            vec![
                Fragment::Text {
                    value: "Get ".into()
                },
                Fragment::WaypointGet,
            ]
        );
    }

    #[test]
    fn ifndef_branch_taken_without_define() {
        let sections = parse_route_file(SAMPLE, &defines(&[])).unwrap();
        // steps: Hillock, enter town, waypoint use (ifndef active), portal set
        assert_eq!(sections[0].steps.len(), 4);
        assert_eq!(
            sections[0].steps[2].fragments,
            vec![Fragment::WaypointUse {
                area_id: "1_1_town".into()
            }]
        );
    }

    #[test]
    fn subs_attach_to_previous_step() {
        let sections = parse_route_file(SAMPLE, &defines(&["LEAGUE_START"])).unwrap();
        let last = sections[0].steps.last().unwrap();
        assert_eq!(last.sub_steps.len(), 2);
        assert_eq!(
            last.sub_steps[0],
            vec![
                Fragment::Text {
                    value: "Go ".into()
                },
                Fragment::Dir { dir_index: 6 },
            ]
        );
        assert_eq!(
            last.sub_steps[1],
            vec![Fragment::Text {
                value: "Recommended Level: 4".into()
            }]
        );
    }

    #[test]
    fn implicit_default_section_when_no_header() {
        let sections = parse_route_file("{logout}\n", &defines(&[])).unwrap();
        assert_eq!(sections[0].name, "Default");
        assert_eq!(sections[0].steps.len(), 1);
    }

    #[test]
    fn errors() {
        assert!(matches!(
            parse_route_file("#endif\n", &defines(&[])),
            Err(ParseError::UnexpectedEndif { line: 1 })
        ));
        assert!(matches!(
            parse_route_file("#section X\n    #sub {dir|90}\n", &defines(&[])),
            Err(ParseError::OrphanSub { line: 2 })
        ));
        assert!(matches!(
            parse_route_file("#ifdef LEAGUE_START\n{logout}\n", &defines(&[])),
            Err(ParseError::UnterminatedConditional)
        ));
    }

    #[test]
    fn inactive_sub_lines_are_skipped() {
        let src =
            "#section S\n{logout}\n#ifdef X\n    step {generic|inner}\n    #sub hint\n#endif\n";
        let sections = parse_route_file(src, &defines(&[])).unwrap();
        assert_eq!(sections[0].steps.len(), 1); // only logout
        assert!(sections[0].steps[0].sub_steps.is_empty());
    }

    #[test]
    fn nested_conditionals_all_must_be_true() {
        let src = "#ifdef A\n#ifdef B\n{logout}\n#endif\n#endif\n";

        // Both A and B defined: step should appear
        let sections = parse_route_file(src, &defines(&["A", "B"])).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].steps.len(), 1);

        // Only A defined (B false): step should be skipped (no default section created)
        let sections = parse_route_file(src, &defines(&["A"])).unwrap();
        assert_eq!(sections.len(), 0);

        // Only B defined (outer A false, inner B true): step should still be skipped
        // because the outer condition (A) is false, making active = false AND true = false
        let sections = parse_route_file(src, &defines(&["B"])).unwrap();
        assert_eq!(sections.len(), 0);
    }

    #[test]
    fn section_in_open_conditional_errors() {
        let src = "#ifdef X\n#section Y\n";
        assert!(matches!(
            parse_route_file(src, &defines(&[])),
            Err(ParseError::UnterminatedConditional)
        ));
    }
}
