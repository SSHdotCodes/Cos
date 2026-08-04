use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::run_control::AgentRunControl;

/// ISO8601 date coding matching Swift's `.iso8601` strategy (no fractional
/// seconds on encode, tolerant on decode).
pub mod iso8601_date {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::format_description::well_known::{Iso8601, Rfc3339};
    use time::OffsetDateTime;

    const FORMAT: &[time::format_description::FormatItem<'_>] = time::macros::format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
    );

    pub fn serialize<S: Serializer>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error> {
        let rounded = value.replace_nanosecond(0).unwrap_or(*value);
        serializer.serialize_str(&rounded.format(FORMAT).map_err(serde::ser::Error::custom)?)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<OffsetDateTime, D::Error> {
        let raw = String::deserialize(deserializer)?;
        OffsetDateTime::parse(&raw, &Rfc3339)
            .or_else(|_| OffsetDateTime::parse(&raw, &Iso8601::DEFAULT))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningEffort {
    #[serde(rename = "minimal")]
    Minimal,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "extraHigh")]
    ExtraHigh,
    #[serde(rename = "max")]
    Max,
}

impl ReasoningEffort {
    pub const ALL: [ReasoningEffort; 6] = [
        ReasoningEffort::Minimal,
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::ExtraHigh,
        ReasoningEffort::Max,
    ];

    pub fn raw_value(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::ExtraHigh => "extraHigh",
            Self::Max => "max",
        }
    }

    pub fn from_raw(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|effort| effort.raw_value() == value)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::ExtraHigh => "Extra High",
            Self::Max => "Max",
        }
    }

    pub fn short_title(self) -> &'static str {
        self.title()
    }

    pub fn rank(self) -> usize {
        Self::ALL.iter().position(|candidate| *candidate == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderBridge {
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "pi")]
    Pi,
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "opencode")]
    OpenCode,
    #[serde(rename = "qwen")]
    Qwen,
    #[serde(rename = "openAICompatible")]
    OpenAICompatible,
}

impl ProviderBridge {
    pub fn title(self) -> &'static str {
        match self {
            Self::Codex => "ChatGPT",
            Self::Pi => "Pi",
            Self::Claude => "Claude",
            Self::OpenCode => "OpenCode",
            Self::Qwen => "Qwen",
            Self::OpenAICompatible => "OpenAI compatible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthenticationMode {
    #[serde(rename = "subscription")]
    Subscription,
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "local")]
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub bridge: ProviderBridge,
    pub auth_mode: AuthenticationMode,
    #[serde(rename = "baseURL", default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keychain_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfile {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    pub name: String,
    pub model: String,
    pub context_window: i64,
    pub supports_images: bool,
    pub supports_tools: bool,
    pub supported_efforts: Vec<ReasoningEffort>,
}

impl ModelProfile {
    pub fn new(
        id: &str,
        provider_id: &str,
        name: &str,
        model: &str,
        context_window: i64,
        supports_images: bool,
        supports_tools: bool,
        supported_efforts: Vec<ReasoningEffort>,
    ) -> Self {
        Self {
            id: id.into(),
            provider_id: provider_id.into(),
            name: name.into(),
            model: model.into(),
            context_window,
            supports_images,
            supports_tools,
            supported_efforts,
        }
    }

    pub fn effort_options(&self) -> &[ReasoningEffort] {
        if self.supported_efforts.is_empty() {
            static HIGH: [ReasoningEffort; 1] = [ReasoningEffort::High];
            &HIGH
        } else {
            &self.supported_efforts
        }
    }

    pub fn supports_fast_mode(&self) -> bool {
        self.provider_id == "chatgpt"
    }

    pub fn normalized_effort(&self, requested: ReasoningEffort) -> ReasoningEffort {
        let options = self.effort_options();
        if options.contains(&requested) {
            return requested;
        }
        options
            .iter()
            .copied()
            .min_by(|left, right| {
                let left_distance = left.rank().abs_diff(requested.rank());
                let right_distance = right.rank().abs_diff(requested.rank());
                left_distance
                    .cmp(&right_distance)
                    // On a tie the Swift version picks the higher rank.
                    .then(right.rank().cmp(&left.rank()))
            })
            .unwrap_or(ReasoningEffort::High)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubagentRoute {
    pub model: ModelProfile,
    pub provider: ProviderProfile,
}

impl SubagentRoute {
    pub fn id(&self) -> &str {
        &self.model.id
    }

    pub fn accepts(&self, effort: ReasoningEffort) -> bool {
        self.model.effort_options().contains(&effort)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CosSubagentRequest {
    pub task: String,
    pub model_id: String,
    pub effort: ReasoningEffort,
}

pub struct SubagentAuthorization;

impl SubagentAuthorization {
    pub fn is_explicitly_requested(prompt: &str) -> bool {
        let value = prompt.to_lowercase();
        if Self::is_explicitly_forbidden(&value) {
            return false;
        }
        value.contains("/subagent")
            || value.contains("subagent")
            || value.contains("sub-agent")
            || value.contains("delegate to another model")
            || value.contains("delegate this to")
    }

    pub fn is_explicitly_forbidden(prompt: &str) -> bool {
        let value = prompt.to_lowercase();
        ["do not use subagent", "don't use subagent", "no subagent", "without subagent"]
            .iter()
            .any(|needle| value.contains(needle))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

impl MessageRole {
    pub fn raw_value(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkTraceKind {
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "reasoning")]
    Reasoning,
    #[serde(rename = "tool")]
    Tool,
    #[serde(rename = "subagent")]
    Subagent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkTraceItem {
    pub id: Uuid,
    pub kind: WorkTraceKind,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(with = "iso8601_date")]
    pub created_at: OffsetDateTime,
}

impl WorkTraceItem {
    pub fn new(kind: WorkTraceKind, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            title: title.into(),
            detail: detail.into(),
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    #[serde(with = "iso8601_date")]
    pub created_at: OffsetDateTime,
    pub is_streaming: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_items: Option<Vec<WorkTraceItem>>,
}

impl ChatMessage {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: content.into(),
            created_at: OffsetDateTime::now_utc(),
            is_streaming: false,
            work_items: None,
        }
    }

    pub fn streaming_assistant(id: Uuid) -> Self {
        Self {
            id,
            role: MessageRole::Assistant,
            content: String::new(),
            created_at: OffsetDateTime::now_utc(),
            is_streaming: true,
            work_items: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoalStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "budgetLimited")]
    BudgetLimited,
    #[serde(rename = "complete")]
    Complete,
}

impl GoalStatus {
    pub fn raw_value(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::BudgetLimited => "budgetLimited",
            Self::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentGoal {
    pub objective: String,
    pub status: GoalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    pub used_tokens: i64,
    #[serde(with = "iso8601_date")]
    pub created_at: OffsetDateTime,
}

impl AgentGoal {
    pub fn new(objective: impl Into<String>, token_budget: Option<i64>) -> Self {
        Self {
            objective: objective.into(),
            status: GoalStatus::Active,
            token_budget,
            used_tokens: 0,
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CosThread {
    pub id: Uuid,
    pub title: String,
    pub workspace_path: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
    pub effort: ReasoningEffort,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<AgentGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted_context: Option<String>,
    #[serde(with = "iso8601_date")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601_date")]
    pub updated_at: OffsetDateTime,
}

impl CosThread {
    pub fn new(workspace_path: impl Into<String>, model_id: impl Into<String>) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id: Uuid::new_v4(),
            title: "New task".into(),
            workspace_path: workspace_path.into(),
            model_id: model_id.into(),
            effort: ReasoningEffort::High,
            messages: Vec::new(),
            goal: None,
            compacted_context: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone)]
pub struct AgentRequest {
    pub prompt: String,
    pub latest_user_request: String,
    pub thread: CosThread,
    pub model: ModelProfile,
    pub provider: ProviderProfile,
    pub effort: ReasoningEffort,
    pub fast_mode: bool,
    pub full_access: bool,
    pub workspace_is_trusted: bool,
    pub extension_instructions: String,
    pub tools_enabled: bool,
    pub computer_use_enabled: bool,
    pub browser_enabled: bool,
    pub available_subagent_routes: Vec<SubagentRoute>,
    pub subagents_authorized: bool,
    pub agent_depth: u32,
    pub run_control: Option<Arc<AgentRunControl>>,
}

impl AgentRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        prompt: impl Into<String>,
        latest_user_request: Option<String>,
        thread: CosThread,
        model: ModelProfile,
        provider: ProviderProfile,
        effort: ReasoningEffort,
        fast_mode: bool,
        full_access: bool,
    ) -> Self {
        let prompt = prompt.into();
        Self {
            latest_user_request: latest_user_request.unwrap_or_else(|| prompt.clone()),
            prompt,
            thread,
            model,
            provider,
            effort,
            fast_mode,
            full_access,
            workspace_is_trusted: false,
            extension_instructions: String::new(),
            tools_enabled: true,
            computer_use_enabled: false,
            browser_enabled: false,
            available_subagent_routes: Vec::new(),
            subagents_authorized: false,
            agent_depth: 0,
            run_control: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    Status(String),
    WorkDelta(String),
    TextDelta(String),
    Tool { name: String, detail: String },
    Subagent { name: String, detail: String },
    SteeringApplied(Vec<crate::run_control::SteeringMessage>),
    Usage { input: i64, output: i64 },
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    #[serde(default)]
    pub appearance: AppearanceMode,
    #[serde(default)]
    pub fast_mode: bool,
    #[serde(default = "default_true")]
    pub full_access: bool,
    #[serde(default = "default_true")]
    pub auto_compact: bool,
    #[serde(default = "default_compact_percent")]
    pub compact_at_percent: f64,
    #[serde(default = "default_keep_recent")]
    pub keep_recent_tokens: i64,
    #[serde(default)]
    pub show_token_usage: bool,
    #[serde(default = "default_true")]
    pub animate_streaming: bool,
    #[serde(default = "default_workspace")]
    pub default_workspace: String,
    #[serde(default = "default_selected_model")]
    pub selected_model_id: String,
    #[serde(default = "default_effort")]
    pub default_effort: ReasoningEffort,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_model_id: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_compact_percent() -> f64 {
    78.0
}
fn default_keep_recent() -> i64 {
    20_000
}
fn default_workspace() -> String {
    dirs_home().to_string_lossy().into_owned()
}
fn default_selected_model() -> String {
    DefaultCatalog::models()[0].id.clone()
}
fn default_effort() -> ReasoningEffort {
    ReasoningEffort::High
}

pub fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"))
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            appearance: AppearanceMode::System,
            fast_mode: false,
            full_access: true,
            auto_compact: true,
            compact_at_percent: 78.0,
            keep_recent_tokens: 20_000,
            show_token_usage: false,
            animate_streaming: true,
            default_workspace: default_workspace(),
            selected_model_id: default_selected_model(),
            default_effort: ReasoningEffort::High,
            title_model_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppearanceMode {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
    #[serde(rename = "trueDark")]
    TrueDark,
}

impl Default for AppearanceMode {
    fn default() -> Self {
        AppearanceMode::System
    }
}

impl AppearanceMode {
    pub const ALL: [AppearanceMode; 4] = [
        AppearanceMode::System,
        AppearanceMode::Light,
        AppearanceMode::Dark,
        AppearanceMode::TrueDark,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::TrueDark => "True Dark",
        }
    }
}

pub struct DefaultCatalog;

impl DefaultCatalog {
    pub fn providers() -> Vec<ProviderProfile> {
        let provider = |id: &str,
                        name: &str,
                        bridge: ProviderBridge,
                        auth_mode: AuthenticationMode,
                        base_url: Option<&str>,
                        keychain_account: Option<&str>,
                        executable: Option<&str>| ProviderProfile {
            id: id.into(),
            name: name.into(),
            bridge,
            auth_mode,
            base_url: base_url.and_then(|value| Url::parse(value).ok()),
            keychain_account: keychain_account.map(str::to_string),
            executable: executable.map(str::to_string),
            is_enabled: true,
        };
        vec![
            provider(
                "chatgpt",
                "ChatGPT Plus / Pro",
                ProviderBridge::Codex,
                AuthenticationMode::Subscription,
                Some("https://chatgpt.com/backend-api/codex"),
                None,
                Some("codex"),
            ),
            provider(
                "anthropic",
                "Claude Pro / Max",
                ProviderBridge::Claude,
                AuthenticationMode::Subscription,
                Some("https://api.anthropic.com/v1"),
                Some("anthropic-subscription"),
                Some("claude"),
            ),
            provider(
                "xai",
                "X Premium / SuperGrok",
                ProviderBridge::OpenCode,
                AuthenticationMode::Subscription,
                Some("https://api.x.ai/v1"),
                Some("xai-subscription"),
                Some("opencode"),
            ),
            provider(
                "opencode-go",
                "OpenCode Go",
                ProviderBridge::OpenCode,
                AuthenticationMode::ApiKey,
                Some("https://api.opencode.ai/v1"),
                Some("opencode-go"),
                Some("opencode"),
            ),
            provider(
                "qwen",
                "Qwen Token Plan",
                ProviderBridge::Qwen,
                AuthenticationMode::ApiKey,
                Some("https://coding-intl.dashscope.aliyuncs.com/v1"),
                Some("qwen-token-plan"),
                Some("qwen"),
            ),
            provider("pi", "Pi harness", ProviderBridge::Pi, AuthenticationMode::Local, None, None, Some("pi")),
            provider(
                "openai-api",
                "OpenAI API",
                ProviderBridge::OpenAICompatible,
                AuthenticationMode::ApiKey,
                Some("https://api.openai.com/v1"),
                Some("openai-api"),
                None,
            ),
        ]
    }

    pub fn models() -> Vec<ModelProfile> {
        use ReasoningEffort as E;
        vec![
            ModelProfile::new("chatgpt:gpt-5.6-sol", "chatgpt", "5.6 Sol", "gpt-5.6-sol", 400_000, true, true, E::ALL.to_vec()),
            ModelProfile::new("chatgpt:gpt-5.6-terra", "chatgpt", "5.6 Terra", "gpt-5.6-terra", 400_000, true, true, E::ALL.to_vec()),
            ModelProfile::new("chatgpt:gpt-5.6-luna", "chatgpt", "5.6 Luna", "gpt-5.6-luna", 200_000, false, false, E::ALL.to_vec()),
            ModelProfile::new(
                "anthropic:claude-opus-5",
                "anthropic",
                "Claude Opus 5",
                "claude-opus-5",
                200_000,
                true,
                true,
                vec![E::Low, E::Medium, E::High, E::ExtraHigh, E::Max],
            ),
            ModelProfile::new(
                "anthropic:claude-sonnet-5",
                "anthropic",
                "Claude Sonnet 5",
                "claude-sonnet-5",
                200_000,
                true,
                true,
                vec![E::Low, E::Medium, E::High, E::ExtraHigh, E::Max],
            ),
            ModelProfile::new(
                "anthropic:claude-fable-5",
                "anthropic",
                "Claude Fable 5",
                "claude-fable-5",
                200_000,
                true,
                true,
                vec![E::Low, E::Medium, E::High, E::ExtraHigh, E::Max],
            ),
            ModelProfile::new(
                "anthropic:claude-haiku-4.5",
                "anthropic",
                "Claude Haiku 4.5",
                "claude-haiku-4.5",
                200_000,
                false,
                false,
                vec![E::Low],
            ),
            ModelProfile::new("xai:grok-4.5", "xai", "Grok 4.5", "grok-4.5", 256_000, true, true, vec![E::Low, E::Medium, E::High]),
            ModelProfile::new("opencode-go:big-pickle", "opencode-go", "Big Pickle", "opencode/big-pickle", 200_000, true, true, E::ALL.to_vec()),
            ModelProfile::new("qwen:qwen3.8-max", "qwen", "Qwen 3.8 Max", "qwen3.8-max", 262_144, true, true, E::ALL.to_vec()),
            ModelProfile::new("pi:smart", "pi", "Pi Smart Route", "smart", 200_000, true, true, E::ALL.to_vec()),
            ModelProfile::new("openai-api:custom", "openai-api", "OpenAI API model", "gpt-5.6", 400_000, true, true, E::ALL.to_vec()),
        ]
    }
}
