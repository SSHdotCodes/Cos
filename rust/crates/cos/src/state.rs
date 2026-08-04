//! AppModel — the state owner, ported from the Swift AppModel. Core runtime
//! work happens on a dedicated tokio runtime; events stream back over
//! executor-agnostic futures channels.

use cos_core::*;
use gpui::{App, AsyncApp, Context, Entity, Task, WeakEntity};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("cos-core")
            .build()
            .expect("tokio runtime")
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDirectoryTrust {
    pub thread_id: Uuid,
    pub workspace_path: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingComputerUseRun {
    pub thread_id: Uuid,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalSkillSource {
    Codex,
    ClaudeCode,
    Folder,
}

impl ExternalSkillSource {
    pub const ALL: [ExternalSkillSource; 3] = [Self::Codex, Self::ClaudeCode, Self::Folder];

    pub fn title(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::Folder => "Another folder",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Codex => "Import skills from ~/.codex/skills",
            Self::ClaudeCode => "Import skills from ~/.claude/skills",
            Self::Folder => "Choose any folder containing SKILL.md bundles",
        }
    }

    pub fn plugin_id(self) -> String {
        let raw = match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claudecode",
            Self::Folder => "folder",
        };
        format!("codes.ssh.cos.imported-{raw}")
    }

    pub fn default_roots(self) -> Vec<PathBuf> {
        let home = dirs_home();
        match self {
            Self::Codex => vec![home.join(".codex/skills")],
            Self::ClaudeCode => vec![home.join(".claude/skills")],
            Self::Folder => vec![],
        }
    }
}

pub struct ActiveRun {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub assistant_id: Uuid,
    pub control: Arc<AgentRunControl>,
}

pub struct AppModel {
    pub threads: Vec<CosThread>,
    pub selected_thread_id: Option<Uuid>,
    pub preferences: AppPreferences,
    pub providers: Vec<ProviderProfile>,
    pub models: Vec<ModelProfile>,
    pub plugins: Vec<InstalledPlugin>,
    pub is_running: bool,
    pub activity: String,
    pub last_error: Option<String>,
    pub login_status: HashMap<String, String>,
    pub provider_sessions: HashMap<String, ProviderSessionInfo>,
    pub is_plugin_library_presented: bool,
    pub pending_directory_trust: Option<PendingDirectoryTrust>,
    pub skill_import_counts: HashMap<ExternalSkillSource, usize>,
    pub skill_import_status: HashMap<ExternalSkillSource, String>,
    pub available_update: Option<CosUpdateManifest>,
    pub is_checking_for_update: bool,
    pub is_installing_update: bool,
    pub update_status: Option<String>,
    pub computer_use_access_granted: bool,
    pub computer_use_access_status: Option<String>,
    pub marketplace_plugins: Vec<CosMarketplaceListing>,
    pub is_loading_marketplace: bool,
    pub installing_marketplace_plugin_id: Option<String>,
    pub marketplace_error: Option<String>,
    pub is_browser_panel_presented: bool,
    pub pending_computer_use_run: Option<PendingComputerUseRun>,
    pub current_version: String,
    pub current_build: i64,

    store: ThreadStore,
    runtime: AgentRuntime,
    secure_store: SecureStore,
    update_service: Arc<CosUpdateService>,
    active_run: Option<ActiveRun>,
    run_task: Option<Task<()>>,
    title_tasks: HashMap<Uuid, Task<()>>,
    last_update_check: Option<Instant>,
    pub trusted_workspaces: HashSet<String>,
    pub disabled_plugin_ids: HashSet<String>,
    pub disabled_skill_keys: HashSet<String>,
    built_in_plugins_url: PathBuf,
    event_bridge: Arc<Mutex<Option<futures::channel::mpsc::UnboundedSender<ModelCommand>>>>,
}

/// Cross-executor commands from core callbacks into the model.
pub enum ModelCommand {
    ThreadPersisted,
}

impl AppModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        crate::embedded::materialize();
        let built_in = crate::embedded::built_in_plugins_root();
        let mut model = Self {
            threads: Vec::new(),
            selected_thread_id: None,
            preferences: crate::prefs::load("preferences").unwrap_or_default(),
            providers: merge_providers(crate::prefs::load("providers")),
            models: merge_models(crate::prefs::load("models")),
            plugins: Vec::new(),
            is_running: false,
            activity: "Ready".into(),
            last_error: None,
            login_status: HashMap::new(),
            provider_sessions: HashMap::new(),
            is_plugin_library_presented: false,
            pending_directory_trust: None,
            skill_import_counts: HashMap::new(),
            skill_import_status: HashMap::new(),
            available_update: None,
            is_checking_for_update: false,
            is_installing_update: false,
            update_status: None,
            computer_use_access_granted: CosComputerUseAccess::is_granted(),
            computer_use_access_status: None,
            marketplace_plugins: Vec::new(),
            is_loading_marketplace: false,
            installing_marketplace_plugin_id: None,
            marketplace_error: None,
            is_browser_panel_presented: false,
            pending_computer_use_run: None,
            current_version: crate::app_version().0,
            current_build: crate::app_version().1,
            store: ThreadStore::default(),
            runtime: AgentRuntime::default(),
            secure_store: SecureStore::default(),
            update_service: Arc::new(CosUpdateService::new(
                url::Url::parse(CosUpdateService::DEFAULT_FEED_URL).unwrap(),
            )),
            active_run: None,
            run_task: None,
            title_tasks: HashMap::new(),
            last_update_check: None,
            trusted_workspaces: crate::prefs::load::<Vec<String>>("trustedWorkspaces")
                .unwrap_or_default()
                .into_iter()
                .collect(),
            disabled_plugin_ids: crate::prefs::load::<Vec<String>>("disabledPluginIDs")
                .unwrap_or_default()
                .into_iter()
                .collect(),
            disabled_skill_keys: crate::prefs::load::<Vec<String>>("disabledSkillKeys")
                .unwrap_or_default()
                .into_iter()
                .collect(),
            built_in_plugins_url: built_in,
            event_bridge: Arc::new(Mutex::new(None)),
        };
        model.normalize_loaded_thread_efforts();
        model.bootstrap(cx);
        model
    }

    // MARK: - Accessors

    pub fn selected_thread(&self) -> Option<&CosThread> {
        self.threads.iter().find(|thread| Some(thread.id) == self.selected_thread_id)
    }

    fn selected_thread_index(&self) -> Option<usize> {
        self.threads.iter().position(|thread| Some(thread.id) == self.selected_thread_id)
    }

    pub fn selected_model(&self) -> ModelProfile {
        let id = self
            .selected_thread()
            .map(|thread| thread.model_id.clone())
            .unwrap_or_else(|| self.preferences.selected_model_id.clone());
        self.models
            .iter()
            .find(|model| model.id == id)
            .unwrap_or(&self.models[0])
            .clone()
    }

    pub fn selected_provider(&self) -> ProviderProfile {
        let provider_id = self.selected_model().provider_id;
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .unwrap_or(&self.providers[0])
            .clone()
    }

    pub fn can_steer_selected_thread(&self) -> bool {
        self.is_running
            && self
                .active_run
                .as_ref()
                .map(|run| Some(run.thread_id) == self.selected_thread_id)
                .unwrap_or(false)
    }

    pub fn is_betterwright_enabled(&self) -> bool {
        self.plugins
            .iter()
            .any(|plugin| plugin.id() == "codes.ssh.cos.betterwright" && plugin.is_enabled)
    }

    pub fn subagent_routes(&self) -> Vec<SubagentRoute> {
        self.runtime.accessible_subagent_routes(&self.providers, &self.models)
    }

    pub fn title_models(&self) -> Vec<ModelProfile> {
        ["chatgpt:gpt-5.6-luna", "xai:grok-4.5", "anthropic:claude-haiku-4.5"]
            .iter()
            .filter_map(|id| self.models.iter().find(|model| &model.id == id).cloned())
            .collect()
    }

    pub fn selected_title_model(&self) -> Option<ModelProfile> {
        let requested = self
            .preferences
            .title_model_id
            .clone()
            .unwrap_or_else(|| "chatgpt:gpt-5.6-luna".to_string());
        let title_models = self.title_models();
        title_models
            .iter()
            .find(|model| model.id == requested)
            .or_else(|| title_models.first())
            .cloned()
    }

    // MARK: - Bootstrap

    fn bootstrap(&mut self, cx: &mut Context<Self>) {
        let threads = self.store.load_all().unwrap_or_else(|error| {
            self.last_error = Some(format!("Could not load tasks: {error}"));
            Vec::new()
        });
        self.threads = threads;
        self.normalize_loaded_thread_efforts();
        if self.threads.is_empty() {
            self.new_thread(cx);
        } else {
            self.selected_thread_id = self.threads.first().map(|thread| thread.id);
        }
        self.reload_plugins(cx);
        self.refresh_skill_import_counts();
        self.refresh_provider_sessions();
        self.check_for_updates(false, cx);
        self.schedule_periodic_update_checks(cx);
        self.start_computer_use_watcher(cx);
        cx.notify();
    }

    fn schedule_periodic_update_checks(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(Duration::from_secs(6 * 60 * 60)).await;
                let alive = this
                    .update(cx, |model, cx| {
                        model.check_for_updates(false, cx);
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    fn start_computer_use_watcher(&mut self, cx: &mut Context<Self>) {
        // Mirrors Swift's activate-notification + pending-request polling.
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let alive = this
                    .update(cx, |model, cx| {
                        let cu_enabled = model
                            .plugins
                            .iter()
                            .any(|plugin| plugin.id() == "codes.ssh.cos.computer-use" && plugin.is_enabled);
                        if cu_enabled || model.pending_computer_use_run.is_some() {
                            model.refresh_computer_use_access(cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    // MARK: - Updates

    pub fn check_for_updates(&mut self, manually: bool, cx: &mut Context<Self>) {
        if self.is_checking_for_update || self.is_installing_update {
            return;
        }
        if !manually
            && self
                .last_update_check
                .map(|last| last.elapsed() < Duration::from_secs(6 * 60 * 60))
                .unwrap_or(false)
        {
            return;
        }
        self.is_checking_for_update = true;
        if manually {
            self.update_status = Some("Checking for updates…".into());
        }
        cx.notify();
        let service = self.update_service.clone();
        let current_version = self.current_version.clone();
        let current_build = self.current_build;
        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let result = service.check(&current_version, current_build).await;
            let _ = sender.send(result);
        });
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let result = receiver.await;
            let _ = this.update(cx, |model, cx| {
                model.is_checking_for_update = false;
                match result {
                    Ok(Ok(manifest)) => {
                        model.last_update_check = Some(Instant::now());
                        if let Some(manifest) = manifest {
                            model.update_status = Some(format!("Cos {} is ready to install.", manifest.version));
                            model.available_update = Some(manifest);
                        } else {
                            model.update_status = manually.then(|| "Cos is up to date.".to_string());
                        }
                    }
                    Ok(Err(error)) => {
                        if manually {
                            model.update_status = None;
                            model.last_error = Some(format!("Could not check for updates: {error}"));
                        }
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn install_available_update(&mut self, cx: &mut Context<Self>) {
        let Some(update) = self.available_update.clone() else { return };
        if self.is_installing_update {
            return;
        }
        if self.is_running {
            self.last_error = Some("Stop the current task before installing the update.".into());
            cx.notify();
            return;
        }
        let Some(current_app_url) = cos_core::bundle_url() else {
            self.last_error = Some(CosUpdateError::NotRunningFromApp.to_string());
            cx.notify();
            return;
        };
        if let Err(error) = CosUpdateService::validate_install_location(&current_app_url) {
            self.last_error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.is_installing_update = true;
        self.update_status = Some(format!("Downloading Cos {}…", update.version));
        cx.notify();
        let process_id = std::process::id() as i32;
        let service = self.update_service.clone();
        let version = update.version.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let result = service.download_and_verify(&update).await;
            let _ = sender.send(result);
        });
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let result = receiver.await;
            match result {
                Ok(Ok(prepared)) => {
                    let quit = this
                        .update(cx, |model, cx| {
                            model.update_status = Some("Installing and restarting…".into());
                            cx.notify();
                            match CosUpdateService::schedule_replacement(&prepared, &current_app_url, process_id) {
                                Ok(()) => true,
                                Err(error) => {
                                    model.is_installing_update = false;
                                    model.update_status = None;
                                    model.last_error = Some(format!(
                                        "Could not install Cos {version}: {error}"
                                    ));
                                    cx.notify();
                                    false
                                }
                            }
                        })
                        .unwrap_or(false);
                    if quit {
                        let _ = cx.update(|cx| {
                            cx.quit();
                        });
                    }
                }
                Ok(Err(error)) => {
                    let _ = this.update(cx, |model, cx| {
                        model.is_installing_update = false;
                        model.update_status = None;
                        model.last_error = Some(format!("Could not install Cos {version}: {error}"));
                        cx.notify();
                    });
                }
                Err(_) => {}
            }
        })
        .detach();
    }

    // MARK: - Threads

    pub fn new_thread(&mut self, cx: &mut Context<Self>) {
        self.new_thread_in(None, cx);
    }

    pub fn new_thread_in(&mut self, workspace_path: Option<String>, cx: &mut Context<Self>) {
        let default_model = self
            .models
            .iter()
            .find(|model| model.id == self.preferences.selected_model_id)
            .unwrap_or(&self.models[0])
            .clone();
        let mut thread = CosThread::new(
            workspace_path.unwrap_or_else(|| self.preferences.default_workspace.clone()),
            self.preferences.selected_model_id.clone(),
        );
        thread.effort = default_model.normalized_effort(self.preferences.default_effort);
        self.threads.insert(0, thread.clone());
        self.selected_thread_id = Some(thread.id);
        self.persist(&thread);
        cx.notify();
    }

    pub fn delete_thread(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.is_running && self.selected_thread_id == Some(id) {
            return;
        }
        if let Some(task) = self.title_tasks.remove(&id) {
            task.detach();
        }
        self.threads.retain(|thread| thread.id != id);
        if self.selected_thread_id == Some(id) {
            self.selected_thread_id = self.threads.first().map(|thread| thread.id);
        }
        let store = self.store.clone();
        tokio_runtime().spawn_blocking(move || {
            let _ = store.delete(id);
        });
        if self.threads.is_empty() {
            self.new_thread(cx);
        }
        cx.notify();
    }

    pub fn set_workspace(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(index) = self.selected_thread_index() else { return };
        self.threads[index].workspace_path = path;
        self.threads[index].updated_at = time::OffsetDateTime::now_utc();
        let thread = self.threads[index].clone();
        self.persist(&thread);
        cx.notify();
    }

    pub fn select_thread(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.selected_thread_id != Some(id) {
            self.selected_thread_id = Some(id);
            cx.notify();
        }
    }

    pub fn select_model(&mut self, model_id: &str, cx: &mut Context<Self>) {
        let Some(model) = self.models.iter().find(|model| model.id == model_id).cloned() else {
            return;
        };
        let Some(index) = self.selected_thread_index() else { return };
        let effort = model.normalized_effort(self.threads[index].effort);
        self.threads[index].model_id = model.id.clone();
        self.threads[index].effort = effort;
        self.threads[index].updated_at = time::OffsetDateTime::now_utc();
        self.preferences.selected_model_id = model.id.clone();
        self.preferences.default_effort = effort;
        self.persist_preferences();
        let thread = self.threads[index].clone();
        self.persist(&thread);
        cx.notify();
    }

    pub fn set_effort(&mut self, effort: ReasoningEffort, cx: &mut Context<Self>) {
        let Some(index) = self.selected_thread_index() else { return };
        let effort = self.selected_model().normalized_effort(effort);
        self.threads[index].effort = effort;
        self.threads[index].updated_at = time::OffsetDateTime::now_utc();
        self.preferences.default_effort = effort;
        self.persist_preferences();
        let thread = self.threads[index].clone();
        self.persist(&thread);
        cx.notify();
    }

    pub fn create_goal(&mut self, objective: String, budget: Option<i64>, cx: &mut Context<Self>) {
        let Some(index) = self.selected_thread_index() else { return };
        self.threads[index].goal = Some(AgentGoal::new(objective, budget));
        let thread = self.threads[index].clone();
        self.persist(&thread);
        cx.notify();
    }

    pub fn clear_goal(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.selected_thread_index() else { return };
        self.threads[index].goal = None;
        let thread = self.threads[index].clone();
        self.persist(&thread);
        cx.notify();
    }

    // MARK: - Running

    pub fn send(&mut self, raw_prompt: &str, cx: &mut Context<Self>) {
        self.start_run(raw_prompt, true, cx);
    }

    pub fn steer(&mut self, raw_prompt: &str, cx: &mut Context<Self>) {
        let prompt = raw_prompt.trim().to_string();
        let Some(run) = &self.active_run else { return };
        if prompt.is_empty() || !self.is_running || Some(run.thread_id) != self.selected_thread_id {
            return;
        }
        let control = run.control.clone();
        self.activity = "Applying steering…".into();
        cx.notify();
        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let _ = sender.send(control.submit(&prompt).await);
        });
        let thread_id = run.thread_id;
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let accepted = receiver.await.unwrap_or(false);
            let _ = this.update(cx, |model, cx| {
                if !accepted
                    && model
                        .active_run
                        .as_ref()
                        .map(|run| run.thread_id == thread_id)
                        .unwrap_or(false)
                {
                    model.activity = "Steering queue is full".into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn start_run(&mut self, raw_prompt: &str, append_user_message: bool, cx: &mut Context<Self>) {
        let prompt = raw_prompt.trim().to_string();
        if prompt.is_empty() || self.is_running {
            return;
        }
        let Some(index) = self.selected_thread_index() else { return };
        if self
            .pending_directory_trust
            .as_ref()
            .map(|pending| pending.thread_id == self.threads[index].id)
            .unwrap_or(false)
        {
            return;
        }

        if append_user_message {
            self.threads[index]
                .messages
                .push(ChatMessage::new(MessageRole::User, prompt.clone()));
            if self.threads[index].messages.len() == 1 {
                self.threads[index].title = "New task".into();
                self.schedule_title_generation(self.threads[index].id, prompt.clone(), cx);
            }
            if self.handle_slash_command(&prompt, index, cx) {
                self.threads[index].updated_at = time::OffsetDateTime::now_utc();
                let thread = self.threads[index].clone();
                self.persist(&thread);
                cx.notify();
                return;
            }
        }

        let computer_use_enabled = self
            .plugins
            .iter()
            .any(|plugin| plugin.id() == "codes.ssh.cos.computer-use" && plugin.is_enabled);
        self.computer_use_access_granted = CosComputerUseAccess::is_granted();
        if computer_use_enabled
            && looks_like_computer_use_request(&prompt)
            && !self.computer_use_access_granted
        {
            self.pending_computer_use_run = Some(PendingComputerUseRun {
                thread_id: self.threads[index].id,
                prompt: prompt.clone(),
            });
            self.threads[index].messages.push(ChatMessage::new(
                MessageRole::Assistant,
                "Cos needs macOS Accessibility access for this task. I’ll continue automatically as soon as the permission becomes active.",
            ));
            self.threads[index].updated_at = time::OffsetDateTime::now_utc();
            let thread = self.threads[index].clone();
            self.persist(&thread);
            self.request_computer_use_access(cx);
            cx.notify();
            return;
        }

        let assistant_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let run_control = Arc::new(AgentRunControl::default());
        self.threads[index].messages.push(ChatMessage::streaming_assistant(assistant_id));
        self.threads[index].updated_at = time::OffsetDateTime::now_utc();
        self.is_running = true;
        self.active_run = Some(ActiveRun {
            id: run_id,
            thread_id: self.threads[index].id,
            assistant_id,
            control: run_control.clone(),
        });
        self.activity = "Preparing context…".into();
        self.last_error = None;

        let thread = &self.threads[index];
        let compaction = CompactionEngine::prepare(
            &thread.messages[..thread.messages.len() - 1],
            thread.compacted_context.as_deref(),
            self.selected_model().context_window,
            if self.preferences.auto_compact {
                self.preferences.compact_at_percent
            } else {
                101.0
            },
            self.preferences.keep_recent_tokens,
        );
        if compaction.did_compact {
            self.threads[index].compacted_context = compaction.compacted_summary.clone();
            self.activity = "Context compacted".into();
        }

        let thread = &self.threads[index];
        let goal_context = thread
            .goal
            .as_ref()
            .map(|goal| {
                format!(
                    "Active goal: {}\nGoal status: {}\nTokens used: {}\n",
                    goal.objective,
                    goal.status.raw_value(),
                    goal.used_tokens
                )
            })
            .unwrap_or_default();
        let reference_plugins: Vec<InstalledPlugin> = self
            .plugins
            .iter()
            .map(|plugin| {
                let mut visible = plugin.clone();
                visible.manifest.skills = plugin
                    .manifest
                    .skills
                    .iter()
                    .filter(|skill| self.is_skill_enabled(skill, plugin))
                    .cloned()
                    .collect();
                visible
            })
            .collect();
        let reference_context = ComposerReferenceResolver::reference_context(&prompt, &reference_plugins);
        let subagents_authorized = SubagentAuthorization::is_explicitly_requested(&prompt);
        let available_subagent_routes = self.subagent_routes();
        let browser_enabled = self.is_betterwright_enabled();
        let effective_prompt = format!(
            "{}\n\n{}\n{}\nConversation context:\n{}\n\nContinue the task. The newest user request is: {}",
            CosSettingsPlugin::SYSTEM_PROMPT,
            goal_context,
            reference_context,
            compaction.prompt_context,
            prompt
        );
        let mut request = AgentRequest::new(
            effective_prompt,
            Some(prompt.clone()),
            self.threads[index].clone(),
            self.selected_model(),
            self.selected_provider(),
            self.threads[index].effort,
            self.preferences.fast_mode,
            self.preferences.full_access,
        );
        request.workspace_is_trusted = self.is_workspace_trusted(&self.threads[index].workspace_path);
        request.extension_instructions = self.active_extension_instructions();
        request.computer_use_enabled = computer_use_enabled;
        request.browser_enabled = browser_enabled;
        request.available_subagent_routes = available_subagent_routes;
        request.subagents_authorized = subagents_authorized;
        request.run_control = Some(run_control);
        let thread = self.threads[index].clone();
        self.persist(&thread);

        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let runtime = AgentRuntime::default();
            let _ = sender.send(runtime.stream(request));
        });

        let task = cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let Ok(stream_result) = receiver.await else { return };
            let mut stream = match stream_result {
                Ok(stream) => stream,
                Err(error) => {
                    let _ = this.update(cx, |model, cx| {
                        model.fail_start(run_id, error, cx);
                    });
                    return;
                }
            };
            while let Some(event) = stream.next().await {
                let alive = this
                    .update(cx, |model, cx| {
                        if !model.is_active(run_id) {
                            return false;
                        }
                        match event {
                            Ok(AgentEvent::SteeringApplied(messages)) => {
                                model.apply_steering(messages, run_id, cx);
                            }
                            Ok(event) => model.handle_event(event, run_id, cx),
                            Err(error) => {
                                model.fail_run(run_id, error, cx);
                                return false;
                            }
                        }
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    return;
                }
            }
            let _ = this.update(cx, |model, cx| {
                if model.is_active(run_id) {
                    model.finish_assistant(run_id, cx);
                }
            });
        });
        self.run_task = Some(task);
        cx.notify();
    }

    fn is_active(&self, run_id: Uuid) -> bool {
        self.active_run.as_ref().map(|run| run.id == run_id).unwrap_or(false)
    }

    fn handle_event(&mut self, event: AgentEvent, run_id: Uuid, cx: &mut Context<Self>) {
        let Some(run) = self.active_run.as_ref() else { return };
        if run.id != run_id {
            return;
        }
        let (thread_id, assistant_id) = (run.thread_id, run.assistant_id);
        let Some(thread_index) = self.threads.iter().position(|thread| thread.id == thread_id) else {
            return;
        };
        match event {
            AgentEvent::Status(status) => {
                self.activity = status.clone();
                self.append_work(
                    WorkTraceItem::new(WorkTraceKind::Status, status, ""),
                    assistant_id,
                    thread_index,
                    true,
                );
            }
            AgentEvent::WorkDelta(text) => self.append_reasoning(&text, assistant_id, thread_index),
            AgentEvent::TextDelta(text) => {
                if let Some(message_index) = self.threads[thread_index]
                    .messages
                    .iter()
                    .position(|message| message.id == assistant_id)
                {
                    self.threads[thread_index].messages[message_index].content.push_str(&text);
                    self.activity = "Working…".into();
                }
            }
            AgentEvent::Tool { name, detail } => {
                self.activity = if detail.is_empty() {
                    format!("Using {name}…")
                } else {
                    format!("{name}: {detail}")
                };
                let title = title_case(&name.replace('_', " "));
                self.append_work(
                    WorkTraceItem::new(WorkTraceKind::Tool, title, detail),
                    assistant_id,
                    thread_index,
                    false,
                );
            }
            AgentEvent::Subagent { name, detail } => {
                self.activity = format!("{name}: {detail}");
                self.upsert_subagent_work(&name, &detail, assistant_id, thread_index);
            }
            AgentEvent::SteeringApplied(_) => {}
            AgentEvent::Usage { input, output } => {
                if let Some(goal) = self.threads[thread_index].goal.as_mut() {
                    goal.used_tokens += input + output;
                    if let Some(budget) = goal.token_budget {
                        if goal.used_tokens >= budget {
                            goal.status = GoalStatus::BudgetLimited;
                        }
                    }
                }
                if self.preferences.show_token_usage {
                    self.activity = format!(
                        "↑ {}  ↓ {} tokens",
                        crate::theme::format_number(input),
                        crate::theme::format_number(output)
                    );
                }
            }
            AgentEvent::Completed => self.activity = "Complete".into(),
        }
        cx.notify();
    }

    fn append_work(&mut self, item: WorkTraceItem, assistant_id: Uuid, thread_index: usize, coalesce: bool) {
        let Some(message_index) = self.threads[thread_index]
            .messages
            .iter()
            .position(|message| message.id == assistant_id)
        else {
            return;
        };
        let items = self.threads[thread_index].messages[message_index]
            .work_items
            .get_or_insert_with(Vec::new);
        if coalesce
            && items
                .last()
                .map(|last| last.kind == item.kind && last.title == item.title)
                .unwrap_or(false)
        {
            return;
        }
        if items.len() < 120 {
            items.push(item);
        }
    }

    fn append_reasoning(&mut self, text: &str, assistant_id: Uuid, thread_index: usize) {
        if text.is_empty() {
            return;
        }
        let Some(message_index) = self.threads[thread_index]
            .messages
            .iter()
            .position(|message| message.id == assistant_id)
        else {
            return;
        };
        let items = self.threads[thread_index].messages[message_index]
            .work_items
            .get_or_insert_with(Vec::new);
        if let Some(last) = items.last_mut() {
            if last.kind == WorkTraceKind::Reasoning && last.detail.len() < 24_000 {
                last.detail.push_str(text);
                return;
            }
        }
        if items.len() < 120 {
            items.push(WorkTraceItem::new(WorkTraceKind::Reasoning, "Reasoning", text));
        }
    }

    fn upsert_subagent_work(&mut self, name: &str, detail: &str, assistant_id: Uuid, thread_index: usize) {
        let Some(message_index) = self.threads[thread_index]
            .messages
            .iter()
            .position(|message| message.id == assistant_id)
        else {
            return;
        };
        let items = self.threads[thread_index].messages[message_index]
            .work_items
            .get_or_insert_with(Vec::new);
        if let Some(last) = items.last_mut() {
            if last.kind == WorkTraceKind::Subagent && last.title == name {
                last.detail = detail.to_string();
                return;
            }
        }
        if items.len() < 120 {
            items.push(WorkTraceItem::new(WorkTraceKind::Subagent, name, detail));
        }
    }

    fn apply_steering(&mut self, messages: Vec<SteeringMessage>, run_id: Uuid, cx: &mut Context<Self>) {
        if messages.is_empty() {
            return;
        }
        let Some(run) = self.active_run.as_ref() else { return };
        if run.id != run_id {
            return;
        }
        let (thread_id, assistant_id) = (run.thread_id, run.assistant_id);
        let Some(thread_index) = self.threads.iter().position(|thread| thread.id == thread_id) else {
            return;
        };
        let detail = messages
            .iter()
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(message_index) = self.threads[thread_index]
            .messages
            .iter()
            .position(|message| message.id == assistant_id)
        {
            self.threads[thread_index].messages[message_index].is_streaming = false;
        }
        self.append_work(
            WorkTraceItem::new(WorkTraceKind::Status, "Steered", detail),
            assistant_id,
            thread_index,
            false,
        );
        for message in messages {
            self.threads[thread_index]
                .messages
                .push(ChatMessage::new(MessageRole::User, message.content));
        }
        let next_assistant_id = Uuid::new_v4();
        self.threads[thread_index]
            .messages
            .push(ChatMessage::streaming_assistant(next_assistant_id));
        self.threads[thread_index].updated_at = time::OffsetDateTime::now_utc();
        if let Some(run) = self.active_run.as_mut() {
            run.assistant_id = next_assistant_id;
        }
        self.activity = "Steering applied".into();
        let thread = self.threads[thread_index].clone();
        self.persist(&thread);
        cx.notify();
    }

    fn finish_assistant(&mut self, run_id: Uuid, cx: &mut Context<Self>) {
        let Some(run) = self.active_run.take() else { return };
        if run.id != run_id {
            self.active_run = Some(run);
            return;
        }
        self.run_task = None;
        let Some(thread_index) = self.threads.iter().position(|thread| thread.id == run.thread_id) else {
            return;
        };
        let Some(message_index) = self.threads[thread_index]
            .messages
            .iter()
            .position(|message| message.id == run.assistant_id)
        else {
            return;
        };
        let extraction = CosSettingsPlugin::extract(&self.threads[thread_index].messages[message_index].content);
        self.threads[thread_index].messages[message_index].content = extraction.visible_text;
        self.threads[thread_index].messages[message_index].is_streaming = false;
        if let Some(mutation) = extraction.mutation {
            self.apply_mutation(mutation, cx);
        }
        if let Some(action) = extraction.management_action {
            self.apply_management(action, cx);
        }
        self.threads[thread_index].updated_at = time::OffsetDateTime::now_utc();
        self.is_running = false;
        self.activity = "Ready".into();
        let thread = self.threads[thread_index].clone();
        self.persist(&thread);
        cx.notify();
    }

    fn fail_start(&mut self, run_id: Uuid, error: AgentRuntimeError, cx: &mut Context<Self>) {
        // Runtime refused to start (e.g. directory trust, missing key).
        let Some(run) = self.active_run.take() else { return };
        if run.id != run_id {
            self.active_run = Some(run);
            return;
        }
        self.run_task = None;
        self.fail_assistant_inner(run.thread_id, run.assistant_id, error, cx);
    }

    fn fail_run(&mut self, run_id: Uuid, error: AgentRuntimeError, cx: &mut Context<Self>) {
        let Some(run) = self.active_run.take() else { return };
        if run.id != run_id {
            self.active_run = Some(run);
            return;
        }
        self.run_task = None;
        self.fail_assistant_inner(run.thread_id, run.assistant_id, error, cx);
    }

    fn fail_assistant_inner(
        &mut self,
        thread_id: Uuid,
        assistant_id: Uuid,
        error: AgentRuntimeError,
        cx: &mut Context<Self>,
    ) {
        let Some(thread_index) = self.threads.iter().position(|thread| thread.id == thread_id) else {
            return;
        };
        let Some(message_index) = self.threads[thread_index]
            .messages
            .iter()
            .position(|message| message.id == assistant_id)
        else {
            return;
        };
        if let AgentRuntimeError::DirectoryTrustRequired(workspace_path) = &error {
            let retry_prompt = self.threads[thread_index]
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::User)
                .map(|message| message.content.clone())
                .unwrap_or_default();
            self.threads[thread_index].messages.remove(message_index);
            self.threads[thread_index].updated_at = time::OffsetDateTime::now_utc();
            self.is_running = false;
            self.activity = "Waiting for directory trust".into();
            self.last_error = None;
            self.pending_directory_trust = Some(PendingDirectoryTrust {
                thread_id,
                workspace_path: workspace_path.clone(),
                prompt: retry_prompt,
            });
            let thread = self.threads[thread_index].clone();
            self.persist(&thread);
            cx.notify();
            return;
        }
        if self.threads[thread_index].messages[message_index].content.is_empty() {
            self.threads[thread_index].messages[message_index].content =
                format!("I couldn’t start this run. {error}");
        }
        self.threads[thread_index].messages[message_index].is_streaming = false;
        self.is_running = false;
        self.activity = "Needs attention".into();
        self.last_error = Some(error.to_string());
        let thread = self.threads[thread_index].clone();
        self.persist(&thread);
        cx.notify();
    }

    pub fn trust_pending_workspace_and_continue(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_directory_trust.clone() else { return };
        if !self.threads.iter().any(|thread| thread.id == pending.thread_id) {
            return;
        }
        self.trusted_workspaces
            .insert(normalize_workspace_path(&pending.workspace_path));
        let workspaces: Vec<String> = self.trusted_workspaces.iter().cloned().collect();
        crate::prefs::save("trustedWorkspaces", &workspaces);
        self.pending_directory_trust = None;
        self.selected_thread_id = Some(pending.thread_id);
        self.activity = "Directory trusted".into();
        self.start_run(&pending.prompt, false, cx);
    }

    pub fn decline_pending_workspace_trust(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_directory_trust.take() else { return };
        if let Some(index) = self.threads.iter().position(|thread| thread.id == pending.thread_id) {
            self.threads[index].messages.push(ChatMessage::new(
                MessageRole::Assistant,
                "Run canceled. This directory remains untrusted.",
            ));
            self.threads[index].updated_at = time::OffsetDateTime::now_utc();
            let thread = self.threads[index].clone();
            self.persist(&thread);
        }
        self.activity = "Ready".into();
        cx.notify();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.active_run.take() else { return };
        run.control.clear_provider_interrupt(run.id);
        self.run_task = None;
        self.is_running = false;
        self.activity = "Stopped".into();
        if let Some(thread_index) = self.threads.iter().position(|thread| thread.id == run.thread_id) {
            if let Some(message_index) = self.threads[thread_index]
                .messages
                .iter()
                .position(|message| message.id == run.assistant_id)
            {
                self.threads[thread_index].messages[message_index].is_streaming = false;
                let thread = self.threads[thread_index].clone();
                self.persist(&thread);
            }
        }
        cx.notify();
    }

    // MARK: - Slash commands (/goal)

    fn handle_slash_command(&mut self, prompt: &str, thread_index: usize, cx: &mut Context<Self>) -> bool {
        let mut pieces = prompt.splitn(2, char::is_whitespace);
        let Some(first) = pieces.next() else { return false };
        if !first.eq_ignore_ascii_case("/goal") {
            return false;
        }
        let argument = pieces.next().unwrap_or("").trim();
        let response: String;

        if argument.is_empty() || argument.eq_ignore_ascii_case("status") {
            if let Some(goal) = &self.threads[thread_index].goal {
                let budget = goal
                    .token_budget
                    .map(|value| format!(" of {}", crate::theme::format_number(value)))
                    .unwrap_or_default();
                response = format!(
                    "Goal: **{}**\n\nStatus: {} · {}{} tokens used.",
                    goal.objective,
                    goal.status.raw_value(),
                    crate::theme::format_number(goal.used_tokens),
                    budget
                );
            } else {
                response = "No goal is active. Use `/goal Write the objective here` or `/goal --budget 100000 Write the objective here`.".into();
            }
        } else if argument.eq_ignore_ascii_case("clear") {
            self.threads[thread_index].goal = None;
            response = "Goal cleared.".into();
        } else if argument.eq_ignore_ascii_case("complete") {
            if let Some(goal) = &mut self.threads[thread_index].goal {
                goal.status = GoalStatus::Complete;
                response = format!("Goal marked complete: **{}**", goal.objective);
            } else {
                response = "No active goal to complete.".into();
            }
        } else {
            let mut objective = argument.to_string();
            let mut budget: Option<i64> = None;
            let parts: Vec<&str> = argument.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "--budget" {
                if let Ok(parsed) = parts[1].parse::<i64>() {
                    budget = Some(parsed);
                    objective = parts[2..].join(" ");
                }
            }
            if objective.is_empty() {
                self.threads[thread_index].messages.push(ChatMessage::new(
                    MessageRole::Assistant,
                    "Add an objective after `/goal`.",
                ));
                return true;
            }
            self.threads[thread_index].goal = Some(AgentGoal::new(objective.clone(), budget));
            response = match budget {
                Some(value) => format!(
                    "Goal pinned with a {} token budget: **{}**",
                    crate::theme::format_number(value),
                    objective
                ),
                None => format!("Goal pinned: **{objective}**"),
            };
        }

        self.threads[thread_index]
            .messages
            .push(ChatMessage::new(MessageRole::Assistant, response));
        self.activity = "Ready".into();
        cx.notify();
        true
    }

    // MARK: - Settings & management markers

    fn apply_mutation(&mut self, mutation: SettingsMutation, cx: &mut Context<Self>) {
        match mutation {
            SettingsMutation::FastMode(value) => self.preferences.fast_mode = value,
            SettingsMutation::FullAccess(value) => self.preferences.full_access = value,
            SettingsMutation::AutoCompact(value) => self.preferences.auto_compact = value,
            SettingsMutation::ShowTokenUsage(value) => self.preferences.show_token_usage = value,
            SettingsMutation::Effort(effort) => self.set_effort(effort, cx),
        }
        self.persist_preferences();
    }

    fn apply_management(&mut self, action: CosManagementAction, cx: &mut Context<Self>) {
        let result: Result<(), String> = (|| {
            match action {
                CosManagementAction::CreateSkill {
                    id,
                    name,
                    description,
                    instructions,
                    plugin_id,
                } => {
                    self.create_managed_skill(&id, &name, &description, &instructions, plugin_id.as_deref())?;
                    self.activity = "Skill created".into();
                }
                CosManagementAction::RemoveSkill { id, plugin_id } => {
                    self.remove_managed_skill(&id, plugin_id.as_deref())?;
                    self.activity = "Skill moved to Trash".into();
                }
                CosManagementAction::CreatePlugin {
                    id,
                    name,
                    description,
                    instructions,
                } => {
                    self.create_managed_plugin(&id, &name, &description, instructions.as_deref())?;
                    self.activity = "Plugin created".into();
                }
                CosManagementAction::RemovePlugin { id } => {
                    self.remove_managed_plugin(&id)?;
                    self.activity = "Plugin moved to Trash".into();
                }
                CosManagementAction::SetPluginEnabled { id, enabled } => {
                    self.set_plugin_enabled(&id, enabled)?;
                    self.activity = if enabled { "Plugin enabled".into() } else { "Plugin disabled".into() };
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.reload_plugins(cx),
            Err(error) => {
                self.last_error = Some(format!("Cos could not manage that skill or plugin: {error}"));
                self.activity = "Needs attention".into();
                cx.notify();
            }
        }
    }

    fn managed_plugins_root(&self) -> PathBuf {
        cos_core::application_support_dir().join("Cos/Plugins")
    }

    fn create_managed_skill(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
        instructions: &str,
        plugin_id: Option<&str>,
    ) -> Result<(), String> {
        validate_managed_id(id)?;
        validate_managed_text(name, 100)?;
        validate_managed_text(description, 500)?;
        validate_managed_text(instructions, 64_000)?;
        let owner_id = plugin_id.unwrap_or("codes.ssh.cos.user-skills").to_string();
        validate_managed_id(&owner_id)?;
        if owner_id == "codes.ssh.cos.settings" {
            return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
        }

        let plugin_root = self.managed_plugins_root().join(&owner_id);
        std::fs::create_dir_all(&plugin_root).map_err(|e| e.to_string())?;
        let manifest_url = plugin_root.join("cos.plugin.json");
        let mut manifest: CosPluginManifest = if manifest_url.exists() {
            serde_json::from_slice(&std::fs::read(&manifest_url).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
        } else if plugin_id.is_none() {
            CosPluginManifest {
                schema_version: 1,
                id: owner_id.clone(),
                name: "My Cos Skills".into(),
                version: "1.0.0".into(),
                author: full_user_name(),
                description: "Skills created and managed through the built-in Cos plugin.".into(),
                capabilities: vec![PluginCapability {
                    id: "cos.skills.user".into(),
                    description: "User-authored Cos skills.".into(),
                    risk: "guarded".into(),
                }],
                skills: Vec::new(),
                homepage: None,
                built_in: Some(false),
            }
        } else {
            return Err(format!("Plugin {owner_id} was not found in Cos-managed storage."));
        };

        let skill_root = plugin_root.join(format!("skills/{id}"));
        std::fs::create_dir_all(&skill_root).map_err(|e| e.to_string())?;
        let safe_description = description.replace('\n', " ").replace('"', "'");
        let markdown = format!(
            "---\nname: {id}\ndescription: \"{safe_description}\"\n---\n\n# {name}\n\n{instructions}\n"
        );
        write_atomic(&skill_root.join("SKILL.md"), markdown.as_bytes())?;
        if !manifest.skills.contains(&id.to_string()) {
            manifest.skills.push(id.to_string());
        }
        manifest.skills.sort();
        self.write_managed_manifest(&manifest, &manifest_url)
    }

    fn remove_managed_skill(&mut self, id: &str, plugin_id: Option<&str>) -> Result<(), String> {
        validate_managed_id(id)?;
        let owner_id = plugin_id.unwrap_or("codes.ssh.cos.user-skills").to_string();
        validate_managed_id(&owner_id)?;
        if owner_id == "codes.ssh.cos.settings" {
            return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
        }
        let plugin_root = self.managed_plugins_root().join(&owner_id);
        let manifest_url = plugin_root.join("cos.plugin.json");
        if !manifest_url.exists() {
            return Err(format!("Plugin {owner_id} was not found in Cos-managed storage."));
        }
        let skill_root = plugin_root.join(format!("skills/{id}"));
        if !skill_root.exists() {
            return Err(format!("Skill {id} was not found in that plugin."));
        }
        trash_item(&skill_root)?;
        let mut manifest: CosPluginManifest =
            serde_json::from_slice(&std::fs::read(&manifest_url).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        manifest.skills.retain(|skill| skill != id);
        self.write_managed_manifest(&manifest, &manifest_url)?;
        self.disabled_skill_keys.remove(&skill_key(id, &owner_id));
        self.persist_disabled_skills();
        Ok(())
    }

    fn create_managed_plugin(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
        instructions: Option<&str>,
    ) -> Result<(), String> {
        validate_managed_id(id)?;
        if id == "codes.ssh.cos.settings" {
            return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
        }
        validate_managed_text(name, 100)?;
        validate_managed_text(description, 500)?;
        if let Some(instructions) = instructions {
            validate_managed_text(instructions, 64_000)?;
        }
        let root = self.managed_plugins_root().join(id);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let has_instructions = instructions
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let manifest = CosPluginManifest {
            schema_version: 1,
            id: id.to_string(),
            name: name.to_string(),
            version: "1.0.0".into(),
            author: full_user_name(),
            description: description.to_string(),
            capabilities: vec![PluginCapability {
                id: format!("{id}.managed"),
                description: "Plugin created through Cos self-management.".into(),
                risk: "guarded".into(),
            }],
            skills: if has_instructions { vec!["main".into()] } else { Vec::new() },
            homepage: None,
            built_in: Some(false),
        };
        self.write_managed_manifest(&manifest, &root.join("cos.plugin.json"))?;
        if has_instructions {
            if let Some(instructions) = instructions {
                self.create_managed_skill("main", name, description, instructions, Some(id))?;
            }
        }
        Ok(())
    }

    fn remove_managed_plugin(&mut self, id: &str) -> Result<(), String> {
        validate_managed_id(id)?;
        if id == "codes.ssh.cos.settings" {
            return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
        }
        let root = self.managed_plugins_root().join(id);
        if !root.exists() {
            return Err(format!("Plugin {id} was not found in Cos-managed storage."));
        }
        trash_item(&root)?;
        self.disabled_plugin_ids.remove(id);
        self.disabled_skill_keys
            .retain(|key| !key.starts_with(&format!("{id}:")));
        self.persist_disabled_plugins();
        self.persist_disabled_skills();
        Ok(())
    }

    fn set_plugin_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        validate_managed_id(id)?;
        if id == "codes.ssh.cos.settings" {
            return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
        }
        if enabled {
            self.disabled_plugin_ids.remove(id);
        } else {
            self.disabled_plugin_ids.insert(id.to_string());
        }
        self.persist_disabled_plugins();
        Ok(())
    }

    pub fn set_plugin(&mut self, plugin: &InstalledPlugin, enabled: bool, cx: &mut Context<Self>) {
        match self.set_plugin_enabled(plugin.id(), enabled) {
            Ok(()) => {
                if enabled && plugin.id() == "codes.ssh.cos.computer-use" {
                    self.request_computer_use_access(cx);
                }
                if !enabled && plugin.id() == "codes.ssh.cos.computer-use" {
                    self.pending_computer_use_run = None;
                }
                if !enabled && plugin.id() == "codes.ssh.cos.betterwright" {
                    self.is_browser_panel_presented = false;
                }
                self.reload_plugins(cx);
            }
            Err(error) => {
                self.last_error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn remove_plugin(&mut self, plugin: &InstalledPlugin, cx: &mut Context<Self>) {
        let result = (|| -> Result<(), String> {
            if plugin.manifest.built_in == Some(true) {
                return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
            }
            trash_item(&plugin.location)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.disabled_plugin_ids.remove(plugin.id());
                self.disabled_skill_keys
                    .retain(|key| !key.starts_with(&format!("{}:", plugin.id())));
                self.persist_disabled_plugins();
                self.persist_disabled_skills();
                self.activity = "Plugin moved to Trash".into();
                self.reload_plugins(cx);
            }
            Err(error) => {
                self.last_error = Some(error);
                cx.notify();
            }
        }
    }

    pub fn is_skill_enabled(&self, skill: &str, plugin: &InstalledPlugin) -> bool {
        !self.disabled_skill_keys.contains(&skill_key(skill, plugin.id()))
    }

    pub fn set_skill(&mut self, skill: &str, plugin: &InstalledPlugin, enabled: bool, cx: &mut Context<Self>) {
        let key = skill_key(skill, plugin.id());
        if enabled {
            self.disabled_skill_keys.remove(&key);
        } else {
            self.disabled_skill_keys.insert(key);
        }
        self.persist_disabled_skills();
        self.activity = if enabled { "Skill enabled".into() } else { "Skill disabled".into() };
        cx.notify();
    }

    pub fn remove_skill(&mut self, skill: &str, plugin: &InstalledPlugin, cx: &mut Context<Self>) {
        let result = (|| -> Result<(), String> {
            if plugin.manifest.built_in == Some(true) {
                return Err("The built-in Cos plugin cannot be disabled, removed, or overwritten.".into());
            }
            validate_managed_id(skill)?;
            let manifest_url = plugin.location.join("cos.plugin.json");
            let mut manifest: CosPluginManifest =
                serde_json::from_slice(&std::fs::read(&manifest_url).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if !manifest.skills.contains(&skill.to_string()) {
                return Err(format!("Skill {skill} was not found in that plugin."));
            }
            let candidates = [
                plugin.location.join(format!("skills/{skill}")),
                plugin.location.join(skill),
            ];
            let Some(skill_root) = candidates.iter().find(|candidate| candidate.exists()) else {
                return Err(format!("Skill {skill} was not found in that plugin."));
            };
            trash_item(skill_root)?;
            manifest.skills.retain(|item| item != skill);
            self.write_managed_manifest(&manifest, &manifest_url)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.disabled_skill_keys.remove(&skill_key(skill, plugin.id()));
                self.persist_disabled_skills();
                self.activity = "Skill moved to Trash".into();
                self.reload_plugins(cx);
            }
            Err(error) => {
                self.last_error = Some(error);
                cx.notify();
            }
        }
    }

    fn write_managed_manifest(&self, manifest: &CosPluginManifest, url: &Path) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
        write_atomic(url, &data)
    }

    fn persist_disabled_plugins(&self) {
        let values: Vec<String> = {
            let mut values: Vec<String> = self.disabled_plugin_ids.iter().cloned().collect();
            values.sort();
            values
        };
        crate::prefs::save("disabledPluginIDs", &values);
    }

    fn persist_disabled_skills(&self) {
        let values: Vec<String> = {
            let mut values: Vec<String> = self.disabled_skill_keys.iter().cloned().collect();
            values.sort();
            values
        };
        crate::prefs::save("disabledSkillKeys", &values);
    }

    pub fn persist_preferences(&self) {
        crate::prefs::save("preferences", &self.preferences);
    }

    pub fn is_workspace_trusted(&self, path: &str) -> bool {
        self.trusted_workspaces.contains(&normalize_workspace_path(path))
    }

    fn active_extension_instructions(&self) -> String {
        let mut sections: Vec<String> = Vec::new();
        let mut remaining: usize = 48_000;
        for plugin in self.plugins.iter().filter(|plugin| plugin.is_enabled) {
            let capability_summary = plugin
                .manifest
                .capabilities
                .iter()
                .map(|capability| format!("{}: {}", capability.id, capability.description))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "Plugin {} — {}\n{}",
                plugin.manifest.id, plugin.manifest.description, capability_summary
            ));
            for skill in &plugin.manifest.skills {
                if remaining == 0 || !self.is_skill_enabled(skill, plugin) {
                    continue;
                }
                let candidates = [
                    plugin.location.join(format!("skills/{skill}/SKILL.md")),
                    plugin.location.join(format!("{skill}/SKILL.md")),
                ];
                let Some(url) = candidates.iter().find(|candidate| candidate.exists()) else {
                    continue;
                };
                let Ok(data) = std::fs::read(url) else { continue };
                let slice_len = data.len().min(remaining);
                sections.push(format!(
                    "Skill {}:{}\n{}",
                    plugin.manifest.id,
                    skill,
                    String::from_utf8_lossy(&data[..slice_len])
                ));
                remaining -= slice_len;
            }
        }
        sections.join("\n\n")
    }

    // MARK: - Catalog & credentials

    pub fn save_catalog(&self) {
        crate::prefs::save("providers", &self.providers);
        crate::prefs::save("models", &self.models);
    }

    pub fn reset_catalog(&mut self, cx: &mut Context<Self>) {
        self.providers = DefaultCatalog::providers();
        self.models = DefaultCatalog::models();
        self.save_catalog();
        cx.notify();
    }

    pub fn add_provider(
        &mut self,
        name: &str,
        base_url: url::Url,
        model_name: &str,
        model_id: &str,
        api_key: &str,
    ) -> Result<(), String> {
        let slug = format!("custom-{}", Uuid::new_v4().to_string().to_lowercase());
        let account = format!("{slug}-key");
        self.secure_store
            .set(api_key, &account)
            .map_err(|error| error.to_string())?;
        self.providers.push(ProviderProfile {
            id: slug.clone(),
            name: name.to_string(),
            bridge: ProviderBridge::OpenAICompatible,
            auth_mode: AuthenticationMode::ApiKey,
            base_url: Some(base_url),
            keychain_account: Some(account),
            executable: None,
            is_enabled: true,
        });
        self.models.push(ModelProfile::new(
            &format!("{slug}:{model_id}"),
            &slug,
            if model_name.is_empty() { model_id } else { model_name },
            model_id,
            200_000,
            true,
            true,
            ReasoningEffort::ALL.to_vec(),
        ));
        self.save_catalog();
        Ok(())
    }

    pub fn set_api_key(&mut self, value: &str, provider: &ProviderProfile) -> Result<(), String> {
        let Some(account) = provider.keychain_account.clone() else { return Ok(()) };
        self.secure_store
            .set(value, &account)
            .map_err(|error| error.to_string())?;
        self.login_status
            .insert(provider.id.clone(), "Key stored in this Mac’s Keychain".into());
        Ok(())
    }

    pub fn has_api_key(&self, provider: &ProviderProfile) -> bool {
        provider
            .keychain_account
            .as_ref()
            .and_then(|account| self.secure_store.get(account).ok().flatten())
            .is_some()
    }

    pub fn sign_in(&mut self, provider: &ProviderProfile, cx: &mut Context<Self>) {
        let command: Vec<String> = match provider.bridge {
            ProviderBridge::Codex => vec![provider.executable.clone().unwrap_or_else(|| "codex".into()), "login".into()],
            ProviderBridge::Claude => vec![
                provider.executable.clone().unwrap_or_else(|| "claude".into()),
                "auth".into(),
                "login".into(),
            ],
            ProviderBridge::OpenCode => {
                if provider.id == "xai" {
                    vec![
                        provider.executable.clone().unwrap_or_else(|| "opencode".into()),
                        "auth".into(),
                        "login".into(),
                        "--provider".into(),
                        "xai".into(),
                    ]
                } else {
                    vec![
                        provider.executable.clone().unwrap_or_else(|| "opencode".into()),
                        "auth".into(),
                        "login".into(),
                    ]
                }
            }
            _ => {
                self.login_status
                    .insert(provider.id.clone(), "This provider uses an API key.".into());
                cx.notify();
                return;
            }
        };
        let shell_command = command
            .iter()
            .map(|part| shell_quoted(part))
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            apple_script_quoted(&shell_command)
        );
        let spawned = std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg("tell application \"Terminal\" to activate")
            .arg("-e")
            .arg(&script)
            .spawn();
        match spawned {
            Ok(_) => {
                self.login_status.insert(
                    provider.id.clone(),
                    if provider.id == "xai" {
                        "Continue the SuperGrok / X Premium sign-in in Terminal".into()
                    } else {
                        "Continue sign-in in Terminal".into()
                    },
                );
                self.monitor_provider_sign_in(provider.clone(), cx);
            }
            Err(error) => {
                self.login_status
                    .insert(provider.id.clone(), format!("Could not open Terminal: {error}"));
            }
        }
        cx.notify();
    }

    pub fn refresh_provider_sessions(&mut self) {
        let mut sessions = HashMap::new();
        for provider in self
            .providers
            .iter()
            .filter(|provider| provider.auth_mode == AuthenticationMode::Subscription)
        {
            if let Some(session) = self.runtime.session_info(provider) {
                sessions.insert(provider.id.clone(), session);
            }
        }
        self.provider_sessions = sessions;
    }

    fn monitor_provider_sign_in(&mut self, provider: ProviderProfile, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            for _ in 0..90 {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let alive = this
                    .update(cx, |model, cx| {
                        model.refresh_provider_sessions();
                        let done = model
                            .provider_sessions
                            .get(&provider.id)
                            .map(|session| {
                                model.login_status.insert(
                                    provider.id.clone(),
                                    format!("Signed in as {}", session.display_name()),
                                );
                            })
                            .is_some();
                        cx.notify();
                        !done
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    // MARK: - Plugins

    pub fn reload_plugins(&mut self, cx: &mut Context<Self>) {
        let workspace = self
            .selected_thread()
            .map(|thread| PathBuf::from(&thread.workspace_path));
        let mut discovered = PluginRegistry::discover(Some(&self.built_in_plugins_url), workspace.as_deref());
        for plugin in discovered.iter_mut() {
            plugin.is_enabled =
                plugin.id() == "codes.ssh.cos.settings" || !self.disabled_plugin_ids.contains(plugin.id());
        }
        self.plugins = discovered;
        self.refresh_computer_use_access(cx);
        cx.notify();
    }

    pub fn load_marketplace(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.is_loading_marketplace {
            return;
        }
        if !force && !self.marketplace_plugins.is_empty() {
            return;
        }
        self.is_loading_marketplace = true;
        self.marketplace_error = None;
        cx.notify();
        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let result = fetch_marketplace(force).await;
            let _ = sender.send(result);
        });
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let result = receiver.await;
            let _ = this.update(cx, |model, cx| {
                model.is_loading_marketplace = false;
                match result {
                    Ok(Ok(mut items)) => {
                        items.sort_by(|lhs, rhs| {
                            (rhs.featured == Some(true))
                                .cmp(&(lhs.featured == Some(true)))
                                .then_with(|| lhs.name.to_lowercase().cmp(&rhs.name.to_lowercase()))
                        });
                        model.marketplace_plugins = items;
                    }
                    Ok(Err(error)) => model.marketplace_error = Some(error),
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn install_marketplace_plugin(&mut self, listing: &CosMarketplaceListing, cx: &mut Context<Self>) {
        if listing.kind != "plugin" || self.installing_marketplace_plugin_id.is_some() {
            return;
        }
        if listing.built_in == Some(true) {
            if listing.id == "codes.ssh.cos.computer-use" {
                self.request_computer_use_access(cx);
            }
            if !self.plugins.iter().any(|plugin| plugin.id() == listing.id) {
                self.last_error = Some(format!(
                    "{} is included with the latest Cos build. Install the current Cos update, then reopen Plugins & Skills.",
                    listing.name
                ));
                cx.notify();
            }
            return;
        }
        self.installing_marketplace_plugin_id = Some(listing.id.clone());
        cx.notify();
        let listing = listing.clone();
        let listing_fetch = listing.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let result = fetch_marketplace_manifest(&listing_fetch).await;
            let _ = sender.send(result);
        });
        cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let result = receiver.await;
            let _ = this.update(cx, |model, cx| {
                let listing_name = listing.name.clone();
                let install = match result {
                    Ok(Ok(manifest)) => model.install_marketplace_manifest(&listing, manifest),
                    Ok(Err(error)) => Err(error),
                    Err(_) => Ok(()),
                };
                if let Err(error) = install {
                    model.last_error = Some(format!("Could not install {listing_name}: {error}"));
                }
                model.installing_marketplace_plugin_id = None;
                cx.notify();
            });
        })
        .detach();
    }

    fn install_marketplace_manifest(
        &mut self,
        listing: &CosMarketplaceListing,
        manifest: CosPluginManifest,
    ) -> Result<(), String> {
        static ID_VALID: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-z0-9][a-z0-9._-]{1,63}$").unwrap());
        if manifest.schema_version != 1 || manifest.id != listing.id || !ID_VALID.is_match(&manifest.id) {
            return Err("The marketplace plugin manifest is invalid or does not match its listing.".into());
        }
        let target = self.managed_plugins_root().join(&manifest.id);
        std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        self.write_managed_manifest(&manifest, &target.join("cos.plugin.json"))?;
        self.disabled_plugin_ids.remove(&manifest.id);
        self.persist_disabled_plugins();
        self.activity = format!("{} installed", manifest.name);
        Ok(())
    }

    pub fn reload_after_marketplace_install(&mut self, cx: &mut Context<Self>) {
        self.reload_plugins(cx);
    }

    pub fn install_plugin_from_disk(&mut self, manifest_url: &Path, cx: &mut Context<Self>) {
        let result = (|| -> Result<(), String> {
            if manifest_url.file_name().and_then(|name| name.to_str()) != Some("cos.plugin.json") {
                return Err("Choose a file named cos.plugin.json.".into());
            }
            let manifest: CosPluginManifest =
                serde_json::from_slice(&std::fs::read(manifest_url).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let plugins_root = cos_core::application_support_dir().join("Cos/Plugins");
            std::fs::create_dir_all(&plugins_root).map_err(|e| e.to_string())?;
            let target = plugins_root.join(&manifest.id);
            if target.exists() {
                std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
            }
            let source_dir = manifest_url.parent().ok_or("Invalid manifest path.")?;
            copy_directory(source_dir, &target).map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                let is_cu = serde_json::from_slice::<CosPluginManifest>(
                    &std::fs::read(manifest_url).unwrap_or_default(),
                )
                .map(|manifest| manifest.id == "codes.ssh.cos.computer-use")
                .unwrap_or(false);
                if is_cu {
                    self.request_computer_use_access(cx);
                }
                self.reload_plugins(cx);
            }
            Err(error) => {
                self.last_error = Some(format!("Could not install the plugin: {error}"));
                cx.notify();
            }
        }
    }

    // MARK: - Skill import

    pub fn refresh_skill_import_counts(&mut self) {
        for source in ExternalSkillSource::ALL {
            if source == ExternalSkillSource::Folder {
                continue;
            }
            let count = discover_skill_directories(&source.default_roots()).len();
            self.skill_import_counts.insert(source, count);
        }
    }

    pub fn import_skills(&mut self, source: ExternalSkillSource, cx: &mut Context<Self>) {
        if source == ExternalSkillSource::Folder {
            // Folder selection happens in the view; it calls import_skills_from.
            return;
        }
        self.import_skills_from(source.default_roots(), source, cx);
    }

    pub fn import_skills_from(&mut self, roots: Vec<PathBuf>, source: ExternalSkillSource, cx: &mut Context<Self>) {
        self.skill_import_status.insert(source, "Importing…".into());
        cx.notify();
        let plugins_root = self.managed_plugins_root();
        let result = perform_skill_import(&roots, source, &plugins_root);
        match result {
            Ok((imported, skipped)) => {
                let status = if imported == 0 {
                    "No compatible skills found".to_string()
                } else {
                    format!(
                        "Imported {imported} skill{}{}",
                        if imported == 1 { "" } else { "s" },
                        if skipped > 0 { format!(" · {skipped} skipped") } else { String::new() }
                    )
                };
                self.skill_import_status.insert(source, status);
                self.activity = if imported == 0 { "No skills imported".into() } else { "Skills imported".into() };
                self.refresh_skill_import_counts();
                self.reload_plugins(cx);
            }
            Err(error) => {
                self.skill_import_status.insert(source, "Import failed".into());
                self.last_error = Some(format!("Could not import skills: {error}"));
                self.activity = "Needs attention".into();
                cx.notify();
            }
        }
    }

    // MARK: - Computer Use access

    pub fn refresh_computer_use_access(&mut self, cx: &mut Context<Self>) {
        self.computer_use_access_granted = CosComputerUseAccess::is_granted();
        if self.computer_use_access_granted {
            if self.computer_use_access_status.as_deref() != Some("Accessibility access granted") {
                self.computer_use_access_status = Some("Accessibility access granted".into());
            }
            self.resume_pending_computer_use_run(cx);
        }
        cx.notify();
    }

    pub fn request_computer_use_access(&mut self, cx: &mut Context<Self>) {
        if CosComputerUseAccess::is_granted() {
            self.refresh_computer_use_access(cx);
            return;
        }
        self.computer_use_access_status = Some("Use the macOS prompt to allow Cos in Accessibility.".into());
        let _ = CosComputerUseAccess::request();
        cx.notify();
    }

    pub fn open_accessibility_settings(&mut self) {
        let _ = std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }

    fn resume_pending_computer_use_run(&mut self, cx: &mut Context<Self>) {
        if !self.computer_use_access_granted || self.is_running {
            return;
        }
        let Some(pending) = self.pending_computer_use_run.clone() else { return };
        if self.selected_thread_id != Some(pending.thread_id) {
            return;
        }
        self.pending_computer_use_run = None;
        self.start_run(&pending.prompt, false, cx);
    }

    // MARK: - Title generation

    fn schedule_title_generation(&mut self, thread_id: Uuid, prompt: String, cx: &mut Context<Self>) {
        let Some(model) = self.selected_title_model() else { return };
        let Some(provider) = self
            .providers
            .iter()
            .find(|provider| provider.id == model.provider_id)
            .cloned()
        else {
            return;
        };
        let workspace = self
            .threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| thread.workspace_path.clone())
            .unwrap_or_else(|| self.preferences.default_workspace.clone());
        let clipped: String = prompt.chars().take(2_000).collect();
        let title_prompt = format!(
            "Write a specific 3–7 word task title for this user request. Use plain title case text only: no quotes, no markdown, no period, and no prefix such as “Title:”.\n\nUser request:\n{clipped}"
        );
        let mut title_thread = CosThread::new(&workspace, &model.id);
        title_thread.id = thread_id;
        title_thread.effort = ReasoningEffort::Low;
        let mut request = AgentRequest::new(
            title_prompt.clone(),
            Some(title_prompt),
            title_thread,
            model,
            provider,
            ReasoningEffort::Low,
            false,
            false,
        );
        request.workspace_is_trusted = true;
        request.tools_enabled = false;

        let (sender, receiver) = futures::channel::oneshot::channel();
        tokio_runtime().spawn(async move {
            let runtime = AgentRuntime::default();
            let collected = async {
                let mut stream = runtime.stream(request).ok()?;
                let mut output = String::new();
                while let Some(event) = stream.next().await {
                    if let Ok(AgentEvent::TextDelta(text)) = event {
                        output.push_str(&text);
                    }
                }
                Some(output)
            };
            let _ = sender.send(collected.await);
        });
        let task = cx.spawn(async move |this: WeakEntity<AppModel>, cx: &mut AsyncApp| {
            let output = receiver.await.ok().flatten();
            let _ = this.update(cx, |model, cx| {
                model.title_tasks.remove(&thread_id);
                let cleaned = output.and_then(|text| clean_generated_title(&text));
                if let Some(title) = cleaned {
                    if let Some(index) = model.threads.iter().position(|thread| thread.id == thread_id) {
                        model.threads[index].title = title;
                        model.threads[index].updated_at = time::OffsetDateTime::now_utc();
                        let thread = model.threads[index].clone();
                        model.persist(&thread);
                        cx.notify();
                        return;
                    }
                }
                if let Some(index) = model.threads.iter().position(|thread| thread.id == thread_id) {
                    if model.threads[index].title == "New task" {
                        model.threads[index].title = fallback_title(&prompt);
                        let thread = model.threads[index].clone();
                        model.persist(&thread);
                        cx.notify();
                    }
                }
            });
        });
        self.title_tasks.insert(thread_id, task);
    }

    // MARK: - Persistence

    pub fn persist(&self, thread: &CosThread) {
        let store = self.store.clone();
        let thread = thread.clone();
        tokio_runtime().spawn_blocking(move || {
            let _ = store.upsert(&thread);
        });
    }

    fn normalize_loaded_thread_efforts(&mut self) {
        for thread in self.threads.iter_mut() {
            if let Some(profile) = self.models.iter().find(|model| model.id == thread.model_id) {
                thread.effort = profile.normalized_effort(thread.effort);
            }
        }
        if let Some(profile) = self
            .models
            .iter()
            .find(|model| model.id == self.preferences.selected_model_id)
        {
            let normalized = profile.normalized_effort(self.preferences.default_effort);
            self.preferences.default_effort = normalized;
        }
    }
}

// MARK: - Free helpers

pub fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn skill_key(skill: &str, plugin_id: &str) -> String {
    format!("{plugin_id}:{skill}")
}

fn normalize_workspace_path(path: &str) -> String {
    cos_core::canonical_path(Path::new(path)).to_string_lossy().into_owned()
}

pub fn looks_like_computer_use_request(prompt: &str) -> bool {
    let value = prompt.to_lowercase();
    if value.contains("@computer") || value.contains("computer use") {
        return true;
    }
    if value.contains("@betterwright") || value.contains("/browser") {
        return false;
    }
    let actions = ["open ", "click ", "type ", "send ", "log in", "login", "navigate ", "go to "];
    let destinations = [" app", "safari", "chrome", "chat "];
    actions.iter().any(|action| value.contains(action))
        && destinations.iter().any(|destination| value.contains(destination))
}

pub fn clean_generated_title(raw: &str) -> Option<String> {
    static PREFIX: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^title\s*:\s*").unwrap());
    static WHITESPACE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\s+").unwrap());
    let first = raw.lines().next().unwrap_or("").trim();
    let title = PREFIX.replace(first, "");
    let title = title.trim_matches(|c: char| "\"'`*_#–—-. ".contains(c));
    let title = WHITESPACE.replace_all(title, " ");
    if title.chars().count() < 3 {
        return None;
    }
    let clipped: String = title.chars().take(54).collect();
    let clipped = clipped.trim();
    (!clipped.is_empty()).then(|| clipped.to_string())
}

pub fn fallback_title(prompt: &str) -> String {
    static WHITESPACE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\s+").unwrap());
    let one_line = WHITESPACE.replace_all(prompt.trim(), " ");
    one_line.chars().take(54).collect()
}

fn validate_managed_id(id: &str) -> Result<(), String> {
    static VALID: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-z0-9][a-z0-9._-]*$").unwrap());
    if !(2..=64).contains(&id.chars().count()) || !VALID.is_match(id) {
        return Err(
            "Use a 2–64 character lowercase ID made from letters, numbers, dots, underscores, or hyphens.".into(),
        );
    }
    Ok(())
}

fn validate_managed_text(text: &str, maximum: usize) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > maximum {
        return Err("Names and instructions must be non-empty and within Cos’s size limits.".into());
    }
    Ok(())
}

fn write_atomic(url: &Path, data: &[u8]) -> Result<(), String> {
    let temporary = url.with_extension("cos-tmp");
    std::fs::write(&temporary, data).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, url).map_err(|e| e.to_string())
}

pub fn trash_item(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let manager = unsafe { NSFileManager::defaultManager() };
    let url = unsafe { NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())) };
    unsafe {
        manager
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|error| error.localizedDescription().to_string())?;
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_directory(&path, &destination)?;
        } else {
            std::fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

fn full_user_name() -> String {
    let name = unsafe { objc2_foundation::NSFullUserName() };
    let name = name.to_string();
    if name.is_empty() {
        "Cos user".to_string()
    } else {
        name
    }
}

fn shell_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn apple_script_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

async fn fetch_marketplace(force: bool) -> Result<Vec<CosMarketplaceListing>, String> {
    let client = reqwest_client();
    let request = client
        .get("https://cos.ssh.codes/api/plugins")
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(15));
    let request = if force {
        request.header("Cache-Control", "no-cache")
    } else {
        request
    };
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err("The Cos marketplace returned an invalid response.".into());
    }
    let data = response.bytes().await.map_err(|e| e.to_string())?;
    if data.len() > 2_000_000 {
        return Err("The Cos marketplace returned an invalid response.".into());
    }
    let catalog: CosMarketplaceResponse = serde_json::from_slice(&data)
        .map_err(|_| "The Cos marketplace returned an invalid response.".to_string())?;
    Ok(catalog.items)
}

async fn fetch_marketplace_manifest(
    listing: &CosMarketplaceListing,
) -> Result<CosPluginManifest, String> {
    if let Some(manifest) = &listing.manifest {
        return Ok(manifest.clone());
    }
    let encoded: String = url::form_urlencoded::byte_serialize(listing.id.as_bytes()).collect();
    let url = format!("https://cos.ssh.codes/api/plugins/{encoded}/manifest");
    let response = reqwest_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err("The Cos marketplace returned an invalid response.".into());
    }
    let data = response.bytes().await.map_err(|e| e.to_string())?;
    if data.len() > 256_000 {
        return Err("The Cos marketplace returned an invalid response.".into());
    }
    serde_json::from_slice(&data).map_err(|_| "The marketplace plugin manifest is invalid or does not match its listing.".into())
}

fn reqwest_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new())
        })
        .clone()
}

pub fn discover_skill_directories(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: HashMap<String, PathBuf> = HashMap::new();
    for root in roots {
        let resolved_root = cos_core::canonical_path(root);
        let Ok(metadata) = std::fs::metadata(&resolved_root) else { continue };
        if !metadata.is_dir() {
            continue;
        }
        let direct_manifest = resolved_root.join("SKILL.md");
        if direct_manifest.exists() {
            found.insert(resolved_root.to_string_lossy().into_owned(), resolved_root.clone());
        }
        let mut stack = vec![resolved_root.clone()];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name_str) = name.to_str() else { continue };
                if name_str.starts_with('.') {
                    continue;
                }
                if ["node_modules", ".git", ".build"].contains(&name_str) {
                    continue;
                }
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    stack.push(path);
                    continue;
                }
                if name_str == "SKILL.md" {
                    if let Some(parent) = path.parent() {
                        let directory = cos_core::canonical_path(parent);
                        found.insert(directory.to_string_lossy().into_owned(), directory);
                    }
                }
            }
        }
    }
    let mut values: Vec<PathBuf> = found.into_values().collect();
    values.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    values
}

fn perform_skill_import(
    roots: &[PathBuf],
    source: ExternalSkillSource,
    plugins_root: &Path,
) -> Result<(usize, usize), String> {
    let directories = discover_skill_directories(roots);
    if directories.is_empty() {
        return Ok((0, 0));
    }
    let plugin_root = plugins_root.join(source.plugin_id());
    let skills_root = plugin_root.join("skills");
    std::fs::create_dir_all(&skills_root).map_err(|e| e.to_string())?;

    let mut imported: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for directory in &directories {
        let Some(id) = imported_skill_id(directory) else {
            skipped += 1;
            continue;
        };
        if imported.contains(&id) {
            skipped += 1;
            continue;
        }
        match copy_imported_skill(directory, &skills_root.join(&id)) {
            Ok(()) => imported.push(id),
            Err(_) => skipped += 1,
        }
    }

    let manifest = CosPluginManifest {
        schema_version: 1,
        id: source.plugin_id(),
        name: format!("Imported from {}", source.title()),
        version: "1.0.0".into(),
        author: "Cos Importer".into(),
        description: format!(
            "Portable skills imported locally from {}. Original source folders are left unchanged.",
            source.title()
        ),
        capabilities: vec![PluginCapability {
            id: "cos.skills.import".into(),
            description: "Read-only import of portable SKILL.md bundles selected by the user.".into(),
            risk: "safe".into(),
        }],
        skills: {
            let mut skills = imported.clone();
            skills.sort();
            skills
        },
        homepage: None,
        built_in: Some(false),
    };
    let data = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    write_atomic(&plugin_root.join("cos.plugin.json"), &data)?;
    Ok((imported.len(), skipped))
}

fn imported_skill_id(directory: &Path) -> Option<String> {
    let manifest = directory.join("SKILL.md");
    let data = std::fs::read(&manifest).ok()?;
    if data.len() > 1_000_000 {
        return None;
    }
    static NAME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"(?m)^name:\s*["']?([^"'\n]+)"#).unwrap());
    static NON_SLUG: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"[^a-z0-9._-]+").unwrap());
    let text = String::from_utf8_lossy(&data[..data.len().min(64_000)]);
    let raw = NAME
        .captures(&text)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .unwrap_or_else(|| {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string()
        });
    let normalized = NON_SLUG
        .replace_all(&raw.to_lowercase(), "-")
        .trim_matches(|c| c == '-' || c == '.' || c == '_')
        .to_string();
    let first = normalized.chars().next()?;
    if !(2..=64).contains(&normalized.chars().count()) || !(first.is_ascii_alphanumeric()) {
        return None;
    }
    Some(normalized)
}

fn copy_imported_skill(source: &Path, target: &Path) -> Result<(), String> {
    let staging = target
        .parent()
        .ok_or("Invalid target")?
        .join(format!(".import-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut total_bytes = 0usize;
        let mut file_count = 0usize;
        let mut stack = vec![source.to_path_buf()];
        while let Some(directory) = stack.pop() {
            let entries = std::fs::read_dir(&directory)
                .map_err(|_| "The selected folder does not contain a readable SKILL.md bundle.".to_string())?;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name_str) = name.to_str() else { continue };
                if name_str.starts_with('.') || ["node_modules", ".git", ".build"].contains(&name_str) {
                    continue;
                }
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_symlink() {
                    continue;
                }
                let relative = path
                    .strip_prefix(source)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .trim_matches('/')
                    .to_string();
                if relative.is_empty() || relative.contains("../") {
                    continue;
                }
                let destination = staging.join(&relative);
                if kind.is_dir() {
                    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
                } else if kind.is_file() {
                    let size = entry.metadata().map_err(|e| e.to_string())?.len() as usize;
                    file_count += 1;
                    total_bytes += size;
                    if file_count > 1_000 || total_bytes > 10_000_000 || size > 2_000_000 {
                        return Err("A skill exceeded Cos’s 10 MB or 1,000-file import limit.".into());
                    }
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    std::fs::copy(&path, &destination).map_err(|e| e.to_string())?;
                }
            }
        }
        if !staging.join("SKILL.md").exists() {
            return Err("The selected folder does not contain a readable SKILL.md bundle.".into());
        }
        if target.exists() {
            trash_item(target)?;
        }
        std::fs::rename(&staging, target).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn merge_providers(saved: Option<Vec<ProviderProfile>>) -> Vec<ProviderProfile> {
    let mut result = saved.unwrap_or_default();
    for item in DefaultCatalog::providers() {
        if let Some(existing) = result.iter_mut().find(|existing| existing.id == item.id) {
            existing.bridge = item.bridge;
            existing.auth_mode = item.auth_mode;
            existing.base_url = item.base_url;
            existing.keychain_account = item.keychain_account;
            existing.executable = item.executable;
        } else {
            result.push(item);
        }
    }
    result
}

fn merge_models(saved: Option<Vec<ModelProfile>>) -> Vec<ModelProfile> {
    let mut result: Vec<ModelProfile> = saved
        .unwrap_or_default()
        .into_iter()
        .filter(|model| model.id != "anthropic:claude-5")
        .collect();
    for item in DefaultCatalog::models() {
        if let Some(existing) = result.iter_mut().find(|existing| existing.id == item.id) {
            existing.provider_id = item.provider_id;
            existing.name = item.name;
            existing.model = item.model;
            existing.context_window = item.context_window;
            existing.supports_images = item.supports_images;
            existing.supports_tools = item.supports_tools;
            existing.supported_efforts = item.supported_efforts;
        } else {
            result.push(item);
        }
    }
    result
}
