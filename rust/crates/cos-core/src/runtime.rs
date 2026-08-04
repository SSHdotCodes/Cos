use crate::error::AgentRuntimeError;
use crate::harness::{AgentEventStream, CosHarness, SubagentRunner};
use crate::models::{
    AgentRequest, CosSubagentRequest, CosThread, DefaultCatalog, ModelProfile, ProviderBridge,
    ProviderProfile, ReasoningEffort, SubagentRoute,
};
use crate::secure_store::SecureStore;
use base64::Engine;
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AgentCredential {
    pub token: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
}

impl AgentCredential {
    pub fn new(token: impl Into<String>) -> Self {
        Self { token: token.into(), account_id: None, email: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSessionInfo {
    pub email: Option<String>,
    pub account_id: Option<String>,
}

impl ProviderSessionInfo {
    pub fn display_name(&self) -> String {
        self.email
            .clone()
            .or_else(|| self.account_id.clone())
            .unwrap_or_else(|| "Connected subscription".to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct AgentRuntime {
    secure_store: SecureStore,
}

impl AgentRuntime {
    pub fn stream(&self, request: AgentRequest) -> Result<AgentEventStream, AgentRuntimeError> {
        if !request.workspace_is_trusted {
            return Err(AgentRuntimeError::DirectoryTrustRequired(
                request.thread.workspace_path.clone(),
            ));
        }
        let (routed_request, credential) = self.routed_request_and_credential(&request)?;
        let runner: SubagentRunner = {
            let parent = routed_request.clone();
            let runtime = self.clone();
            Arc::new(move |subagent_request| runtime.subagent_stream(&parent, subagent_request))
        };
        Ok(CosHarness::stream(routed_request, credential, Some(runner)))
    }

    pub fn accessible_subagent_routes(
        &self,
        providers: &[ProviderProfile],
        models: &[ModelProfile],
    ) -> Vec<SubagentRoute> {
        let usable: std::collections::HashMap<String, ProviderProfile> = providers
            .iter()
            .filter(|provider| {
                provider.is_enabled
                    && provider.bridge != ProviderBridge::Pi
                    && self.credential_for(provider).ok().flatten().is_some()
            })
            .map(|provider| (provider.id.clone(), provider.clone()))
            .collect();
        models
            .iter()
            .filter(|model| model.supports_tools && usable.contains_key(&model.provider_id))
            .map(|model| SubagentRoute {
                model: model.clone(),
                provider: usable[&model.provider_id].clone(),
            })
            .collect()
    }

    pub fn session_info(&self, provider: &ProviderProfile) -> Option<ProviderSessionInfo> {
        if provider.auth_mode != crate::models::AuthenticationMode::Subscription {
            return None;
        }
        let credential = subscription_credential(provider)?;
        Some(ProviderSessionInfo { email: credential.email, account_id: credential.account_id })
    }

    fn subagent_stream(
        &self,
        parent: &AgentRequest,
        request: CosSubagentRequest,
    ) -> Result<AgentEventStream, AgentRuntimeError> {
        if !parent.subagents_authorized || parent.agent_depth != 0 {
            return Err(AgentRuntimeError::LaunchFailed(
                "subagents were not authorized by the user for this run".into(),
            ));
        }
        let Some(route) = parent
            .available_subagent_routes
            .iter()
            .find(|route| route.id() == request.model_id)
        else {
            return Err(AgentRuntimeError::LaunchFailed(format!(
                "{} is not in this run's accessible subagent model allowlist",
                request.model_id
            )));
        };
        if !route.accepts(request.effort) {
            let valid = route
                .model
                .effort_options()
                .iter()
                .map(|effort| effort.title())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(AgentRuntimeError::LaunchFailed(format!(
                "{} does not support {} reasoning. Available efforts: {valid}",
                route.model.name,
                request.effort.title()
            )));
        }

        let mut child_thread = CosThread::new(&parent.thread.workspace_path, &route.model.id);
        child_thread.effort = request.effort;
        child_thread
            .messages
            .push(crate::models::ChatMessage::new(crate::models::MessageRole::User, &request.task));
        let mut child_request = AgentRequest::new(
            format!(
                "You are a focused Cos subagent. Complete this bounded delegated task and return a concise, evidence-based result to the parent agent.\n\nDelegated task:\n{}",
                request.task
            ),
            Some(request.task.clone()),
            child_thread,
            route.model.clone(),
            route.provider.clone(),
            request.effort,
            parent.fast_mode && route.model.supports_fast_mode(),
            parent.full_access,
        );
        child_request.workspace_is_trusted = parent.workspace_is_trusted;
        child_request.extension_instructions = parent.extension_instructions.clone();
        child_request.tools_enabled = true;
        child_request.computer_use_enabled = false;
        child_request.browser_enabled = parent.browser_enabled;
        child_request.available_subagent_routes = Vec::new();
        child_request.subagents_authorized = false;
        child_request.agent_depth = parent.agent_depth + 1;

        let (routed_request, credential) = self.routed_request_and_credential(&child_request)?;
        Ok(CosHarness::stream(routed_request, credential, None))
    }

    fn routed_request_and_credential(
        &self,
        request: &AgentRequest,
    ) -> Result<(AgentRequest, AgentCredential), AgentRuntimeError> {
        let mut routed_request = request.clone();
        if request.provider.bridge == ProviderBridge::Pi {
            let chatgpt = DefaultCatalog::providers()
                .into_iter()
                .find(|provider| provider.id == "chatgpt")
                .ok_or_else(|| AgentRuntimeError::UnsupportedProvider(request.provider.name.clone()))?;
            let model = DefaultCatalog::models()
                .into_iter()
                .find(|model| model.provider_id == "chatgpt")
                .ok_or_else(|| AgentRuntimeError::UnsupportedProvider(request.provider.name.clone()))?;
            routed_request.provider = chatgpt.clone();
            routed_request.model = model;
            let credential = self
                .credential_for(&chatgpt)?
                .ok_or_else(|| AgentRuntimeError::MissingApiKey(chatgpt.name.clone()))?;
            return Ok((routed_request, credential));
        }
        let credential = self
            .credential_for(&request.provider)?
            .ok_or_else(|| AgentRuntimeError::MissingApiKey(request.provider.name.clone()))?;
        Ok((routed_request, credential))
    }

    fn credential_for(&self, provider: &ProviderProfile) -> Result<Option<AgentCredential>, AgentRuntimeError> {
        if let Some(account) = &provider.keychain_account {
            if let Some(token) = self
                .secure_store
                .get(account)
                .map_err(|error| AgentRuntimeError::LaunchFailed(error.to_string()))?
            {
                return Ok(Some(AgentCredential::new(token)));
            }
        }
        if provider.auth_mode == crate::models::AuthenticationMode::Subscription {
            return Ok(subscription_credential(provider));
        }
        Ok(None)
    }
}

pub fn subscription_credential(provider: &ProviderProfile) -> Option<AgentCredential> {
    match provider.id.as_str() {
        "chatgpt" => chatgpt_credential(),
        "xai" => opencode_credential("xai"),
        "opencode-go" => opencode_credential("opencode-go"),
        "anthropic" => {
            if let Ok(value) = std::env::var("ANTHROPIC_API_KEY") {
                if !value.is_empty() {
                    return Some(AgentCredential::new(value));
                }
            }
            credential_from_json(
                &crate::models::dirs_home().join(".claude/.credentials.json"),
                &["accessToken", "access_token", "token", "key"],
            )
        }
        _ => None,
    }
}

fn chatgpt_credential() -> Option<AgentCredential> {
    let url = crate::models::dirs_home().join(".codex/auth.json");
    let root = json_at(&url)?;
    let tokens = root.get("tokens")?.as_object()?;
    let access = tokens.get("access_token")?.as_str().filter(|value| !value.is_empty())?;
    let account = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| account_id_from_jwt(access));
    let id_token = tokens.get("id_token").and_then(Value::as_str);
    let email = find_string("email", &root)
        .or_else(|| id_token.and_then(|token| string_in_jwt("email", token)))
        .or_else(|| string_in_jwt("email", access));
    Some(AgentCredential {
        token: access.to_string(),
        account_id: account,
        email,
    })
}

fn opencode_credential(name: &str) -> Option<AgentCredential> {
    let url = crate::models::dirs_home().join(".local/share/opencode/auth.json");
    let root = json_at(&url)?;
    let entry = root.get(name)?.as_object()?;
    for key in ["access", "token", "key"] {
        if let Some(token) = entry.get(key).and_then(Value::as_str).filter(|value| !value.is_empty()) {
            let account = entry
                .get("accountId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| account_id_from_jwt(token));
            let email = entry
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| string_in_jwt("email", token));
            return Some(AgentCredential { token: token.to_string(), account_id: account, email });
        }
    }
    None
}

fn credential_from_json(url: &std::path::Path, preferred_keys: &[&str]) -> Option<AgentCredential> {
    let object = json_at(url)?;
    for key in preferred_keys {
        if let Some(token) = find_string(key, &object) {
            return Some(AgentCredential {
                account_id: find_string("account_id", &object).or_else(|| account_id_from_jwt(&token)),
                email: find_string("email", &object).or_else(|| string_in_jwt("email", &token)),
                token,
            });
        }
    }
    None
}

fn json_at(url: &std::path::Path) -> Option<Value> {
    let data = std::fs::read(url).ok()?;
    if data.len() > 2_000_000 {
        return None;
    }
    serde_json::from_slice(&data).ok()
}

fn find_string(name: &str, value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(name).and_then(Value::as_str).filter(|value| !value.is_empty()) {
                return Some(found.to_string());
            }
            map.values().find_map(|nested| find_string(name, nested))
        }
        Value::Array(items) => items.iter().find_map(|nested| find_string(name, nested)),
        _ => None,
    }
}

fn account_id_from_jwt(token: &str) -> Option<String> {
    let payload = jwt_payload(token)?;
    if let Some(auth) = payload.get("https://api.openai.com/auth").and_then(Value::as_object) {
        if let Some(account) = auth.get("chatgpt_account_id").and_then(Value::as_str) {
            return Some(account.to_string());
        }
    }
    payload
        .get("chatgpt_account_id")
        .or_else(|| payload.get("account_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn string_in_jwt(name: &str, token: &str) -> Option<String> {
    let payload = jwt_payload(token)?;
    find_string(name, &payload)
}

fn jwt_payload(token: &str) -> Option<Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let engine = base64::engine::general_purpose::URL_SAFE;
    let data = engine
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]))
        .ok()?;
    serde_json::from_slice(&data).ok()
}

#[allow(unused)]
fn _unused(_: ReasoningEffort) {}
