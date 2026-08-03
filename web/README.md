# Cos website

The download site and moderated plugin/skill marketplace for Cos. It uses only Node's standard library so the server stays small and fast.

```sh
PORT=3000 DATA_DIR=/tmp/cos-web-data node server.mjs
```

Production is deployed with `rasppost` to `cos.ssh.codes`. Mutable submissions live in `DATA_DIR`, outside the deployed app directory.

`GET /api/update` serves the no-cache native app release manifest. Its SHA-256 must match the versioned ZIP in `public/downloads/` before deployment.
