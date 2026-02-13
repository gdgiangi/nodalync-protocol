use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Folder name → entity_type mapping for Nodes/ subdirectories
pub fn folder_to_entity_type(folder: &str) -> Option<&'static str> {
    match folder {
        "People" => Some("Person"),
        "Organizations" => Some("Organization"),
        "Products" => Some("Product"),
        "Research" => Some("Research"),
        "Ideas" => Some("Concept"),
        "Insights" => Some("Concept"),
        "Patterns" => Some("Pattern"),
        "Decisions" => Some("Decision"),
        "Tasks" => Some("Task"),
        "Assets" => Some("Asset"),
        "Bets" => Some("Bet"),
        "Commitments" => Some("Commitment"),
        "Conversations" => Some("Conversation"),
        "Pipeline" => Some("Pipeline"),
        "Problems" => Some("Problem"),
        "ProofPoints" => Some("ProofPoint"),
        "Self" => Some("Self"),
        "Wins" => Some("Win"),
        _ => None,
    }
}

/// Parsed YAML frontmatter from an Obsidian note
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Frontmatter {
    #[serde(rename = "type")]
    pub note_type: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub tags: Option<Vec<String>>,
    pub related: Option<Vec<String>>,
    pub org: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
    pub relationship: Option<String>,
    // Catch extra fields
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// A vault-derived entity (from Nodes/ filename + frontmatter)
#[derive(Debug, Clone)]
pub struct VaultEntity {
    pub canonical_label: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub confidence: f64,
    pub tags: Vec<String>,
    pub source_file: String,
}

/// A relationship extracted from frontmatter or wiki-links
#[derive(Debug, Clone)]
pub struct VaultRelationship {
    pub subject_label: String,
    pub predicate: String,
    pub object_label: String,
    pub confidence: f64,
    pub source: String, // "frontmatter", "wikilink", "ner"
}

/// Parse YAML frontmatter from markdown content.
/// Returns None if no frontmatter block is found.
pub fn parse_frontmatter(content: &str) -> Option<Frontmatter> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    // Find closing ---
    let after_open = &trimmed[3..];
    let close_pos = after_open.find("\n---")?;
    let yaml_block = &after_open[..close_pos];
    serde_yaml::from_str(yaml_block).ok()
}

/// Extract all wiki-links from markdown content.
/// Returns Vec of (page_name, display_text_or_none).
/// Handles both [[Page Name]] and [[Page Name|Display Text]].
pub fn extract_wiki_links(content: &str) -> Vec<(String, Option<String>)> {
    let re = Regex::new(r"\[\[([^\]\|]+?)(?:\|([^\]]+?))?\]\]").unwrap();
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for cap in re.captures_iter(content) {
        let page_name = cap.get(1).unwrap().as_str().trim().to_string();
        let display = cap.get(2).map(|m| m.as_str().trim().to_string());
        let key = page_name.to_lowercase();
        if !seen.contains(&key) {
            seen.insert(key);
            links.push((page_name, display));
        }
    }
    links
}

/// Determine if a file is inside the Nodes/ directory and extract
/// the entity info from its path.
/// Returns (entity_label, entity_type) if the file is a Node.
pub fn entity_from_node_path(file_path: &Path, vault_root: &Path) -> Option<(String, String)> {
    let relative = file_path.strip_prefix(vault_root).ok()?;
    let components: Vec<&str> = relative
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Expect: Nodes / <Category> / <Filename>.md
    if components.len() < 3 || components[0] != "Nodes" {
        return None;
    }

    let folder = components[1];
    let entity_type = folder_to_entity_type(folder)?;

    // Strip .md extension for label
    let filename = components.last()?;
    let label = filename.strip_suffix(".md").unwrap_or(filename).to_string();
    if label.is_empty() {
        return None;
    }

    Some((label, entity_type.to_string()))
}

/// Extract frontmatter-based relationships for a given entity.
/// Uses `related`, `org`, and wiki-links in frontmatter fields.
pub fn relationships_from_frontmatter(
    entity_label: &str,
    fm: &Frontmatter,
) -> Vec<VaultRelationship> {
    let mut rels = Vec::new();

    // related: ["[[X]]", "[[Y]]"] → relatedTo
    if let Some(ref related_list) = fm.related {
        for item in related_list {
            if let Some(target) = strip_wikilink(item) {
                rels.push(VaultRelationship {
                    subject_label: entity_label.to_string(),
                    predicate: "relatedTo".to_string(),
                    object_label: target,
                    confidence: 0.95,
                    source: "frontmatter".to_string(),
                });
            }
        }
    }

    // org: "[[X]]" → worksFor (for people) / partOf (for others)
    if let Some(ref org_val) = fm.org {
        if let Some(target) = strip_wikilink(org_val) {
            let predicate = if fm.note_type.as_deref() == Some("person") {
                "worksFor"
            } else {
                "partOf"
            };
            rels.push(VaultRelationship {
                subject_label: entity_label.to_string(),
                predicate: predicate.to_string(),
                object_label: target,
                confidence: 0.95,
                source: "frontmatter".to_string(),
            });
        }
    }

    rels
}

/// Extract "mentions" relationships from wiki-links in the body text
/// (excluding frontmatter).
pub fn relationships_from_wikilinks(entity_label: &str, content: &str) -> Vec<VaultRelationship> {
    let body = strip_frontmatter(content);
    let links = extract_wiki_links(&body);
    let mut rels = Vec::new();

    for (page_name, _display) in links {
        // Don't create self-references
        if page_name.to_lowercase() == entity_label.to_lowercase() {
            continue;
        }
        rels.push(VaultRelationship {
            subject_label: entity_label.to_string(),
            predicate: "mentions".to_string(),
            object_label: page_name,
            confidence: 0.85,
            source: "wikilink".to_string(),
        });
    }

    rels
}

/// Strip frontmatter from content, returning only body text
fn strip_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    let after_open = &trimmed[3..];
    if let Some(close_pos) = after_open.find("\n---") {
        let body_start = close_pos + 4; // skip "\n---"
        if body_start < after_open.len() {
            return after_open[body_start..].to_string();
        }
    }
    content.to_string()
}

/// Strip wiki-link brackets from a string: "[[Foo]]" → "Foo", "[[Foo|Bar]]" → "Foo"
/// Also handles YAML-quoted strings like `"[[Foo]]"`
fn strip_wikilink(s: &str) -> Option<String> {
    // Strip outer quotes and whitespace first
    let trimmed = s.trim().trim_matches('"').trim();
    if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
        let inner = &trimmed[2..trimmed.len() - 2];
        let page = if let Some(pipe_pos) = inner.find('|') {
            &inner[..pipe_pos]
        } else {
            inner
        };
        let page = page.trim();
        if page.is_empty() {
            None
        } else {
            Some(page.to_string())
        }
    } else if trimmed.is_empty() {
        None
    } else {
        // Plain text (not wrapped in [[ ]]) — use as-is
        Some(trimmed.to_string())
    }
}

/// Directories to exclude from scanning
const EXCLUDED_DIRS: &[&str] = &[
    ".obsidian",
    "Scripts",
    "Templates",
    "vector_db",
    ".git",
    ".trash",
];

/// Check if a directory should be excluded from scanning
pub fn should_exclude_dir(dir_name: &str) -> bool {
    EXCLUDED_DIRS.contains(&dir_name) || dir_name.starts_with('.')
}

/// Strict fallback NER for non-Nodes files.
/// Only extracts high-confidence entities from prose paragraphs.
pub struct FallbackNER {
    person_re: Regex,
    // Blocklist of common English words that are NOT entities
    blocklist: HashSet<String>,
}

impl FallbackNER {
    pub fn new() -> Result<Self> {
        let person_re =
            Regex::new(r"\b(?:Mr\.|Mrs\.|Dr\.|Prof\.)\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+)+)\b")?;

        let blocklist_words = vec![
            // Common English words that get falsely extracted
            "About",
            "Above",
            "According",
            "Action",
            "Active",
            "After",
            "Again",
            "Against",
            "All",
            "Also",
            "Always",
            "Amount",
            "And",
            "Another",
            "Any",
            "Application",
            "Approach",
            "Are",
            "Around",
            "Available",
            "Back",
            "Based",
            "Basic",
            "Because",
            "Been",
            "Before",
            "Being",
            "Below",
            "Best",
            "Better",
            "Between",
            "Both",
            "Build",
            "Building",
            "Built",
            "But",
            "Can",
            "Case",
            "Change",
            "Clear",
            "Close",
            "Code",
            "Come",
            "Common",
            "Complete",
            "Complex",
            "Connection",
            "Consider",
            "Context",
            "Control",
            "Core",
            "Could",
            "Create",
            "Current",
            "Custom",
            "Data",
            "Day",
            "Days",
            "Decision",
            "Default",
            "Define",
            "Deploy",
            "Design",
            "Development",
            "Different",
            "Direct",
            "Does",
            "Done",
            "Down",
            "During",
            "Each",
            "Early",
            "Easy",
            "Effect",
            "Either",
            "Enable",
            "End",
            "Ensure",
            "Error",
            "Even",
            "Event",
            "Every",
            "Example",
            "Execute",
            "Existing",
            "Expect",
            "Experience",
            "External",
            "Fact",
            "False",
            "Feature",
            "Field",
            "File",
            "Final",
            "Find",
            "First",
            "Focus",
            "Follow",
            "For",
            "Force",
            "Form",
            "From",
            "Full",
            "Function",
            "Future",
            "General",
            "Get",
            "Give",
            "Global",
            "Going",
            "Good",
            "Great",
            "Group",
            "Growth",
            "Had",
            "Handle",
            "Has",
            "Have",
            "Help",
            "Here",
            "High",
            "Hold",
            "How",
            "However",
            "Idea",
            "Impact",
            "Implement",
            "Important",
            "Include",
            "Info",
            "Initial",
            "Input",
            "Inside",
            "Instead",
            "Internal",
            "Into",
            "Issue",
            "Item",
            "Its",
            "Just",
            "Keep",
            "Key",
            "Kind",
            "Know",
            "Known",
            "Large",
            "Last",
            "Late",
            "Launch",
            "Lead",
            "Left",
            "Less",
            "Let",
            "Level",
            "Like",
            "Line",
            "Link",
            "List",
            "Live",
            "Local",
            "Long",
            "Look",
            "Low",
            "Made",
            "Main",
            "Major",
            "Make",
            "Many",
            "Match",
            "Matter",
            "May",
            "Mean",
            "Method",
            "Might",
            "Model",
            "More",
            "Most",
            "Move",
            "Much",
            "Must",
            "Name",
            "Need",
            "New",
            "Next",
            "None",
            "Normal",
            "Not",
            "Note",
            "Nothing",
            "Now",
            "Number",
            "Off",
            "Old",
            "Once",
            "One",
            "Only",
            "Open",
            "Option",
            "Order",
            "Other",
            "Out",
            "Output",
            "Over",
            "Overview",
            "Own",
            "Part",
            "Path",
            "Pattern",
            "Per",
            "Place",
            "Plan",
            "Point",
            "Post",
            "Potential",
            "Power",
            "Present",
            "Previous",
            "Primary",
            "Problem",
            "Process",
            "Product",
            "Project",
            "Proper",
            "Provide",
            "Public",
            "Pull",
            "Purpose",
            "Push",
            "Put",
            "Quality",
            "Query",
            "Question",
            "Quick",
            "Quite",
            "Range",
            "Rate",
            "Read",
            "Ready",
            "Real",
            "Really",
            "Reason",
            "Recent",
            "Record",
            "Related",
            "Release",
            "Remove",
            "Report",
            "Request",
            "Required",
            "Response",
            "Rest",
            "Result",
            "Return",
            "Review",
            "Right",
            "Role",
            "Run",
            "Running",
            "Safe",
            "Same",
            "Save",
            "Scale",
            "Scope",
            "Second",
            "Section",
            "See",
            "Seems",
            "Send",
            "Serialization",
            "Server",
            "Service",
            "Session",
            "Set",
            "Setup",
            "Should",
            "Show",
            "Side",
            "Simple",
            "Since",
            "Single",
            "Size",
            "Small",
            "Some",
            "Something",
            "Source",
            "Space",
            "Specific",
            "Stage",
            "Standard",
            "Start",
            "State",
            "Status",
            "Step",
            "Still",
            "Stop",
            "Store",
            "Strategy",
            "Strong",
            "Structure",
            "Style",
            "Subject",
            "Success",
            "Such",
            "Support",
            "Sure",
            "System",
            "Table",
            "Take",
            "Target",
            "Task",
            "Team",
            "Tech",
            "Tell",
            "Term",
            "Test",
            "Than",
            "That",
            "The",
            "Their",
            "Them",
            "Then",
            "There",
            "These",
            "They",
            "Thing",
            "Think",
            "This",
            "Those",
            "Though",
            "Through",
            "Time",
            "Today",
            "Together",
            "Too",
            "Tool",
            "Top",
            "Total",
            "Track",
            "True",
            "Try",
            "Turn",
            "Type",
            "Under",
            "Until",
            "Update",
            "Upon",
            "Use",
            "Used",
            "User",
            "Using",
            "Value",
            "Version",
            "Very",
            "View",
            "Want",
            "Was",
            "Way",
            "Well",
            "Were",
            "What",
            "When",
            "Where",
            "Whether",
            "Which",
            "While",
            "Who",
            "Whole",
            "Why",
            "Will",
            "With",
            "Within",
            "Without",
            "Work",
            "Working",
            "Would",
            "Write",
            "Year",
            "Yet",
            "You",
            "Your",
            // Programming/tech terms
            "Abstract",
            "Algorithm",
            "Array",
            "Binary",
            "Boolean",
            "Buffer",
            "Cache",
            "Class",
            "Client",
            "Command",
            "Component",
            "Config",
            "Configuration",
            "Const",
            "Constructor",
            "Container",
            "Crypto",
            "Database",
            "Debug",
            "Enum",
            "Export",
            "Filter",
            "Format",
            "Framework",
            "Handler",
            "Hash",
            "Header",
            "Heap",
            "Import",
            "Index",
            "Instance",
            "Interface",
            "Iterator",
            "Layer",
            "Library",
            "Macro",
            "Manager",
            "Memory",
            "Message",
            "Middleware",
            "Module",
            "Mutex",
            "Namespace",
            "Network",
            "Object",
            "Package",
            "Parser",
            "Plugin",
            "Pointer",
            "Protocol",
            "Proxy",
            "Queue",
            "Reference",
            "Registry",
            "Render",
            "Repository",
            "Resolution",
            "Resource",
            "Router",
            "Runtime",
            "Schema",
            "Script",
            "Selector",
            "Sequence",
            "Socket",
            "Stack",
            "Stream",
            "String",
            "Struct",
            "Syntax",
            "Template",
            "Thread",
            "Timeout",
            "Token",
            "Trait",
            "Transaction",
            "Transform",
            "Trigger",
            "Tuple",
            "Validator",
            "Variable",
            "Vector",
            "Vendor",
            "Virtual",
            "Wrapper",
        ];

        let blocklist: HashSet<String> = blocklist_words.iter().map(|w| w.to_lowercase()).collect();

        Ok(Self {
            person_re,
            blocklist,
        })
    }

    /// Check if text is in the blocklist (case-insensitive)
    pub fn is_blocked(&self, text: &str) -> bool {
        self.blocklist.contains(&text.to_lowercase())
    }

    /// Extract entities from non-Nodes content using strict rules.
    /// Only extracts from prose paragraphs (not code blocks, headers, or frontmatter).
    pub fn extract_from_prose(
        &self,
        content: &str,
        known_entities: &HashSet<String>, // lowercase labels of known entities
    ) -> Vec<(String, String, f64)> {
        // (label, type, confidence)
        let body = strip_frontmatter(content);
        let prose = extract_prose_only(&body);
        let mut results = Vec::new();
        let mut seen = HashSet::new();

        // Only extract titled persons (Dr., Mr., etc.)
        for cap in self.person_re.captures_iter(&prose) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().trim().to_string();
                let key = name.to_lowercase();
                if !seen.contains(&key)
                    && !self.is_blocked(&name)
                    && !known_entities.contains(&key)
                    && name.split_whitespace().count() >= 2
                {
                    seen.insert(key);
                    results.push((name, "Person".to_string(), 0.7));
                }
            }
        }

        results
    }
}

/// Extract only prose paragraphs from markdown, stripping:
/// - Code blocks (``` ... ```)
/// - Headers (# ...)
/// - YAML frontmatter
/// - Bullet points that are just links
fn extract_prose_only(content: &str) -> String {
    let mut prose = String::new();
    let mut in_code_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        // Skip headers
        if trimmed.starts_with('#') {
            continue;
        }
        // Skip pure link lines
        if trimmed.starts_with("- [[") || trimmed.starts_with("* [[") {
            continue;
        }
        // Skip empty lines but keep prose
        if !trimmed.is_empty() {
            prose.push_str(line);
            prose.push('\n');
        }
    }

    prose
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
type: person
created: 2025-12-26
tags:
  - leedana
  - cofounder
related:
  - "[[Leedana]]"
  - "[[Nodalync]]"
org: "[[Leedana]]"
role: CEO & Co-founder
status: active
---

# Hassan El Rakhawy
Some body text."#;

        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.note_type.as_deref(), Some("person"));
        assert_eq!(fm.status.as_deref(), Some("active"));
        assert_eq!(fm.role.as_deref(), Some("CEO & Co-founder"));
        assert_eq!(fm.org.as_deref(), Some("[[Leedana]]"));

        let related = fm.related.unwrap();
        assert_eq!(related.len(), 2);
        assert!(
            related.contains(&"\"[[Leedana]]\"".to_string())
                || related.contains(&"[[Leedana]]".to_string())
        );
    }

    #[test]
    fn test_extract_wiki_links() {
        let content = r#"
Working with [[Hassan El Rakhawy]] on [[Leedana]] platform.
See [[Nodalync|the protocol]] for details.
Also check [[Nodalync]] again.
"#;
        let links = extract_wiki_links(content);
        assert_eq!(links.len(), 3); // Nodalync deduped
        assert_eq!(links[0].0, "Hassan El Rakhawy");
        assert_eq!(links[1].0, "Leedana");
        assert_eq!(links[2].0, "Nodalync");
        assert_eq!(links[2].1.as_deref(), Some("the protocol"));
    }

    #[test]
    fn test_strip_wikilink() {
        assert_eq!(
            strip_wikilink("\"[[Leedana]]\""),
            Some("Leedana".to_string())
        );
        assert_eq!(strip_wikilink("[[Foo|Bar]]"), Some("Foo".to_string()));
        assert_eq!(strip_wikilink("plain text"), Some("plain text".to_string()));
    }

    #[test]
    fn test_entity_from_node_path() {
        let vault = Path::new("C:\\vault");
        let file = Path::new("C:\\vault\\Nodes\\People\\Hassan El Rakhawy.md");
        let result = entity_from_node_path(file, vault);
        assert!(result.is_some());
        let (label, etype) = result.unwrap();
        assert_eq!(label, "Hassan El Rakhawy");
        assert_eq!(etype, "Person");
    }

    #[test]
    fn test_folder_mapping() {
        assert_eq!(folder_to_entity_type("People"), Some("Person"));
        assert_eq!(folder_to_entity_type("Organizations"), Some("Organization"));
        assert_eq!(folder_to_entity_type("Ideas"), Some("Concept"));
        assert_eq!(folder_to_entity_type("Insights"), Some("Concept"));
        assert_eq!(folder_to_entity_type("Unknown"), None);
    }

    #[test]
    fn test_relationships_from_frontmatter() {
        let fm = Frontmatter {
            note_type: Some("person".to_string()),
            related: Some(vec!["[[Leedana]]".to_string(), "[[Nodalync]]".to_string()]),
            org: Some("[[Leedana]]".to_string()),
            role: Some("CEO".to_string()),
            ..Default::default()
        };

        let rels = relationships_from_frontmatter("Hassan El Rakhawy", &fm);
        assert_eq!(rels.len(), 3); // 2 relatedTo + 1 worksFor
        assert!(rels
            .iter()
            .any(|r| r.predicate == "worksFor" && r.object_label == "Leedana"));
    }

    #[test]
    fn test_should_exclude_dir() {
        assert!(should_exclude_dir(".obsidian"));
        assert!(should_exclude_dir("Scripts"));
        assert!(should_exclude_dir(".git"));
        assert!(!should_exclude_dir("Nodes"));
        assert!(!should_exclude_dir("Dashboards"));
    }
}
