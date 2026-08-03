# Cos architecture

## Design target

Cos keeps the resident app small. SwiftUI owns windows and interaction state; the native Swift `CosHarness` owns provider streaming and tool orchestration; Security owns secrets. There is no embedded browser or resident third-party agent runtime.

## Runtime flow

```text
Composer
  → AppModel validates slash commands and saves the user message
  → CompactionEngine builds the prompt checkpoint
  → AgentRuntime resolves a local credential and directory trust
  → CosHarness runs one bounded provider-neutral tool loop
      → ChatGPT Responses / Anthropic Messages / OpenAI-compatible SSE
      → workspace file, search, patch, Full Access command, and native macOS accessibility tools
  → AgentEvent stream updates the answer and structured work trace
  → ThreadStore atomically persists the finished snapshot
```

The harness permits at most 24 tool steps, bounds response and tool-result buffers, truncates its in-run transcript, and streams provider bytes instead of retaining full network responses. Login CLIs can bootstrap subscription credentials, but no external agent CLI runs the task.

Enabled plugin skill files are injected into a bounded extension context. The model can call one native Cos tool at a time; meaningful statuses, reasoning summaries, and every tool invocation become work-trace events that auto-collapse after completion. Redundant harness-start bookkeeping is not shown.

Computer Use enumerates a fresh macOS accessibility tree for every indexed action and deliberately retains no stale element handles. The newest user-authored request is injected separately as its authority boundary; ordinary steps inside that request continue without another prompt, while third-party UI content cannot expand the scope.

The skill importer copies bounded portable bundles into a dedicated local plugin, preserving `SKILL.md`, scripts, references, and assets while skipping symlinks and build/dependency directories. Codex and Claude Code source folders remain unchanged. The plugin window also loads the moderated `cos.ssh.codes` catalog directly, supports search, and installs validated matching manifests into Cos-managed storage.

## Persistence

Task snapshots live in:

```text
~/Library/Application Support/Cos/Threads/<UUID>.json
```

`ThreadStore` is an actor, performs atomic writes, and sorts tasks by last update. Credentials are never included in these snapshots.

Preferences and the provider/model catalog use namespaced `UserDefaults`. Secrets use the Keychain service `codes.ssh.cos`.

## Compaction

Cos estimates prompt tokens without loading a tokenizer into the UI process. At the configured context percentage it:

1. preserves the configured recent token window verbatim;
2. adds clipped, role-labelled older events to a bounded checkpoint;
3. carries forward the previous checkpoint;
4. keeps the full transcript visible in the UI.

This is deliberately deterministic and inexpensive. A later plugin can replace checkpoint summarization with a provider model without changing the thread format.

## Goals

Goals contain an objective, status, optional token budget, usage, and creation date. `/goal` is handled locally before an agent run. The active goal is injected into every effective prompt and survives compaction.

## Appearance

System, Light, and Dark follow standard SwiftUI color schemes. True Dark also replaces material-backed composer/sidebar surfaces, the window toolbar, and both main/settings backgrounds with pure black. A narrow AppKit window bridge is used only for titlebar transparency and background color.

## Marketplace server

`web/server.mjs` uses only Node's standard library. It serves static assets and downloads, exposes health/catalog/plugin-manifest/update endpoints, and accepts rate-limited plugin submissions. Mutable submissions are written as JSON Lines in `DATA_DIR`, which is outside the deployed application directory so releases cannot erase them.

## Updates

Cos checks the HTTPS release manifest at launch and every six hours without retaining a network process. A newer version or build reveals one small sidebar action. The update downloads to a temporary directory, is bounded to 250 MB, must match the release SHA-256, and must contain a correctly versioned `codes.ssh.cos` bundle whose code-signing structure verifies. A detached helper swaps the bundle only after Cos exits, relaunches it, and restores the previous bundle if the replacement cannot start. Updates require Cos to run from a writable installed location rather than a disk image.
