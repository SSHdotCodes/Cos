//! Port of the Swift CosCoreTests suite (Tests/CosCoreTests/CosCoreTests.swift).

use cos_core::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

/// Spin up a one-shot local HTTP stub serving the release manifest, matching
/// the Swift URLProtocol stub.
fn manifest_stub_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let body = r#"{
          "version": "1.0.0",
          "build": 100,
          "downloadURL": "https://cos.ssh.codes/downloads/Cos-1.0.0-macOS-arm64.zip",
          "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "minimumSystemVersion": "15.0",
          "releaseNotes": "One-click updates."
        }"#;
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.trim().is_empty() => break,
                    _ => {}
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{port}/cos.json")
}

#[tokio::test]
async fn update_check_finds_new_version_or_build() {
    let url = Url::parse(&manifest_stub_server()).unwrap();
    let service = CosUpdateService::new(url);

    let newer_version = service.check("0.3.0", 4).await.unwrap().unwrap();
    assert_eq!(newer_version.version, "1.0.0");

    let newer_build = service.check("1.0.0", 99).await.unwrap().unwrap();
    assert_eq!(newer_build.build, 100);

    let current = service.check("1.0.0", 100).await.unwrap();
    assert!(current.is_none());
}

#[test]
fn update_version_comparison_handles_semantic_components() {
    assert!(cos_core::update::is_newer("0.3.0", "0.2.9"));
    assert!(cos_core::update::is_newer("1.0.0", "0.99.99"));
    assert!(!cos_core::update::is_newer("0.3", "0.3.0"));
    assert!(!cos_core::update::is_newer("0.2.9", "0.3.0"));
}

#[test]
fn update_manifest_decodes_release_metadata() {
    let json = r#"{
      "version": "1.0.0",
      "build": 100,
      "downloadURL": "https://cos.ssh.codes/downloads/Cos-1.0.0-macOS-arm64.zip",
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "minimumSystemVersion": "15.0",
      "releaseNotes": "One-click updates."
    }"#;
    let manifest: CosUpdateManifest = serde_json::from_str(json).unwrap();
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.build, 100);
    assert_eq!(manifest.download_url.host_str(), Some("cos.ssh.codes"));
}

#[test]
fn compaction_keeps_recent_messages_and_creates_checkpoint() {
    let messages: Vec<ChatMessage> = (0..20)
        .map(|index| {
            ChatMessage::new(
                if index % 2 == 0 { MessageRole::User } else { MessageRole::Assistant },
                "context ".to_string() + &format!("{index} ").repeat(120),
            )
        })
        .collect();
    let result = CompactionEngine::prepare(&messages, Some("Earlier checkpoint"), 2_000, 50.0, 500);
    assert!(result.did_compact);
    assert!(result.compacted_summary.is_some());
    assert!(result.prompt_context.contains("Recent verbatim context"));
    assert!(result.prompt_context.contains("context 19"));
}

#[test]
fn settings_plugin_accepts_only_allowlisted_mutation() {
    let valid = r#"Done. <cos-settings>{"key":"fastMode","value":true}</cos-settings>"#;
    let result = CosSettingsPlugin::extract(valid);
    assert_eq!(result.visible_text, "Done.");
    assert_eq!(result.mutation, Some(SettingsMutation::FastMode(true)));

    let invalid = CosSettingsPlugin::extract(r#"No. <cos-settings>{"key":"shellCommand","value":"rm"}</cos-settings>"#);
    assert!(invalid.mutation.is_none());
}

#[test]
fn settings_plugin_parses_guarded_skill_management() {
    let text = r#"Created. <cos-manage>{"action":"createSkill","id":"release-check","name":"Release Check","description":"Verify a release","instructions":"Build and test it."}</cos-manage>"#;
    let result = CosSettingsPlugin::extract(text);
    assert_eq!(result.visible_text, "Created.");
    assert_eq!(
        result.management_action,
        Some(CosManagementAction::CreateSkill {
            id: "release-check".into(),
            name: "Release Check".into(),
            description: "Verify a release".into(),
            instructions: "Build and test it.".into(),
            plugin_id: None,
        })
    );
}

#[test]
fn default_catalog_references_known_providers() {
    let providers = DefaultCatalog::providers();
    let models = DefaultCatalog::models();
    let ids: std::collections::HashSet<&str> = providers.iter().map(|p| p.id.as_str()).collect();
    assert!(!models.is_empty());
    assert!(models.iter().all(|model| ids.contains(model.provider_id.as_str())));
    assert!(providers
        .iter()
        .filter(|provider| provider.bridge != ProviderBridge::Pi)
        .all(|provider| provider.base_url.is_some()));
}

#[test]
fn catalog_uses_model_specific_reasoning_efforts() {
    use ReasoningEffort as E;
    let models = DefaultCatalog::models();
    let find = |id: &str| models.iter().find(|model| model.id == id).unwrap();

    let grok = find("xai:grok-4.5");
    assert_eq!(grok.model, "grok-4.5");
    assert_eq!(grok.effort_options(), &[E::Low, E::Medium, E::High]);
    assert_eq!(grok.normalized_effort(E::Max), E::High);
    assert_eq!(grok.normalized_effort(E::Minimal), E::Low);
    assert!(!grok.supports_fast_mode());

    let opus = find("anthropic:claude-opus-5");
    assert_eq!(opus.effort_options(), &[E::Low, E::Medium, E::High, E::ExtraHigh, E::Max]);
    let sol = find("chatgpt:gpt-5.6-sol");
    assert!(sol.supports_fast_mode());

    let luna = find("chatgpt:gpt-5.6-luna");
    assert_eq!(luna.effort_options(), &E::ALL);
    let haiku = find("anthropic:claude-haiku-4.5");
    assert_eq!(haiku.effort_options(), &[E::Low]);
}

#[test]
fn composer_reference_suggestions_include_commands_skills_and_plugins() {
    let manifest = CosPluginManifest {
        schema_version: 1,
        id: "codes.ssh.cos.computer-use".into(),
        name: "Computer Use".into(),
        version: "1.0.0".into(),
        author: "Cos".into(),
        description: "Operate Mac apps".into(),
        capabilities: vec![],
        skills: vec!["computer-use".into()],
        homepage: None,
        built_in: Some(true),
    };
    let plugin = InstalledPlugin {
        manifest,
        location: std::path::PathBuf::from("/tmp/computer-use"),
        is_trusted: true,
        is_enabled: true,
    };

    let slash_query = ComposerReferenceResolver::query("/", 1).unwrap();
    let slash_suggestions = ComposerReferenceResolver::suggestions(&slash_query, &[plugin.clone()], 8);
    assert!(slash_suggestions.iter().any(|s| s.title == "/subagent"));
    assert!(slash_suggestions.iter().any(|s| s.title == "/goal"));
    assert!(slash_suggestions.iter().any(|s| s.title == "/computer-use"));

    let plugin_query = ComposerReferenceResolver::query("@comp", 5).unwrap();
    let plugin_suggestions = ComposerReferenceResolver::suggestions(&plugin_query, &[plugin], 8);
    assert_eq!(plugin_suggestions.first().map(|s| s.title.as_str()), Some("@computer-use"));

    let replacement = plugin_suggestions.first().unwrap().insertion.clone();
    let query = ComposerReferenceQuery { trigger: '@', term: "comp".into(), range_location: 4, range_length: 5 };
    let (updated, cursor) = ComposerReferenceResolver::replacing_query("Use @comp", &query, &replacement);
    assert_eq!(updated, "Use @computer-use ");
    assert_eq!(cursor, 18);
}

#[test]
fn subagent_route_uses_exact_model_effort_allowlist() {
    let models = DefaultCatalog::models();
    let grok = models.iter().find(|model| model.id == "xai:grok-4.5").unwrap();
    let provider = DefaultCatalog::providers()
        .into_iter()
        .find(|provider| provider.id == grok.provider_id)
        .unwrap();
    let route = SubagentRoute { model: grok.clone(), provider };

    assert!(route.accepts(ReasoningEffort::Low));
    assert!(route.accepts(ReasoningEffort::High));
    assert!(!route.accepts(ReasoningEffort::Minimal));
    assert!(!route.accepts(ReasoningEffort::Max));
    assert_eq!(route.id(), "xai:grok-4.5");
}

#[test]
fn subagent_authority_requires_explicit_positive_user_intent() {
    assert!(SubagentAuthorization::is_explicitly_requested("/subagent ask Grok to review this"));
    assert!(SubagentAuthorization::is_explicitly_requested("Delegate this to another model"));
    assert!(!SubagentAuthorization::is_explicitly_requested("Do not use subagents for this"));
    assert!(SubagentAuthorization::is_explicitly_forbidden("Work without subagents"));
    assert!(!SubagentAuthorization::is_explicitly_requested("Review this implementation"));
}

#[test]
fn agent_request_defaults_to_no_subagent_authority_or_recursion() {
    let model = DefaultCatalog::models().into_iter().next().unwrap();
    let provider = DefaultCatalog::providers()
        .into_iter()
        .find(|provider| provider.id == model.provider_id)
        .unwrap();
    let thread = CosThread::new("/tmp", &model.id);
    let request = AgentRequest::new(
        "hello",
        None,
        thread,
        model,
        provider,
        ReasoningEffort::Low,
        false,
        false,
    );

    assert!(!request.subagents_authorized);
    assert!(request.available_subagent_routes.is_empty());
    assert_eq!(request.agent_depth, 0);
    assert!(request.run_control.is_none());
    assert!(!request.browser_enabled);
}

#[test]
fn betterwright_uses_bounded_stable_session_names() {
    assert_eq!(
        CosBetterWrightRuntime::sanitized_session("Task 123 / Browser"),
        "task-123-browser"
    );
    assert_eq!(CosBetterWrightRuntime::sanitized_session("---"), "default");
    assert!(CosBetterWrightRuntime::sanitized_session(&"a".repeat(200)).len() <= 80);
}

#[tokio::test]
async fn run_control_keeps_steering_fifo_and_bounds_queue() {
    let control = AgentRunControl::new(2);
    let first_accepted = control.submit("first").await;
    let second_accepted = control.submit("second").await;
    let overflow_accepted = control.submit("third").await;
    let messages = control.drain().await;

    assert!(first_accepted);
    assert!(second_accepted);
    assert!(!overflow_accepted);
    assert_eq!(
        messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    assert!(control.drain().await.is_empty());
}

#[tokio::test]
async fn run_control_interrupts_only_installed_provider_generation() {
    let control = AgentRunControl::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let current_token = Uuid::new_v4();
    control
        .install_provider_interrupt(current_token, move || {
            let _ = sender.send(());
        })
        .await;

    assert!(control.submit("change direction").await);
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("provider request interrupted");
    control.clear_provider_interrupt(Uuid::new_v4()).await;

    let second = control.drain().await;
    assert_eq!(
        second.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
        vec!["change direction"]
    );
    control.clear_provider_interrupt(current_token).await;
}

#[test]
fn older_preferences_decode_without_title_model() {
    let json = r#"{"appearance":"system","fastMode":false,"fullAccess":true,"autoCompact":true,"compactAtPercent":78,"keepRecentTokens":20000,"showTokenUsage":false,"animateStreaming":true,"defaultWorkspace":"/tmp","selectedModelID":"chatgpt:gpt-5.6-sol","defaultEffort":"high"}"#;
    let preferences: AppPreferences = serde_json::from_str(json).unwrap();
    assert!(preferences.title_model_id.is_none());
}

#[test]
fn computer_use_can_list_foreground_applications_without_retained_state() {
    let result = CosComputerUseRuntime::execute(
        "computer_list_apps",
        None, None, None, None, None, None, None, None,
    );
    assert!(!result.is_empty());
}

#[test]
fn older_messages_decode_without_work_trace() {
    let id = Uuid::new_v4();
    // Decoded with a secondsSince1970-style timestamp in the Swift test; here
    // we verify workItems is optional regardless of date representation.
    let json = format!(
        r#"{{"id":"{}","role":"assistant","content":"done","createdAt":"1970-01-01T00:00:00Z","isStreaming":false}}"#,
        id.to_string().to_uppercase()
    );
    let message: ChatMessage = serde_json::from_str(&json).unwrap();
    assert!(message.work_items.is_none());
    assert_eq!(message.created_at, OffsetDateTime::UNIX_EPOCH);
}

#[test]
fn thread_store_round_trip() {
    let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
    let store = ThreadStore::new(root.clone());
    let timestamp = OffsetDateTime::from_unix_timestamp(2_000_000_000).unwrap();
    let mut message = ChatMessage::new(MessageRole::User, "hello");
    message.created_at = timestamp;
    let mut thread = CosThread::new("/tmp", "test");
    thread.messages = vec![message];
    thread.created_at = timestamp;
    thread.updated_at = timestamp;

    store.upsert(&thread).unwrap();
    let loaded = store.load_all().unwrap();
    assert_eq!(loaded, vec![thread]);
    let _ = std::fs::remove_dir_all(root);
}

/// Extra coverage for the ported transport/tool layer (beyond the Swift suite).
#[test]
fn tool_call_extracts_marker_and_visible_prefix() {
    let text = "Checking the file first.\n<cos-tool>{\"name\":\"read_file\",\"path\":\"src/main.rs\",\"limit\":100}</cos-tool>";
    let call = CosToolCall::extract(text).unwrap();
    assert_eq!(call.name, "read_file");
    assert_eq!(call.path.as_deref(), Some("src/main.rs"));
    assert_eq!(call.limit, Some(100));
    assert_eq!(call.visible_prefix, "Checking the file first.\n");
    assert_eq!(call.display_detail(), "src/main.rs");
}

#[test]
fn tool_call_rejects_oversized_or_malformed_markers() {
    assert!(CosToolCall::extract("no marker").is_none());
    assert!(CosToolCall::extract("<cos-tool>{\"path\":\"x\"}</cos-tool>").is_none());
    let oversized = format!("<cos-tool>{{\"name\":\"read_file\",\"content\":\"{}\"}}</cos-tool>", "x".repeat(200_000));
    assert!(CosToolCall::extract(&oversized).is_none());
}

#[test]
fn thread_snapshot_matches_swift_iso8601_layout() {
    let timestamp = OffsetDateTime::from_unix_timestamp(2_000_000_000).unwrap();
    let mut thread = CosThread::new("/tmp", "chatgpt:gpt-5.6-sol");
    thread.created_at = timestamp;
    thread.updated_at = timestamp;
    let json = serde_json::to_string_pretty(&thread).unwrap();
    assert!(json.contains("\"workspacePath\": \"/tmp\""));
    assert!(json.contains("\"modelID\": \"chatgpt:gpt-5.6-sol\""));
    assert!(json.contains("\"effort\": \"high\""));
    assert!(json.contains("\"2033-05-18T03:33:20Z\""));
    // Swift decodes what it encoded: round trip.
    let decoded: CosThread = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, thread);
}

#[test]
fn system_prompt_contains_capability_sections() {
    let model = DefaultCatalog::models().into_iter().next().unwrap();
    let provider = DefaultCatalog::providers()
        .into_iter()
        .find(|provider| provider.id == model.provider_id)
        .unwrap();
    let thread = CosThread::new("/tmp", &model.id);
    let mut request = AgentRequest::new(
        "Do the thing",
        None,
        thread,
        model,
        provider,
        ReasoningEffort::High,
        false,
        true,
    );
    request.computer_use_enabled = true;
    request.browser_enabled = true;
    let prompt = cos_core::harness::system_prompt(&request);
    assert!(prompt.contains("cos-tool"));
    assert!(prompt.contains("computer_get_state"));
    assert!(prompt.contains("browser_run"));
    assert!(prompt.contains("Never emit spawn_subagent"));
    assert!(prompt.contains("cos-settings"));
    assert!(prompt.contains("Do the thing"));
}
