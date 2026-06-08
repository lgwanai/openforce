//! SKILL.md YAML frontmatter parser and body extractor.

use crate::SkillFrontmatter;

/// Parse YAML frontmatter from a SKILL.md file.
/// Returns None if the file doesn't contain valid frontmatter with required fields.
pub fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return None;
    }

    let yaml_block = parts[1].trim();
    if yaml_block.is_empty() {
        return None;
    }

    // Manual YAML parsing — avoids adding a YAML dependency.
    // The frontmatter is simple key-value pairs; we handle:
    //   name: value
    //   description: long text that may span multiple lines
    //   allowed-tools: (not parsed here, handled below)
    //   version: value
    //   author: value
    //   license: value
    let mut name = String::new();
    let mut description = String::new();
    let mut allowed_tools: Option<Vec<String>> = None;
    let mut version: Option<String> = None;
    let mut author: Option<String> = None;
    let mut license: Option<String> = None;

    let mut in_description = false;
    let mut description_lines: Vec<String> = Vec::new();

    for line in yaml_block.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if in_description && !description_lines.is_empty() {
                in_description = false;
            }
            continue;
        }

        // Parse key: value
        if let Some(v) = parse_key_value(trimmed, "name:") {
            name = v;
            in_description = false;
        } else if let Some(v) = parse_key_value(trimmed, "description:") {
            // Description might be on a single line or start here and continue
            if !v.is_empty() {
                description = v;
                in_description = false;
            } else {
                in_description = true;
            }
        } else if let Some(v) = parse_key_value(trimmed, "version:") {
            version = Some(v);
            in_description = false;
        } else if let Some(v) = parse_key_value(trimmed, "author:") {
            author = Some(v);
            in_description = false;
        } else if let Some(v) = parse_key_value(trimmed, "license:") {
            license = Some(v);
            in_description = false;
        } else if trimmed.starts_with("allowed-tools:") || trimmed.starts_with("allowed_tools:") {
            // Parse allowed-tools: [tool1, tool2] or allowed-tools:\n  - tool1\n  - tool2
            let rest = trimmed.split_once(':').map(|(_, r)| r.trim()).unwrap_or("");
            if rest.starts_with('[') {
                // Inline array: [tool1, tool2]
                allowed_tools = Some(parse_inline_array(rest));
            } else if rest.is_empty() {
                // Multi-line array follows
                allowed_tools = Some(vec![]);
            }
            in_description = false;
        } else if in_description {
            // Continuation of multi-line description
            description_lines.push(trimmed.to_string());
        } else if allowed_tools.is_some() && trimmed.starts_with("- ") {
            // YAML list item for allowed-tools
            let tool = trimmed
                .trim_start_matches("- ")
                .trim()
                .trim_matches('"')
                .to_string();
            if let Some(ref mut tools) = allowed_tools {
                tools.push(tool);
            }
        }
    }

    // If we collected multi-line description
    if description.is_empty() && !description_lines.is_empty() {
        description = description_lines.join(" ");
    }

    if name.is_empty() {
        return None;
    }

    Some(SkillFrontmatter {
        name,
        description,
        allowed_tools,
        version,
        author,
        metadata: None,
        license,
    })
}

/// Extract the body content after the YAML frontmatter.
/// Returns the Markdown content after the second `---` fence.
pub fn extract_body(content: &str) -> String {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return content.to_string();
    }
    parts[2].trim().to_string()
}

/// Parse `key: value` returning the value (unquoted).
fn parse_key_value(line: &str, key: &str) -> Option<String> {
    if line.starts_with(key) {
        let rest = line[key.len()..].trim();
        // Remove surrounding quotes
        let value = rest
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| rest.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(rest);
        Some(value.to_string())
    } else {
        None
    }
}

/// Parse an inline YAML array like `[tool1, tool2, tool3]`.
fn parse_inline_array(s: &str) -> Vec<String> {
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_frontmatter() {
        let content = "---\nname: test-skill\ndescription: A test skill\n---\nBody here";
        let fm = parse_frontmatter(content).expect("should parse");
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description, "A test skill");
        assert!(fm.allowed_tools.is_none());
        assert!(fm.version.is_none());
    }

    #[test]
    fn test_parse_full_frontmatter() {
        let content = "---\nname: deploy\nversion: \"1.0\"\nauthor: openforce\nlicense: MIT\ndescription: Deploy to production\nallowed-tools: [web_search, web_fetch]\n---\nBody";
        let fm = parse_frontmatter(content).expect("should parse");
        assert_eq!(fm.name, "deploy");
        assert_eq!(fm.version.as_deref(), Some("1.0"));
        assert_eq!(fm.author.as_deref(), Some("openforce"));
        assert_eq!(fm.license.as_deref(), Some("MIT"));
        assert_eq!(
            fm.allowed_tools.as_deref(),
            Some(&["web_search".to_string(), "web_fetch".to_string()][..])
        );
    }

    #[test]
    fn test_parse_multiline_allowed_tools() {
        let content =
            "---\nname: test\ndescription: test\nallowed-tools:\n  - tool_a\n  - tool_b\n---\nBody";
        let fm = parse_frontmatter(content).expect("should parse");
        assert_eq!(
            fm.allowed_tools.as_deref(),
            Some(&["tool_a".to_string(), "tool_b".to_string()][..])
        );
    }

    #[test]
    fn test_extract_body() {
        let content = "---\nname: test\ndescription: test\n---\n# Instructions\nDo the thing.";
        let body = extract_body(content);
        assert!(body.contains("# Instructions"));
        assert!(body.contains("Do the thing."));
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "Just some markdown without frontmatter";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn test_missing_name() {
        let content = "---\ndescription: no name\n---\nBody";
        assert!(parse_frontmatter(content).is_none());
    }
}
