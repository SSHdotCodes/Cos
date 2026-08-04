use crate::plugins::InstalledPlugin;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComposerReferenceKind {
    Command,
    Skill,
    Plugin,
}

impl ComposerReferenceKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Skill => "Skill",
            Self::Plugin => "Plugin",
        }
    }

    fn rank(self) -> usize {
        match self {
            Self::Command => 0,
            Self::Skill => 1,
            Self::Plugin => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComposerReferenceQuery {
    pub trigger: char,
    pub term: String,
    pub range_location: usize,
    pub range_length: usize,
}

impl ComposerReferenceQuery {
    pub fn signature(&self) -> String {
        format!("{}:{}:{}:{}", self.trigger, self.range_location, self.range_length, self.term.to_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComposerReferenceSuggestion {
    pub id: String,
    pub kind: ComposerReferenceKind,
    pub title: String,
    pub detail: String,
    pub insertion: String,
}

pub struct ComposerReferenceResolver;

fn commands() -> Vec<ComposerReferenceSuggestion> {
    vec![
        ComposerReferenceSuggestion {
            id: "command:subagent".into(),
            kind: ComposerReferenceKind::Command,
            title: "/subagent".into(),
            detail: "Delegate a bounded task to another model and effort".into(),
            insertion: "/subagent ".into(),
        },
        ComposerReferenceSuggestion {
            id: "command:goal".into(),
            kind: ComposerReferenceKind::Command,
            title: "/goal".into(),
            detail: "Set a goal or show the active goal".into(),
            insertion: "/goal ".into(),
        },
        ComposerReferenceSuggestion {
            id: "command:goal-budget".into(),
            kind: ComposerReferenceKind::Command,
            title: "/goal --budget".into(),
            detail: "Set a goal with a token budget".into(),
            insertion: "/goal --budget ".into(),
        },
        ComposerReferenceSuggestion {
            id: "command:goal-status".into(),
            kind: ComposerReferenceKind::Command,
            title: "/goal status".into(),
            detail: "Show goal progress and token usage".into(),
            insertion: "/goal status".into(),
        },
        ComposerReferenceSuggestion {
            id: "command:goal-complete".into(),
            kind: ComposerReferenceKind::Command,
            title: "/goal complete".into(),
            detail: "Mark the active goal complete".into(),
            insertion: "/goal complete".into(),
        },
        ComposerReferenceSuggestion {
            id: "command:goal-clear".into(),
            kind: ComposerReferenceKind::Command,
            title: "/goal clear".into(),
            detail: "Remove the active goal".into(),
            insertion: "/goal clear".into(),
        },
    ]
}

/// The resolver works in UTF-16 code units to match the NSString-backed Swift
/// implementation and the editor's selection offsets.
fn utf16_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn utf16_slice(units: &[u16], start: usize, length: usize) -> String {
    String::from_utf16_lossy(&units[start..(start + length).min(units.len())])
}

impl ComposerReferenceResolver {
    pub fn query(text: &str, selection_utf16_offset: usize) -> Option<ComposerReferenceQuery> {
        let source = utf16_units(text);
        let cursor = selection_utf16_offset.min(source.len());
        let mut start = cursor;

        while start > 0 {
            let unit = source[start - 1];
            // Whitespace check on the UTF-16 unit (covers ASCII whitespace and
            // common Unicode separators in the BMP via char round-trip).
            let is_space = char::from_u32(unit as u32).map(|c| c.is_whitespace()).unwrap_or(false)
                || (0xD800..0xE000).contains(&unit);
            if is_space {
                break;
            }
            start -= 1;
        }

        if start >= cursor {
            return None;
        }
        let token = utf16_slice(&source, start, cursor - start);
        let trigger = token.chars().next()?;
        if trigger != '/' && trigger != '@' {
            return None;
        }
        let term: String = token.chars().skip(1).collect();
        if term
            .chars()
            .any(|c| !(c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == ' '))
        {
            return None;
        }
        Some(ComposerReferenceQuery {
            trigger,
            term,
            range_location: start,
            range_length: cursor - start,
        })
    }

    pub fn suggestions(
        query: &ComposerReferenceQuery,
        plugins: &[InstalledPlugin],
        limit: usize,
    ) -> Vec<ComposerReferenceSuggestion> {
        let enabled: Vec<&InstalledPlugin> = plugins.iter().filter(|plugin| plugin.is_enabled).collect();
        let candidates: Vec<ComposerReferenceSuggestion> = if query.trigger == '/' {
            let skills = enabled.iter().flat_map(|plugin| {
                plugin.manifest.skills.iter().map(|skill| ComposerReferenceSuggestion {
                    id: format!("skill:{}:{}", plugin.id(), skill),
                    kind: ComposerReferenceKind::Skill,
                    title: format!("/{skill}"),
                    detail: format!("{} · {}", plugin.manifest.name, plugin.manifest.description),
                    insertion: format!("/{skill} "),
                })
            });
            commands().into_iter().chain(skills).collect()
        } else {
            enabled
                .iter()
                .map(|plugin| {
                    let handle = plugin_handle(&plugin.manifest.name);
                    ComposerReferenceSuggestion {
                        id: format!("plugin:{}", plugin.id()),
                        kind: ComposerReferenceKind::Plugin,
                        title: format!("@{handle}"),
                        detail: plugin.manifest.description.clone(),
                        insertion: format!("@{handle} "),
                    }
                })
                .collect()
        };

        let normalized_term = query.term.to_lowercase();
        let mut filtered: Vec<ComposerReferenceSuggestion> = candidates
            .into_iter()
            .filter(|suggestion| {
                normalized_term.is_empty()
                    || format!("{} {}", suggestion.title, suggestion.detail)
                        .to_lowercase()
                        .contains(&normalized_term)
            })
            .collect();
        filtered.sort_by(|left, right| {
            let left_prefix = left
                .title
                .chars()
                .skip(1)
                .collect::<String>()
                .to_lowercase()
                .starts_with(&normalized_term);
            let right_prefix = right
                .title
                .chars()
                .skip(1)
                .collect::<String>()
                .to_lowercase()
                .starts_with(&normalized_term);
            right_prefix
                .cmp(&left_prefix)
                .then(left.kind.rank().cmp(&right.kind.rank()))
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
        });
        filtered.truncate(limit.max(0) as usize);
        filtered
    }

    pub fn replacing_query(
        text: &str,
        query: &ComposerReferenceQuery,
        insertion: &str,
    ) -> (String, usize) {
        let source = utf16_units(text);
        if query.range_location == usize::MAX || query.range_location + query.range_length > source.len() {
            return (text.to_string(), query.range_location.min(source.len()));
        }
        let mut updated: Vec<u16> = source[..query.range_location].to_vec();
        updated.extend(insertion.encode_utf16());
        updated.extend_from_slice(&source[query.range_location + query.range_length..]);
        let insertion_len = insertion.encode_utf16().count();
        (String::from_utf16_lossy(&updated), query.range_location + insertion_len)
    }

    pub fn reference_context(prompt: &str, plugins: &[InstalledPlugin]) -> String {
        let enabled: Vec<&InstalledPlugin> = plugins.iter().filter(|plugin| plugin.is_enabled).collect();
        let keep = |c: char| c.is_alphanumeric() || c == '/' || c == '@' || c == '.' || c == '_' || c == '-';
        let words: Vec<String> = prompt
            .split_whitespace()
            .map(|word| word.trim_matches(|c: char| !keep(c)).to_string())
            .collect();
        let mut lines: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for word in words.into_iter().filter(|word| word.chars().count() > 1) {
            if let Some(rest) = word.strip_prefix('/') {
                if rest.eq_ignore_ascii_case("goal") {
                    continue;
                }
                for plugin in &enabled {
                    let Some(skill) = plugin
                        .manifest
                        .skills
                        .iter()
                        .find(|skill| skill.eq_ignore_ascii_case(rest))
                    else {
                        continue;
                    };
                    let key = format!("skill:{}:{}", plugin.id(), skill);
                    if seen.insert(key) {
                        lines.push(format!(
                            "- Apply skill /{} from {} ({}).",
                            skill, plugin.manifest.name, plugin.id()
                        ));
                    }
                }
            } else if let Some(rest) = word.strip_prefix('@') {
                for plugin in &enabled {
                    if !plugin_matches(plugin, rest) {
                        continue;
                    }
                    let key = format!("plugin:{}", plugin.id());
                    if seen.insert(key) {
                        lines.push(format!(
                            "- Use plugin @{} ({}) and its declared capabilities.",
                            plugin_handle(&plugin.manifest.name),
                            plugin.id()
                        ));
                    }
                }
            }
        }

        if lines.is_empty() {
            return String::new();
        }
        format!("The user explicitly referenced these Cos extensions:\n{}", lines.join("\n"))
    }
}

pub fn plugin_handle(name: &str) -> String {
    let lowered = name.to_lowercase();
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in lowered.chars() {
        if c.is_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            pieces.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces.join("-")
}

fn plugin_matches(plugin: &InstalledPlugin, requested: &str) -> bool {
    let normalized = requested.to_lowercase();
    let id = plugin.id().to_lowercase();
    plugin_handle(&plugin.manifest.name) == normalized
        || id == normalized
        || id.ends_with(&format!(".{normalized}"))
}
