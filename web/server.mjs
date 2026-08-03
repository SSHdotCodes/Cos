import { createServer } from "node:http";
import { readFile, mkdir, appendFile, stat } from "node:fs/promises";
import { createReadStream } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const publicRoot = join(root, "public");
const catalogPath = join(root, "data", "plugins.json");
const updatePath = join(root, "data", "update.json");
const dataRoot = process.env.DATA_DIR || join(root, ".data");
const host = process.env.HOST || "127.0.0.1";
const port = Number(process.env.PORT || 3000);
const publicOrigin = process.env.PUBLIC_ORIGIN || `http://${host}:${port}`;
const limits = new Map();

const mime = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".zip": "application/zip",
  ".dmg": "application/x-apple-diskimage",
};

const headers = {
  "Content-Security-Policy": "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=()",
  "Cross-Origin-Opener-Policy": "same-origin",
};

function json(response, statusCode, body, extra = {}) {
  response.writeHead(statusCode, { ...headers, ...extra, "Content-Type": "application/json; charset=utf-8" });
  response.end(JSON.stringify(body));
}

async function catalog() {
  return JSON.parse(await readFile(catalogPath, "utf8"));
}

function cleanText(value, maximum = 240) {
  return String(value || "").trim().slice(0, maximum);
}

function clientIP(request) {
  return cleanText(request.headers["cf-connecting-ip"] || request.socket.remoteAddress || "unknown", 80);
}

function canSubmit(ip) {
  const now = Date.now();
  const recent = (limits.get(ip) || []).filter((time) => now - time < 60 * 60 * 1000);
  if (recent.length >= 5) return false;
  recent.push(now);
  limits.set(ip, recent);
  return true;
}

async function bodyJSON(request) {
  let body = "";
  for await (const chunk of request) {
    body += chunk;
    if (body.length > 65_536) throw new Error("Submission is too large.");
  }
  return JSON.parse(body || "{}");
}

function validSubmission(input) {
  const item = {
    type: input.type === "skill" ? "skill" : "plugin",
    id: cleanText(input.id, 90).toLowerCase(),
    name: cleanText(input.name, 80),
    version: cleanText(input.version, 24),
    author: cleanText(input.author, 80),
    description: cleanText(input.description, 800),
    homepage: cleanText(input.homepage, 300),
    manifestURL: cleanText(input.manifestURL, 300),
    tags: Array.isArray(input.tags) ? input.tags.map((tag) => cleanText(tag, 30)).filter(Boolean).slice(0, 8) : [],
  };
  if (!/^[a-z0-9][a-z0-9._-]{2,89}$/.test(item.id)) throw new Error("Use a reverse-domain or slug ID with letters, numbers, dots, dashes, or underscores.");
  if (!item.name || !item.version || !item.author || item.description.length < 24) throw new Error("Name, version, author, and a useful description are required.");
  for (const value of [item.homepage, item.manifestURL].filter(Boolean)) {
    const url = new URL(value);
    if (url.protocol !== "https:") throw new Error("Links must use HTTPS.");
  }
  return item;
}

async function handleAPI(request, response, url) {
  if (request.method === "GET" && url.pathname === "/api/health") {
    return json(response, 200, { ok: true, service: "cos-marketplace", version: "1.0.0" });
  }
  if (request.method === "GET" && url.pathname === "/api/update") {
    const update = JSON.parse(await readFile(updatePath, "utf8"));
    return json(response, 200, update, { "Cache-Control": "no-store" });
  }
  if (request.method === "GET" && url.pathname === "/api/plugins") {
    const items = await catalog();
    return json(response, 200, { items, total: items.length });
  }
  const manifestMatch = url.pathname.match(/^\/api\/plugins\/([^/]+)\/manifest$/);
  if (request.method === "GET" && manifestMatch) {
    const id = decodeURIComponent(manifestMatch[1]);
    const item = (await catalog()).find((candidate) => candidate.id === id);
    if (!item) return json(response, 404, { error: "Plugin not found." });
    const manifest = item.manifest || {
      schemaVersion: 1, id: item.id, name: item.name, version: item.version,
      author: item.author, description: item.description, capabilities: [], skills: [],
      homepage: `${publicOrigin}/plugins/${encodeURIComponent(item.id)}`,
    };
    return json(response, 200, manifest, { "Content-Disposition": `attachment; filename="${item.id}.cos.plugin.json"` });
  }
  if (request.method === "POST" && url.pathname === "/api/plugins/submit") {
    const origin = request.headers.origin;
    if (origin && origin !== publicOrigin) return json(response, 403, { error: "Origin not allowed." });
    const ip = clientIP(request);
    if (!canSubmit(ip)) return json(response, 429, { error: "Too many submissions. Try again later." }, { "Retry-After": "3600" });
    try {
      const item = validSubmission(await bodyJSON(request));
      await mkdir(dataRoot, { recursive: true });
      const record = { ...item, status: "pending", submittedAt: new Date().toISOString(), sourceIP: ip };
      await appendFile(join(dataRoot, "submissions.jsonl"), JSON.stringify(record) + "\n", { encoding: "utf8", mode: 0o600 });
      return json(response, 202, { ok: true, status: "pending", message: "Submitted for review." });
    } catch (error) {
      return json(response, 400, { error: error.message || "Invalid submission." });
    }
  }
  return json(response, 404, { error: "Not found." });
}

async function serveStatic(response, pathname) {
  const requested = pathname === "/" || pathname.startsWith("/plugins/") || pathname === "/publish" ? "/index.html" : pathname;
  const relative = normalize(decodeURIComponent(requested)).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = join(publicRoot, relative);
  if (!filePath.startsWith(publicRoot)) return json(response, 403, { error: "Forbidden." });
  try {
    const fileStat = await stat(filePath);
    if (!fileStat.isFile()) throw new Error("not a file");
    response.writeHead(200, {
      ...headers,
      "Content-Type": mime[extname(filePath).toLowerCase()] || "application/octet-stream",
      "Content-Length": fileStat.size,
      "Cache-Control": extname(filePath) === ".html" ? "no-cache" : "public, max-age=3600",
    });
    createReadStream(filePath).pipe(response);
  } catch {
    json(response, 404, { error: "Not found." });
  }
}

await mkdir(dataRoot, { recursive: true });
createServer(async (request, response) => {
  try {
    const url = new URL(request.url || "/", publicOrigin);
    if (url.pathname.startsWith("/api/")) await handleAPI(request, response, url);
    else if (request.method === "GET" || request.method === "HEAD") await serveStatic(response, url.pathname);
    else json(response, 405, { error: "Method not allowed." }, { Allow: "GET, HEAD, POST" });
  } catch (error) {
    console.error(error);
    if (!response.headersSent) json(response, 500, { error: "Internal server error." });
  }
}).listen(port, host, () => console.log(`Cos marketplace listening at ${publicOrigin}`));
