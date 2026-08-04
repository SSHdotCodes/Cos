//! Built-in plugins and provider logos, embedded into the binary and
//! materialized into a Cos-owned cache directory at startup so plugin
//! discovery and skill loading work exactly like the Swift resource bundle.

use std::path::{Path, PathBuf};

const FILES: &[(&str, &str)] = &[
    (
        "betterwright/cos.plugin.json",
        include_str!("../../../../Sources/Cos/Resources/BuiltInPlugins/betterwright/cos.plugin.json"),
    ),
    (
        "betterwright/skills/browser/SKILL.md",
        include_str!("../../../../Sources/Cos/Resources/BuiltInPlugins/betterwright/skills/browser/SKILL.md"),
    ),
    (
        "computer-use/cos.plugin.json",
        include_str!("../../../../Sources/Cos/Resources/BuiltInPlugins/computer-use/cos.plugin.json"),
    ),
    (
        "computer-use/skills/computer-use/SKILL.md",
        include_str!("../../../../Sources/Cos/Resources/BuiltInPlugins/computer-use/skills/computer-use/SKILL.md"),
    ),
    (
        "computer-use/skills/computer-use/agents/openai.yaml",
        include_str!("../../../../Sources/Cos/Resources/BuiltInPlugins/computer-use/skills/computer-use/agents/openai.yaml"),
    ),
    (
        "cos/cos.plugin.json",
        include_str!("../../../../Sources/Cos/Resources/BuiltInPlugins/cos/cos.plugin.json"),
    ),
];

const LOGOS: &[(&str, &str)] = &[
    ("openai.svg", include_str!("../../../../Sources/Cos/Resources/ProviderLogos/openai.svg")),
    ("claude.svg", include_str!("../../../../Sources/Cos/Resources/ProviderLogos/claude.svg")),
    ("grok.svg", include_str!("../../../../Sources/Cos/Resources/ProviderLogos/grok.svg")),
    ("opencode.svg", include_str!("../../../../Sources/Cos/Resources/ProviderLogos/opencode.svg")),
    ("pi.svg", include_str!("../../../../Sources/Cos/Resources/ProviderLogos/pi.svg")),
    ("qwen.svg", include_str!("../../../../Sources/Cos/Resources/ProviderLogos/qwen.svg")),
];

/// Materialize embedded assets, refreshing files whose content changed.
pub fn materialize() {
    let root = built_in_plugins_root();
    for (relative, contents) in FILES {
        write_if_changed(&root.join(relative), contents);
    }
    let logos = provider_logos_root();
    for (name, contents) in LOGOS {
        write_if_changed(&logos.join(name), contents);
    }
}

pub fn built_in_plugins_root() -> PathBuf {
    cos_core::application_support_dir().join("Cos/BuiltInPlugins")
}

pub fn provider_logos_root() -> PathBuf {
    cos_core::application_support_dir().join("Cos/ProviderLogos")
}

fn write_if_changed(path: &Path, contents: &str) {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == contents {
            return;
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, contents);
}
