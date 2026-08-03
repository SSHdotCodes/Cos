---
name: run-on-server
description: Deploy, upload, run forever, publish, and manage apps or services through configured rasppost/pi-server or rtxpost targets. Use only when the user explicitly asks Cos to put a project on a server, Pi, RTX/GPU machine, or ssh.codes; keep it running across disconnects or reboots; or manage an existing deployment.
---

# Run on Server

Use the locally configured `rasppost` command for lightweight Pi and `ssh.codes` services. Use `rtxpost` for GPU, CUDA, local-model, or explicitly requested RTX work.

## Guardrails

- Deploy only after an explicit user request to deploy, publish, upload, run, or manage a server project.
- Never delete or unpublish a project unless the user requested that exact outcome.
- Treat uploads as complete source-tree replacements. Never upload a partial directory over an existing project.
- Keep live databases, uploads, credentials, and other persistent data outside the deployed source directory.
- Use Full Access for server commands. If the configured command is unavailable, report the missing local dependency instead of inventing credentials or hosts.

## Existing Pi projects

Treat the server as canonical. Before inspecting or changing source:

1. Resolve the project with `rasppost list`.
2. Download a fresh binary-safe archive into a new temporary directory using the configured SSH/pi-server workflow.
3. Reapply only the intended local changes onto that fresh copy.
4. Immediately before the final upload or release, pull again into another clean directory and reconcile any newer server changes.
5. Never use local Git history as a substitute for the fresh pull.

If the remote advanced and the changes cannot be reconciled safely, stop and explain the conflict rather than overwriting either copy.

## Deploy

1. Inspect the complete synchronized app and determine its start command, port, dependencies, and persistence needs.
2. Run the target’s connection or test command.
3. Run local tests and builds relevant to the service.
4. Deploy a public Pi service with `rasppost release`, a private worker with `rasppost deploy --no-publish`, or an RTX service with `rtxpost release`/`deploy` as configured.
5. Install production dependencies before starting when required.
6. Verify status and logs. A durable Pi service must be running with autostart enabled and restart policy set to always; an RTX task must be running with a non-empty process ID.
7. For public services, request the HTTPS endpoint and verify the expected response.

Do not declare success until process state, logs, persistence behavior, and the public endpoint have been checked.
