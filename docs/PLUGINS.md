# Cos plugin format

A plugin is a directory containing `cos.plugin.json` plus optional skills and tool-bridge resources.

```json
{
  "schemaVersion": 1,
  "id": "com.example.my-plugin",
  "name": "My Cos Plugin",
  "version": "0.1.0",
  "author": "Your name",
  "description": "What the plugin adds.",
  "capabilities": [
    {
      "id": "example.read-project",
      "description": "Read files in the active workspace.",
      "risk": "safe"
    }
  ],
  "skills": ["my-skill"],
  "homepage": "https://example.com"
}
```

## Discovery order

1. bundled resources;
2. `~/Library/Application Support/Cos/Plugins`;
3. `<workspace>/.cos/plugins`.

The built-in `codes.ssh.cos.settings` plugin accepts only allowlisted settings plus guarded skill/plugin actions when the user explicitly asks. Cos can create skills, add them to managed plugins, create plugins, enable/disable them, and move managed skills/plugins to Trash. IDs and content are validated, writes stay under `~/Library/Application Support/Cos/Plugins`, and the built-in plugin cannot be changed or removed.

`codes.ssh.cos.computer-use` is a first-party bundled plugin. Computer Use exposes native accessibility tools with explicit intent scoping.

Settings → Import discovers portable `SKILL.md` bundles in `~/.codex/skills`, `~/.claude/skills`, or a folder the user selects. Imported bundles become ordinary local plugins and can be disabled or moved to Trash without touching their source.

## Install and publish

The Plugin Library can install a manifest directory selected from disk. Marketplace manifests can be downloaded from `https://cos.ssh.codes`.

The website submission form posts metadata and an HTTPS manifest URL to a moderated queue. A submission does not become public automatically. The server stores pending records outside the deployed source tree.

## Trust boundary

Version 0.2 parses and displays manifests but does not yet implement package signatures or a sandboxed executable plugin host. Treat third-party plugin resources as code: inspect them and grant only the capabilities you understand. Project-local manifests are not marked trusted automatically.
