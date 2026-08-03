let plugins = [];
let filter = "all";
const grid = document.querySelector("#plugin-grid");
const search = document.querySelector("#plugin-search");

function escapeHTML(value) {
  return String(value).replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]);
}

function render() {
  const query = search.value.trim().toLowerCase();
  const visible = plugins.filter((plugin) => {
    const matchesFilter = filter === "all" || plugin.type === filter;
    const haystack = [plugin.name, plugin.description, plugin.author, ...(plugin.tags || [])].join(" ").toLowerCase();
    return matchesFilter && (!query || haystack.includes(query));
  });
  if (!visible.length) {
    grid.innerHTML = '<p class="loading">No matching plugins yet. You could publish the first one.</p>';
    return;
  }
  grid.innerHTML = visible.map((plugin) => `
    <article class="plugin-card">
      <div class="plugin-top"><div class="plugin-icon">${escapeHTML(plugin.name.slice(0, 1))}</div><span class="plugin-badge">${escapeHTML(plugin.builtIn ? "Built in" : plugin.type)}</span></div>
      <h3>${escapeHTML(plugin.name)}</h3><div class="plugin-meta">${escapeHTML(plugin.author)} · v${escapeHTML(plugin.version)}</div>
      <p>${escapeHTML(plugin.description)}</p><div class="tag-list">${(plugin.tags || []).slice(0, 4).map((tag) => `<span>${escapeHTML(tag)}</span>`).join("")}</div>
      <div class="plugin-footer"><a href="/api/plugins/${encodeURIComponent(plugin.id)}/manifest">${plugin.builtIn ? "View manifest" : "Download manifest"} →</a><span>${escapeHTML(plugin.downloads || "New")}</span></div>
    </article>`).join("");
}

async function load() {
  try {
    const response = await fetch("/api/plugins");
    if (!response.ok) throw new Error("Library unavailable");
    plugins = (await response.json()).items;
    render();
  } catch {
    grid.innerHTML = '<p class="loading">The plugin library is temporarily unavailable.</p>';
  }
}

search.addEventListener("input", render);
document.querySelectorAll("[data-filter]").forEach((button) => button.addEventListener("click", () => {
  filter = button.dataset.filter;
  document.querySelectorAll("[data-filter]").forEach((item) => item.classList.toggle("active", item === button));
  render();
}));

const dialog = document.querySelector("#plugin-dialog");
document.querySelector("#open-submit").addEventListener("click", () => dialog.showModal());
document.querySelector("#copy-command").addEventListener("click", async (event) => {
  await navigator.clipboard.writeText("cos plugin validate ./my-plugin");
  event.currentTarget.textContent = "Copied";
  setTimeout(() => { event.currentTarget.textContent = "Copy"; }, 1400);
});

document.querySelector("#plugin-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const submit = form.querySelector('[type="submit"]');
  const status = document.querySelector("#form-status");
  const values = Object.fromEntries(new FormData(form));
  values.tags = String(values.tags || "").split(",").map((tag) => tag.trim()).filter(Boolean);
  submit.disabled = true; status.className = "form-status"; status.textContent = "Sending…";
  try {
    const response = await fetch("/api/plugins/submit", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(values) });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || "Could not submit.");
    status.textContent = "Submitted. It is now in the safety review queue.";
    form.reset();
  } catch (error) {
    status.className = "form-status error"; status.textContent = error.message;
  } finally { submit.disabled = false; }
});

load();
