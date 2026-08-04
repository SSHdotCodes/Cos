<p align="center">
  <img src="web/public/assets/cos-icon.png" width="96" height="96" alt="Cos logo">
</p>

<h1 align="center">Cos</h1>

<p align="center">
  A fast, native macOS workspace and provider-neutral harness for agentic coding.
</p>

<p align="center">
  <a href="https://github.com/SSHDotCodes/Cos/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/SSHDotCodes/Cos/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/SSHDotCodes/Cos/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/SSHDotCodes/Cos?display_name=tag"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-white.svg"></a>
  <img alt="macOS 15 or later" src="https://img.shields.io/badge/macOS-15%2B-black.svg">
</p>

<p align="center">
  <a href="https://cos.ssh.codes"><strong>Website</strong></a> ·
  <a href="https://github.com/SSHDotCodes/Cos/releases/latest"><strong>Download</strong></a> ·
  <a href="https://cos.ssh.codes/#marketplace"><strong>Plugin marketplace</strong></a> ·
  <a href="docs/ARCHITECTURE.md"><strong>Architecture</strong></a>
</p>

![Cos native macOS workspace](docs/images/cos-workspace.png)

Cos keeps the entire coding-agent loop in one lightweight Swift runtime: provider streaming, goals, compaction, tools, plugins, skills, credentials, and task history. BetterWright starts only for live web work, while every model and subagent stays inside the Cos harness.

## Why Cos

- **Native and lean.** SwiftUI, direct provider transports, macOS Keychain, bounded work traces, and no Electron runtime.
- **One harness for every model.** ChatGPT, Anthropic, xAI, Qwen, OpenCode Go, Pi, and custom OpenAI-compatible endpoints use the same Cos tool loop.
- **Long-running tasks that stay coherent.** Persistent `/goal` state and compacted checkpoints preserve intent while controlling context growth.
- **Steer without starting over.** Type new direction while a run is active and Cos immediately continues from the updated conversation.
- **Native subagents.** Explicitly delegate bounded work to any connected model at one of that model’s supported reasoning efforts; every child still runs through the Cos harness.
- **Model-aware controls.** Each model exposes only the reasoning levels and Fast Mode capability it supports.
- **A real extension system.** Import portable Codex and Claude Code skills, install from the in-app marketplace, or create capability-scoped Cos plugins.
- **Computer Use built in.** Accessibility-first Mac control follows explicit user intent without redundant stops while retaining safety boundaries for consequential actions.
- **A browser you can watch and control.** BetterWright powers persistent agentic browsing, while a local interactive pane shows the exact task session without keeping Chromium alive at idle.
- **Local credentials.** BYOK values are device-only Keychain items; subscription helpers use provider-owned local sessions.

## Interface

| Model-aware reasoning | In-app plugins and skills |
| --- | --- |
| ![Cos model and reasoning picker](docs/images/cos-model-picker.png) | ![Cos plugin marketplace](docs/images/cos-marketplace.png) |

The interface includes System, Light, Dark, and True Dark themes, an expandable work trace that collapses after the final response, quiet update notifications, native settings, directory trust prompts, and a compact Codex-style composer.

## Install

Cos 1.0.1 supports Apple silicon Macs running macOS 15 or later.

1. Download the [Cos 1.0.1 DMG](https://github.com/SSHDotCodes/Cos/releases/download/v1.0.1/Cos-1.0.1.dmg) or [ZIP](https://github.com/SSHDotCodes/Cos/releases/download/v1.0.1/Cos-1.0.1-macOS-arm64.zip).
2. Move `Cos.app` to Applications.
3. Open Cos and configure a provider in **Settings → Providers**.

The 1.0 community build is ad-hoc signed, not Apple-notarized. On first launch, macOS may require **System Settings → Privacy & Security → Open Anyway**. See [Security](docs/SECURITY.md) for the trust model and current distribution limitations.

## Goals

```text
/goal Ship a working release
/goal --budget 100000 Ship a working release
/goal status
/goal complete
/goal clear
```

Goals persist with the task and remain part of compacted agent context.

## Steering and subagents

While Cos is working, type a correction or added instruction and press Command–Return (or the send arrow) to steer the active task. Cos interrupts the current provider turn, preserves its work trace, and continues inside the same harness run with steering applied in order.

## Agentic browser

Enable the built-in BetterWright Browser plugin and click the Browser button in the task header (or press Shift–Command–B). Cos uses a persistent task-specific BetterWright session for `browser_run` tools and attaches the right-side pane to that same session. The viewer binds only to loopback; you can take control, switch tabs, or close the active tab from Cos. The managed browser is a one-time, on-demand download and is not resident when unused.

Use `/subagent` or choose **+ → Ask a subagent** to prefill an exact model and reasoning effort. Cos only offers locally connected providers, validates the model-specific effort allowlist, runs one child at a time, and returns child tools and status inside the parent work trace.

## Providers and credentials

Cos supports provider subscription helpers as well as BYOK. Availability of a model still depends on the connected provider account.

| Provider route | Credential source |
| --- | --- |
| ChatGPT | Local ChatGPT/Codex subscription session or OpenAI API key |
| Anthropic | Local Claude subscription session or Anthropic API key |
| xAI / Grok | Local xAI session or compatible API credential |
| OpenCode Go | Provider token |
| Qwen Token Plan | Provider token |
| Custom endpoint | Base URL, model ID, and Keychain-stored API key |

BYOK values are stored as generic-password items under the `codes.ssh.cos` Keychain service with device-only accessibility. Read the implementation and limitations in [Provider setup](docs/PROVIDERS.md) and [Security](docs/SECURITY.md).

## Build from source

Requirements: macOS 15+, Xcode/Swift 6.2 or later, and Node 20+ for the marketplace tests. Release packaging downloads a pinned portable Node 24.16.0 runtime and BetterWright 1.6.3; development builds can use a compatible global BetterWright installation.

```sh
git clone https://github.com/SSHDotCodes/Cos.git
cd Cos
swift test --scratch-path /tmp/cos-test-build
scripts/run_debug.sh
```

Create the same ad-hoc-signed release artifacts used by the public workflow:

```sh
scripts/build_release.sh
npm test --prefix web
```

Artifacts are written to `outputs/` and intentionally excluded from Git. Tagged releases publish the DMG, architecture-specific ZIP, source archive, and SHA-256 checksums through GitHub Actions.

## Repository map

```text
Sources/CosCore   Provider transports, harness, tools, compaction, updates
Sources/Cos       Native SwiftUI application and built-in plugins
Tests             Core behavior and update tests
web               Zero-dependency marketplace and download service
docs              Architecture, provider, plugin, and security notes
scripts           Debug and release packaging
```

## Plugins and skills

The built-in Cos plugin can manage allowlisted preferences and Cos-owned skills/plugins. Marketplace packages declare capabilities in a readable manifest before installation. Start with [Plugin development](docs/PLUGINS.md) or browse [cos.ssh.codes](https://cos.ssh.codes/#marketplace).

## Contributing

Issues and focused pull requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [security policy](SECURITY.md) before opening one. CI runs Swift tests on an Apple-silicon macOS runner and the marketplace suite on Linux.

## License

Cos is available under the [MIT License](LICENSE). Bundled third-party notices are in [THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES).
