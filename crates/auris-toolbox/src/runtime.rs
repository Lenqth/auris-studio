//! Runtime-scoped discovery, help and dispatch from the same execution registry.

use crate::{ToolGroups, tool_catalog, tool_group};
use serde_json::{Value, json};

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Empty {}

/// Reference tools available through every live transport.
pub fn live_references() -> Vec<crate::live_agent::Definition> {
    [
        ("search_documentation", crate::search_documentation::DESCRIPTION, crate::parameter_schema::<crate::search_documentation::Args>()),
        ("discover_tools", crate::discover_tools::DESCRIPTION, crate::parameter_schema::<crate::discover_tools::Args>()),
        ("tool_help", crate::tool_help::DESCRIPTION, crate::parameter_schema::<crate::tool_help::Args>()),
        ("capabilities", "Read the current live tool catalog and supported task groups. No project or file edits.", crate::parameter_schema::<Empty>()),
    ].into_iter().map(|(name, description, parameters)| crate::live_agent::Definition {
        name: name.into(), description: description.into(), parameters,
    }).collect()
}

/// Read a live reference without opening or changing a saved project.
pub fn call_live_reference(name: &str, args: Value) -> Option<Result<String, String>> {
    let run = || -> Result<String, String> {
        let invalid = |e: serde_json::Error| format!("Invalid arguments for {name}: {e}");
        match name {
            "search_documentation" => {
                crate::search_documentation::run(&serde_json::from_value(args).map_err(invalid)?)
            }
            "capabilities" => {
                let _: Empty = serde_json::from_value(args).map_err(invalid)?;
                let names: Vec<_> = crate::live_agent::definitions()
                    .into_iter()
                    .filter(|d| d.name != "list_instruments")
                    .map(|d| d.name)
                    .collect();
                Ok(json!({"context":"open_document","tools":names}).to_string())
            }
            "tool_help" => {
                let request: crate::tool_help::Args =
                    serde_json::from_value(args).map_err(invalid)?;
                let definition = crate::live_agent::definitions()
                    .into_iter()
                    .find(|d| d.name == request.name && d.name != "list_instruments")
                    .ok_or_else(|| format!("Unknown live tool: {}", request.name))?;
                Ok(json!({"name":definition.name,"description":definition.description,"parameters":definition.parameters}).to_string())
            }
            "discover_tools" => {
                let request: crate::discover_tools::Args =
                    serde_json::from_value(args).map_err(invalid)?;
                if request.group.as_deref().is_some_and(|g| {
                    ![
                        "common",
                        "manual",
                        "mix",
                        "transcription",
                        "vocals",
                        "composition",
                    ]
                    .contains(&g)
                }) {
                    return Err("Unknown task group".into());
                }
                let query = request.query.unwrap_or_default().to_lowercase();
                let definitions: Vec<_> = crate::live_agent::definitions()
                    .into_iter()
                    .filter(|d| {
                        d.name != "list_instruments"
                            && request
                                .group
                                .as_deref()
                                .is_none_or(|g| tool_group(&d.name) == g)
                            && query.split_whitespace().all(|word| {
                                format!("{} {}", d.name, d.description)
                                    .to_lowercase()
                                    .contains(word)
                            })
                    })
                    .collect();
                let tools: Vec<_> = definitions.iter().skip(request.offset).take(10).map(|d| json!({"name":d.name,"group":tool_group(&d.name),"description":crate::concise_description(&d.description)})).collect();
                let next = request.offset.saturating_add(tools.len());
                Ok(json!({"tools":tools,"total":definitions.len(),"next_offset":(next < definitions.len()).then_some(next)}).to_string())
            }
            _ => unreachable!(),
        }
    };
    matches!(
        name,
        "search_documentation" | "tool_help" | "discover_tools" | "capabilities"
    )
    .then(run)
}

/// Execute an enabled saved-project tool. Disabled and unknown names fail before dispatch.
pub fn call_saved(groups: &ToolGroups, name: &str, args: Value) -> Result<String, String> {
    let catalog = tool_catalog();
    let definition = catalog
        .iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| format!("Unknown tool: {name}"))?;
    if !groups.allows(name) {
        return Err(format!(
            "Tool {name} is disabled in this server's startup groups"
        ));
    }
    match name {
        "discover_tools" => discover_saved(
            groups,
            serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {e}"))?,
        ),
        "tool_help" => {
            let request: crate::tool_help::Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {e}"))?;
            if !groups.allows(&request.name) {
                return Err(format!(
                    "Tool {} is disabled; enable group '{}' in AURIS_MCP_TOOL_GROUPS and restart",
                    request.name,
                    tool_group(&request.name)
                ));
            }
            crate::tool_help::run(&request)
        }
        _ => (definition.call)(args),
    }
}

fn discover_saved(
    groups: &ToolGroups,
    args: crate::discover_tools::Args,
) -> Result<String, String> {
    if args.group.as_deref().is_some_and(|g| {
        ![
            "common",
            "manual",
            "mix",
            "transcription",
            "vocals",
            "composition",
        ]
        .contains(&g)
    }) {
        return Err("Unknown task group".into());
    }
    let query = args.query.unwrap_or_default().to_lowercase();
    let matches: Vec<_> = tool_catalog()
        .into_iter()
        .filter(|tool| {
            groups.allows(tool.name)
                && args
                    .group
                    .as_deref()
                    .is_none_or(|group| tool_group(tool.name) == group)
                && query.split_whitespace().all(|word| {
                    format!("{} {}", tool.name, tool.description)
                        .to_lowercase()
                        .contains(word)
                })
        })
        .collect();
    let tools: Vec<_> = matches.iter().skip(args.offset).take(10).map(|tool| json!({"name":tool.name,"group":tool_group(tool.name),"description":crate::concise_description(tool.description)})).collect();
    let next = args.offset.saturating_add(tools.len());
    Ok(json!({"tools":tools,"total":matches.len(),"next_offset":(next < matches.len()).then_some(next)}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_discovery_help_and_capabilities_use_the_live_definitions() {
        let report: Value = serde_json::from_str(
            &call_live_reference("capabilities", json!({}))
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let names = report["tools"].as_array().unwrap();
        assert!(names.contains(&json!("search_documentation")));
        assert!(!names.contains(&json!("compose")));
        let help: Value = serde_json::from_str(
            &call_live_reference("tool_help", json!({"name":"set_performance"}))
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(help["parameters"]["properties"].get("project").is_none());
        assert!(
            call_live_reference("tool_help", json!({"name":"compose"}))
                .unwrap()
                .is_err()
        );
        assert!(
            call_live_reference("capabilities", json!({"project":"/song.auris"}))
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn manual_discovery_and_help_do_not_recommend_disabled_composition() {
        let groups = ToolGroups::parse("manual").unwrap();
        let text = call_saved(&groups, "discover_tools", json!({"query":"compose"})).unwrap();
        let report: Value = serde_json::from_str(&text).unwrap();
        assert!(report["tools"].as_array().unwrap().is_empty());
        assert!(
            call_saved(&groups, "tool_help", json!({"name":"compose"}))
                .unwrap_err()
                .contains("disabled")
        );
        assert!(
            call_saved(&groups, "compose", json!({}))
                .unwrap_err()
                .contains("disabled")
        );
    }
}
