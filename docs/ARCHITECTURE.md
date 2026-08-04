# Cos architecture

## Design target

Cos keeps the resident app small. GPUI owns windows and interaction state; the native Rust `CosHarness` (`cos-core`) owns provider streaming and tool orchestration; the macOS Keychain owns secrets. BetterWright and its managed browser are lazy processes used only for web tasks, never a replacement coding-agent harness.

## Runtime flow

```text
Composer
  → AppModel validates slash commands and saves the user message
  → CompactionEngine builds the prompt checkpoint
  → AgentRuntime resolves a local credential and directory trust
  → CosHarness runs one bounded provider-neutral tool loop
      → ChatGPT Responses / Anthropic Messages / OpenAI-compatible SSE
      → workspace file, search, patch, Full Access command, native macOS accessibility, and BetterWright browser tools
      → explicitly authorized, allowlisted Cos subagents with independently resolved model credentials and effort
  → AgentEvent stream updates the answer and structured work trace
  → ThreadStore atomically persists the finished snapshot
```

The harness permits at most 24 tool steps, bounds response and tool-result buffers, truncates its in-run transcript, and streams provider bytes instead of retaining full network responses. Login CLIs can bootstrap subscription credentials, but no external agent CLI runs the task.

Steering uses a per-run `AgentRunControl`: new instructions enter a bounded FIFO queue and interrupt only the currently installed provider turn. The same `CosHarness` loop appends the ordered steering transcript, preserves prior tool/work events, and continues from the updated prompt. Run, thread, and assistant IDs still guard UI completion so stale events cannot overwrite the active segment.

Subagents use the same `CosHarness`, workspace trust, Full Access boundary, extensions, and native tools as the parent. The runtime builds an allowlist from enabled models whose providers have a resolvable local credential. Model output can select only a stable allowlisted model ID and an effort that model declares. Child runs cannot create grandchildren, Computer Use is not inherited, BetterWright availability follows the enabled browser plugin, execution is sequential, and the parent caps delegation at six calls.

Enabled plugin skill files are injected into a bounded extension context. The model can call one native Cos tool at a time; meaningful statuses, reasoning summaries, and every tool invocation become work-trace events that auto-collapse after completion. Redundant harness-start bookkeeping is not shown.

Computer Use enumerates a fresh macOS accessibility tree for every indexed action and deliberately retains no stale element handles. The newest user-authored request is injected separately as its authority boundary; ordinary steps inside that request continue without another prompt, while third-party UI content cannot expand the scope.

BetterWright uses a pinned CLI and portable Node runtime bundled in release builds. Each Cos task maps to a bounded BetterWright session name under the dedicated `cos` profile. `browser_run` performs small Playwright action-and-observe steps against that persistent session. The in-app inspector starts a loopback-only interactive `betterwright view` for the same session, validates the returned URL, and loads it through a narrow `WKWebView` overlay bridge. Native controls can close the active tab or detach the viewer without destroying the remaining task session. The roughly 200 MB managed browser is installed once on demand and stays out of the resident app process.

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

System, Light, and Dark follow standard macOS color schemes. True Dark also replaces material-backed composer/sidebar surfaces, the window toolbar, and both main/settings backgrounds with pure black.

## Marketplace server

`web/server.mjs` uses only Node's standard library. It serves static assets and downloads, exposes health/catalog/plugin-manifest/update endpoints, and accepts rate-limited plugin submissions. Mutable submissions are written as JSON Lines in `DATA_DIR`, which is outside the deployed application directory so releases cannot erase them.

## Updates

Cos checks the HTTPS release manifest at launch and every six hours without retaining a network process. A newer version or build reveals one small sidebar action. The update downloads to a temporary directory, is bounded to 250 MB, must match the release SHA-256, and must contain a correctly versioned `codes.ssh.cos` bundle whose code-signing structure verifies. A detached helper swaps the bundle only after Cos exits, relaunches it, and restores the previous bundle if the replacement cannot start. Updates require Cos to run from a writable installed location rather than a disk image.
