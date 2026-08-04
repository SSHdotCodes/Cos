//! CosCore — the Rust port of Cos's native agent runtime.
//!
//! Keeps orchestration, extensions, tools, provider streaming, and bounded
//! context in one small runtime, byte-compatible with the Swift implementation
//! (thread snapshots, preferences, plugin manifests, and Keychain service).

pub mod betterwright;
pub(crate) mod cf;
pub mod compaction;
pub mod composer_references;
pub mod computer_use;
pub mod error;
pub mod harness;
pub mod models;
pub mod plugins;
pub mod run_control;
pub mod runtime;
pub mod secure_store;
pub mod thread_store;
pub mod update;

pub use betterwright::{BetterWrightCommandResult, BetterWrightRuntimeError, CosBetterWrightRuntime};
pub use compaction::{CompactionEngine, CompactionResult};
pub use composer_references::{
    plugin_handle, ComposerReferenceKind, ComposerReferenceQuery, ComposerReferenceResolver,
    ComposerReferenceSuggestion,
};
pub use computer_use::{CosComputerUseAccess, CosComputerUseRuntime};
pub use error::AgentRuntimeError;
pub use harness::{AgentEventStream, CosHarness, CosToolCall, SubagentRunner};
pub use models::*;
pub use plugins::{
    CosManagementAction, CosMarketplaceListing, CosMarketplaceResponse, CosPluginManifest,
    CosSettingsPlugin, InstalledPlugin, PluginCapability, PluginRegistry, SettingsMutation,
    SettingsPluginExtraction,
};
pub use run_control::{AgentRunControl, SteeringMessage};
pub use runtime::{AgentCredential, AgentRuntime, ProviderSessionInfo};
pub use secure_store::SecureStore;
pub use thread_store::ThreadStore;
pub use update::{CosUpdateError, CosUpdateManifest, CosUpdateService, PreparedCosUpdate};

use std::path::{Path, PathBuf};

pub fn application_support_dir() -> PathBuf {
    models::dirs_home().join("Library/Application Support")
}

pub fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Resources directory of the enclosing .app bundle, when running from one.
pub fn bundle_resources_dir() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let macos_dir = executable.parent()?;
    if macos_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos_dir.parent()?;
    let resources = contents.join("Resources");
    resources.exists().then_some(resources)
}

/// The running .app bundle URL, when launched from one.
pub fn bundle_url() -> Option<PathBuf> {
    bundle_resources_dir()?.parent()?.parent().map(Path::to_path_buf)
}
