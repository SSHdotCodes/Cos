use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCapability {
    pub id: String,
    pub description: String,
    pub risk: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CosPluginManifest {
    pub schema_version: i64,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_in: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstalledPlugin {
    pub manifest: CosPluginManifest,
    pub location: PathBuf,
    pub is_trusted: bool,
    pub is_enabled: bool,
}

impl InstalledPlugin {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CosMarketplaceListing {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured: Option<bool>,
    #[serde(rename = "builtIn", default, skip_serializing_if = "Option::is_none")]
    pub built_in: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<CosPluginManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CosMarketplaceResponse {
    pub items: Vec<CosMarketplaceListing>,
    pub total: i64,
}

pub struct PluginRegistry;

impl PluginRegistry {
    /// Discover plugins in the built-in bundle, Application Support, and the
    /// workspace, keeping the last manifest per id and sorting built-ins first.
    pub fn discover(built_in_root: Option<&Path>, workspace: Option<&Path>) -> Vec<InstalledPlugin> {
        let mut roots: Vec<(PathBuf, bool)> = Vec::new();
        if let Some(built_in_root) = built_in_root {
            roots.push((built_in_root.to_path_buf(), true));
        }
        let app_support = crate::application_support_dir().join("Cos/Plugins");
        let app_support_marker = app_support.clone();
        roots.push((app_support, false));
        if let Some(workspace) = workspace {
            roots.push((workspace.join(".cos/plugins"), false));
        }

        let mut found: std::collections::HashMap<String, InstalledPlugin> = std::collections::HashMap::new();
        for (root, trusted) in &roots {
            let manifests = find_manifests(root);
            for url in manifests {
                let Ok(data) = std::fs::read(&url) else { continue };
                let Ok(manifest) = serde_json::from_slice::<CosPluginManifest>(&data) else {
                    continue;
                };
                found.insert(
                    manifest.id.clone(),
                    InstalledPlugin {
                        location: url.parent().unwrap_or(root).to_path_buf(),
                        is_trusted: *trusted || root == &app_support_marker,
                        is_enabled: true,
                        manifest,
                    },
                );
            }
        }
        let mut plugins: Vec<InstalledPlugin> = found.into_values().collect();
        plugins.sort_by(|lhs, rhs| {
            let lhs_built_in = lhs.manifest.built_in == Some(true);
            let rhs_built_in = rhs.manifest.built_in == Some(true);
            rhs_built_in
                .cmp(&lhs_built_in)
                .then_with(|| lhs.manifest.name.to_lowercase().cmp(&rhs.manifest.name.to_lowercase()))
        });
        plugins
    }
}

fn find_manifests(root: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() && name == "cos.plugin.json" {
                results.push(path);
            }
        }
    }
    results
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsMutation {
    FastMode(bool),
    FullAccess(bool),
    AutoCompact(bool),
    ShowTokenUsage(bool),
    Effort(crate::models::ReasoningEffort),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CosManagementAction {
    CreateSkill {
        id: String,
        name: String,
        description: String,
        instructions: String,
        plugin_id: Option<String>,
    },
    RemoveSkill {
        id: String,
        plugin_id: Option<String>,
    },
    CreatePlugin {
        id: String,
        name: String,
        description: String,
        instructions: Option<String>,
    },
    RemovePlugin {
        id: String,
    },
    SetPluginEnabled {
        id: String,
        enabled: bool,
    },
}

pub struct SettingsPluginExtraction {
    pub visible_text: String,
    pub mutation: Option<SettingsMutation>,
    pub management_action: Option<CosManagementAction>,
}

pub struct CosSettingsPlugin;

impl CosSettingsPlugin {
    pub const SYSTEM_PROMPT: &'static str = r#"Cos includes a trusted settings tool. When the user explicitly asks to change a Cos setting, include exactly one marker after your brief confirmation:
<cos-settings>{"key":"fastMode|fullAccess|autoCompact|showTokenUsage|effort","value":true}</cos-settings>
For effort, value must be one of minimal, low, medium, high, extraHigh, max. Never emit this marker without an explicit user request.

Cos also includes a guarded self-management tool for Cos-owned skills and plugins. When the user explicitly asks, include exactly one of these markers after your brief confirmation:
<cos-manage>{"action":"createSkill","id":"slug","name":"Name","description":"Purpose","instructions":"Complete skill instructions","pluginID":"optional.plugin.id"}</cos-manage>
<cos-manage>{"action":"removeSkill","id":"slug","pluginID":"optional.plugin.id"}</cos-manage>
<cos-manage>{"action":"createPlugin","id":"plugin.id","name":"Name","description":"Purpose","instructions":"Optional main skill instructions"}</cos-manage>
<cos-manage>{"action":"removePlugin|enablePlugin|disablePlugin","id":"plugin.id"}</cos-manage>
These actions are restricted to Cos-managed directories. Never emit them without an explicit user request, never target the built-in Cos plugin, and use lowercase ASCII slugs containing only letters, numbers, dots, underscores, or hyphens."#;

    pub fn extract(text: &str) -> SettingsPluginExtraction {
        let (visible, settings_payload) = remove_marker("cos-settings", text);
        let (visible, management_payload) = remove_marker("cos-manage", &visible);
        SettingsPluginExtraction {
            visible_text: visible.trim().to_string(),
            mutation: settings_payload.as_deref().and_then(parse_settings),
            management_action: management_payload.as_deref().and_then(parse_management),
        }
    }
}

fn remove_marker(name: &str, text: &str) -> (String, Option<String>) {
    let opening = format!("<{name}>");
    let closing = format!("</{name}>");
    let Some(start) = text.find(&opening) else {
        return (text.to_string(), None);
    };
    let payload_start = start + opening.len();
    let Some(relative_end) = text[payload_start..].find(&closing) else {
        return (text.to_string(), None);
    };
    let payload = text[payload_start..payload_start + relative_end].to_string();
    let visible = format!("{}{}", &text[..start], &text[payload_start + relative_end + closing.len()..]);
    (visible, Some(payload))
}

fn object_from(json: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    if json.len() > 70_000 {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Object(object)) => Some(object),
        _ => None,
    }
}

fn parse_settings(json: &str) -> Option<SettingsMutation> {
    let object = object_from(json)?;
    let key = object.get("key")?.as_str()?;
    match key {
        "fastMode" => object.get("value")?.as_bool().map(SettingsMutation::FastMode),
        "fullAccess" => object.get("value")?.as_bool().map(SettingsMutation::FullAccess),
        "autoCompact" => object.get("value")?.as_bool().map(SettingsMutation::AutoCompact),
        "showTokenUsage" => object.get("value")?.as_bool().map(SettingsMutation::ShowTokenUsage),
        "effort" => crate::models::ReasoningEffort::from_raw(object.get("value")?.as_str()?)
            .map(SettingsMutation::Effort),
        _ => None,
    }
}

fn parse_management(json: &str) -> Option<CosManagementAction> {
    let object = object_from(json)?;
    let action = object.get("action")?.as_str()?;
    let id = object.get("id")?.as_str()?.to_string();
    let plugin_id = object.get("pluginID").and_then(|value| value.as_str()).map(str::to_string);
    match action {
        "createSkill" => Some(CosManagementAction::CreateSkill {
            id,
            name: object.get("name")?.as_str()?.to_string(),
            description: object.get("description")?.as_str()?.to_string(),
            instructions: object.get("instructions")?.as_str()?.to_string(),
            plugin_id,
        }),
        "removeSkill" => Some(CosManagementAction::RemoveSkill { id, plugin_id }),
        "createPlugin" => Some(CosManagementAction::CreatePlugin {
            id,
            name: object.get("name")?.as_str()?.to_string(),
            description: object.get("description")?.as_str()?.to_string(),
            instructions: object.get("instructions").and_then(|value| value.as_str()).map(str::to_string),
        }),
        "removePlugin" => Some(CosManagementAction::RemovePlugin { id }),
        "enablePlugin" => Some(CosManagementAction::SetPluginEnabled { id, enabled: true }),
        "disablePlugin" => Some(CosManagementAction::SetPluginEnabled { id, enabled: false }),
        _ => None,
    }
}
