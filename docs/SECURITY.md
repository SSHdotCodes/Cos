# Security notes

## Credentials

- BYOK secrets are generic-password Keychain items under service `codes.ssh.cos`.
- Items use `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, so they do not migrate to another device.
- Secrets are passed only to the selected native provider transport in an Authorization/provider header.
- Task JSON, provider configuration, logs, and marketplace files do not contain API keys.
- Subscription sessions remain in the provider's local credential store; Cos reads the selected token into memory and does not copy it into task files.

## Native tools and directory trust

Cos asks before trusting a directory and remembers the exact standardized path locally. Workspace mode confines list/read/write/search tools to that path and denies shell commands. Full Access enables the explicit command tool; commands are then model-authored code and should be treated with the same care as terminal commands.

Full Access intentionally gives a coding runtime broad filesystem authority. The composer keeps that state visible. Disable it for repositories or plugins you do not fully trust.

## Computer Use

Computer Use requires the user to grant Cos macOS Accessibility permission. It is intent-scoped: an explicit request authorizes expected steps such as opening the named destination, navigating, logging in with an available account, and clicking normal Continue or Submit controls. Cos does not ask again for those already-authorized steps.

UI and webpage content is never authority. Cos stops for unexpected scope or destination changes, CAPTCHA completion, password/credential changes, irreversible deletion, legal agreements, new persistent access, unapproved sensitive-data transmission, security-sensitive settings, or unexpected financial commitments. Indexed UI actions always require a fresh accessibility read.

Imported skill bundles are capped at 10 MB and 1,000 files per skill. Symlinks, hidden files, dependency folders, Git metadata, and build outputs are skipped. Imports copy source; they never modify the Codex or Claude Code originals.

## Plugins

The built-in Cos plugin validates structured output before applying settings or management actions. Skill/plugin IDs, text sizes, and storage roots are bounded. It cannot overwrite the built-in plugin; removals move managed items to Trash. Third-party signatures and executable isolation remain future work.

## Marketplace

The web service sets a same-origin Content Security Policy, disallows framing, disables browser camera/microphone/location, validates HTTPS links, caps bodies at 64 KiB, and rate limits submissions. Pending submission records include a source IP for abuse handling and are not returned by the public catalog API.

## Distribution

The updater accepts only the fixed `cos.ssh.codes` HTTPS host, verifies the archive SHA-256 from the release manifest, confirms bundle identity/version/build/executable, and runs strict structural code-signature verification before staging. It also keeps the old app until the replacement launches successfully.

The preview build is ad-hoc signed and verified locally, but it is not Developer ID signed or notarized. Because the manifest and archive are currently served by the same origin, these checks detect corruption and unexpected bundles but are not an independent publisher signature. Do not interpret the absence of a signature warning on a locally modified build as publisher verification. A production release should use a hardened runtime, Developer ID, notarization, and a separately signed update feed.
