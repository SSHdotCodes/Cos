# Providers and authentication

Cos separates a provider profile from a model profile. Adding a custom OpenAI-compatible provider automatically adds its model to the main selector.

| Provider | Native transport | Authentication | Secret handling |
|---|---|---|---|
| ChatGPT Plus / Pro | ChatGPT Codex Responses SSE | visible login helper | Local session read in memory |
| Claude Pro / Max | Anthropic Messages SSE | visible login helper or key | Local session/Keychain |
| X Premium / SuperGrok | xAI OpenAI-compatible SSE | OpenCode xAI OAuth helper | Local session read in memory |
| OpenCode Go | OpenAI-compatible SSE | API key | Cos Keychain |
| Qwen Token Plan | DashScope-compatible SSE | API key | Cos Keychain |
| Smart route | Cos selects an available native transport | connected provider | Same as routed provider |
| OpenAI-compatible | `chat/completions` SSE | API key | Cos Keychain |

Provider login helpers are authentication bootstrap only. Cos reads the resulting token locally, keeps it in memory for the request, and executes the complete agent loop itself.

## Full Access

The orange composer badge is authority, not a visual preference. Workspace mode confines native read/write tools to the trusted workspace and denies shell commands. Full Access permits the native command tool and absolute paths.

The exact permission model is provider-specific. Always review a runtime's own prompt and logs when using third-party plugins or an unfamiliar workspace.

## Fast Mode

Fast Mode requests the provider's priority tier where supported. Its state is shown by the lightning bolt in the fixed-width model chip, so the composer never shifts.

## Catalog caveat

The bundled model names follow the requested 2026 catalog. A provider can rename, gate, or remove a model independently of Cos. Custom providers/models are the escape hatch when a catalog changes before the app does.
