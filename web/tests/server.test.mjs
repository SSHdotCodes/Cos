import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";

const port = 39_271;
const child = spawn(process.execPath, ["server.mjs"], {
  cwd: new URL("..", import.meta.url),
  env: { ...process.env, PORT: String(port), PUBLIC_ORIGIN: `http://127.0.0.1:${port}`, DATA_DIR: `/tmp/cos-web-test-${process.pid}` },
  stdio: ["ignore", "pipe", "pipe"],
});

await new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error("server did not start")), 5_000);
  child.stdout.once("data", () => { clearTimeout(timer); resolve(); });
});
test.after(() => child.kill("SIGTERM"));

test("health and catalog endpoints", async () => {
  const health = await fetch(`http://127.0.0.1:${port}/api/health`).then((response) => response.json());
  assert.equal(health.ok, true);
  assert.equal(health.version, "1.0.1");
  const catalog = await fetch(`http://127.0.0.1:${port}/api/plugins`).then((response) => response.json());
  assert.ok(catalog.items.length >= 2);
});

test("serves a no-cache update manifest", async () => {
  const response = await fetch(`http://127.0.0.1:${port}/api/update`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("cache-control"), "no-store");
  const update = await response.json();
  assert.equal(update.version, "1.0.1");
  assert.equal(update.build, 103);
  assert.match(update.downloadURL, /^https:\/\/cos\.ssh\.codes\/downloads\//);
  assert.match(update.sha256, /^[a-f0-9]{64}$/);
});

test("rejects invalid submissions", async () => {
  const response = await fetch(`http://127.0.0.1:${port}/api/plugins/submit`, {
    method: "POST", headers: { "Content-Type": "application/json", Origin: `http://127.0.0.1:${port}` }, body: JSON.stringify({ id: "no" }),
  });
  assert.equal(response.status, 400);
});
