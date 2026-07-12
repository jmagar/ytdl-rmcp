const { App } = window.McpExtApps;

const app = new App({ name: "YouTube search", version: "1.0.0" });
const isDemo = new URLSearchParams(window.location.search).has("demo");
const state = {
  playlistCandidates: [],
};

const ALLOWED_EXTERNAL_ORIGINS = new Set([
  "https://listen.plex.tv",
  "https://app.plex.tv",
]);

const demoResults = [
  {
    title: "Slow Pulp - Falling Apart Live",
    url: "https://www.youtube.com/watch?v=abc123",
    uploader: "Slow Pulp",
    duration: 215,
    thumbnail: "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
  },
  {
    title: "Slow Pulp - Idaho Live",
    url: "https://www.youtube.com/watch?v=def456",
    uploader: "Live Room",
    duration: 188,
    thumbnail: "https://i.ytimg.com/vi/jNQXAC9IVRw/hqdefault.jpg",
  },
];

const els = {
  results: document.querySelector("#results"),
  form: document.querySelector("#search-form"),
  query: document.querySelector("#query"),
  tabs: [...document.querySelectorAll("[data-tab]")],
  views: [...document.querySelectorAll("[data-view]")],
  statsLimit: document.querySelector("#stats-limit"),
  statsStatus: document.querySelector("#stats-status"),
  statsResults: document.querySelector("#stats-results"),
  playlistName: document.querySelector("#playlist-name"),
  playlistStatus: document.querySelector("#playlist-status"),
  playlistResults: document.querySelector("#playlist-results"),
  transfersStatus: document.querySelector("#transfers-status"),
  transfersResults: document.querySelector("#transfers-results"),
};

function formatDuration(seconds) {
  if (!seconds) return "unknown duration";
  const total = Math.round(seconds);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[char]);
}

function toolText(result, fallback) {
  return result?.content?.find((item) => item.type === "text")?.text || fallback;
}

function friendlyError(message) {
  const text = String(message || "");
  // Keep "tools/call failed:" literal for the search_app regression test and
  // for hosts that surface hidden widget callbacks as a plain 404 message.
  if (/hidden while code[_ ]?mode/i.test(text) ||
      /tools\/call failed:\s*404/i.test(text)) {
    return "This panel cannot reach its YouTube tools because the MCP host is in code mode, which hides direct widget callbacks.";
  }
  try {
    const inner = JSON.parse(text)?.error?.message;
    if (inner) return inner;
  } catch (_) {
    // Not a JSON envelope; fall through to the original text.
  }
  return text || "Action failed.";
}

function parseToolPayload(result, label) {
  if (result?.isError) {
    throw new Error(toolText(result, `${label} failed.`));
  }
  if (result?.structuredContent) {
    return result.structuredContent;
  }
  const text = result?.content?.find((item) => item.type === "text")?.text;
  if (!text) throw new Error(`${label} returned no readable payload.`);
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`${label} returned invalid JSON: ${error.message}`);
  }
}

async function callTool(name, args, label = name) {
  const response = await app.callServerTool({ name, arguments: args });
  return parseToolPayload(response, label);
}

function setStatus(view, message) {
  const target = document.querySelector(`#${view}-status`);
  if (target) target.textContent = message || "";
}

function showStatus(message) {
  els.results.innerHTML = `<div class="empty">${escapeHtml(message)}</div>`;
}

function prependStatus(message) {
  els.results.insertAdjacentHTML("afterbegin", `<div class="empty">${escapeHtml(message)}</div>`);
}

function showView(name) {
  for (const tab of els.tabs) {
    tab.setAttribute("aria-selected", String(tab.dataset.tab === name));
  }
  for (const view of els.views) {
    view.hidden = view.dataset.view !== name;
  }
}

function render(results) {
  if (!results || results.length === 0) {
    showStatus("No results.");
    return;
  }
  els.results.innerHTML = results.map((item, index) => `
    <article class="result" aria-selected="${index === 0}">
      <img class="thumb" src="${escapeHtml(item.thumbnail || "")}" alt="" />
      <div class="copy">
        <h2 class="title">${escapeHtml(item.title || "Untitled")}</h2>
        <div class="meta">${escapeHtml(item.uploader || "Unknown channel")} - ${formatDuration(item.duration)}</div>
        <div class="meta">${escapeHtml(item.url)}</div>
      </div>
      <div class="actions">
        <button type="button" data-action="probe" data-url="${escapeHtml(item.url)}">Probe</button>
        <button type="button" class="download" data-action="audio" data-url="${escapeHtml(item.url)}">Audio</button>
        <button type="button" class="download" data-action="video" data-url="${escapeHtml(item.url)}">Video</button>
      </div>
    </article>
  `).join("");
}

function renderPayload(result) {
  const payload = parseToolPayload(result, "Search");
  if (payload.query) els.query.value = payload.query;
  render(payload.results || []);
}

function actionToolCall(action, url) {
  const calls = {
    probe: { name: "youtube_probe", arguments: { urls: url } },
    audio: { name: "youtube_download", arguments: { urls: url, mode: "audio" } },
    video: { name: "youtube_download", arguments: { urls: url, mode: "video" } },
  };
  return calls[action];
}

function renderStats(payload) {
  const recent = payload.recent || [];
  els.statsResults.innerHTML = `
    <div class="row">
      <div class="copy">
        <h2 class="title">${escapeHtml(payload.total_downloads || 0)} download(s)</h2>
        <div class="meta">${escapeHtml(payload.total_files || 0)} file(s), ${escapeHtml(payload.total_size || "0 B")} total</div>
      </div>
    </div>
    ${recent.map((entry) => `
      <div class="row">
        <div class="copy">
          <h2 class="title">${escapeHtml(entry.items?.[0]?.title || "Untitled")}</h2>
          <div class="meta">${escapeHtml(entry.timestamp || "unknown time")} - ${escapeHtml(entry.total_size || "0 B")}</div>
        </div>
      </div>
    `).join("")}
  `;
}

async function loadStats() {
  setStatus("stats", "Loading stats...");
  const limit = Number(els.statsLimit.value || 10);
  const payload = await callTool("youtube_stats", {
    limit,
    response_format: "json",
  }, "Stats");
  renderStats(payload);
  setStatus("stats", `${payload.total_downloads || 0} download(s).`);
}

function selectedCandidateIds() {
  return [...els.playlistResults.querySelectorAll("input[data-candidate-id]:checked")]
    .map((input) => input.dataset.candidateId);
}

function renderPlaylistCandidates(payload) {
  const candidates = payload.candidates || [];
  if (!candidates.length) {
    els.playlistResults.innerHTML = `<div class="empty">No transferred audio candidates.</div>`;
    return;
  }
  els.playlistResults.innerHTML = candidates.map((candidate, index) => `
    <label class="row candidate">
      <input class="check" type="checkbox" data-candidate-id="${escapeHtml(candidate.candidate_id)}" ${index < 25 ? "checked" : ""} />
      <span class="copy">
        <span class="title">${escapeHtml(candidate.title || "Untitled")}</span>
        <span class="meta">${escapeHtml(candidate.uploader || "Unknown artist")} - ${escapeHtml(candidate.timestamp || "")}</span>
      </span>
    </label>
  `).join("");
}

function renderPlaylistResult(payload) {
  els.playlistResults.querySelector(".playlist-summary")?.remove();
  const errors = payload.errors || [];
  const missing = payload.missing || [];
  const detailRows = [
    errors.length ? `<div class="meta error">Errors: ${escapeHtml(errors.join("; "))}</div>` : "",
    missing.length ? `<div class="meta">Missing: ${escapeHtml(missing.map((track) => track.title || "Unknown track").join("; "))}</div>` : "",
  ].join("");
  const linkButtons = payload.plexamp_url && isAllowedExternalUrl(payload.plexamp_url)
    ? `<div class="actions">
        <button type="button" data-open-url="${escapeHtml(payload.plexamp_url)}">Open in Plexamp</button>
        <button type="button" data-copy-text="${escapeHtml(payload.plexamp_url)}">Copy link</button>
      </div>`
    : "";
  els.playlistResults.insertAdjacentHTML("afterbegin", `
    <div class="row playlist-summary">
      <div class="copy">
        <h2 class="title">${escapeHtml(payload.playlist || "Plex playlist")}</h2>
        <div class="meta">${escapeHtml(payload.matched || 0)} matched, ${escapeHtml(payload.added || 0)} added, ${escapeHtml(payload.already_present || 0)} already present, ${(payload.missing || []).length} missing</div>
        <div class="meta">${escapeHtml(payload.playback_link_status || "")}</div>
        ${detailRows}
      </div>
      ${linkButtons}
    </div>
  `);
}

async function loadPlaylistCandidates() {
  setStatus("playlist", "Loading candidates...");
  const payload = await callTool("youtube_plex_playlist", {
    action: "list_candidates",
    limit: 100,
    response_format: "json",
  }, "Playlist candidates");
  state.playlistCandidates = payload.candidates || [];
  renderPlaylistCandidates(payload);
  setStatus("playlist", `${state.playlistCandidates.length} candidate(s).`);
}

async function previewPlaylist() {
  const playlist = els.playlistName.value.trim() || undefined;
  const candidate_ids = selectedCandidateIds();
  if (!candidate_ids.length) {
    setStatus("playlist", "Select at least one candidate before previewing.");
    return;
  }
  setStatus("playlist", "Previewing Plex matches...");
  const payload = await callTool("youtube_plex_playlist", {
    action: "preview",
    playlist,
    candidate_ids,
    limit: 100,
    response_format: "json",
  }, "Playlist preview");
  renderPlaylistResult(payload);
  setStatus("playlist", (payload.errors || []).length ? "Preview finished with errors." : "Preview complete.");
}

async function applyPlaylist() {
  const playlist = els.playlistName.value.trim() || undefined;
  const candidate_ids = selectedCandidateIds();
  if (!candidate_ids.length) {
    setStatus("playlist", "Select at least one candidate before applying.");
    return;
  }
  setStatus("playlist", "Applying Plex playlist...");
  const payload = await callTool("youtube_plex_playlist", {
    action: "apply",
    playlist,
    candidate_ids,
    limit: 100,
    response_format: "json",
  }, "Playlist apply");
  renderPlaylistResult(payload);
  setStatus("playlist", (payload.errors || []).length ? "Apply finished with errors." : "Apply complete. Review the summary.");
}

function renderTransfers(payload) {
  const entries = payload.entries || [];
  if (!entries.length) {
    els.transfersResults.innerHTML = `<div class="empty">No queued transfers.</div>`;
    return;
  }
  els.transfersResults.innerHTML = entries.map((entry) => `
    <div class="row">
      <div class="copy">
        <h2 class="title">${escapeHtml(entry.manifest_id)}</h2>
        <div class="meta">${escapeHtml(entry.status)} - ${escapeHtml(entry.staging_path)}</div>
        <div class="meta">${escapeHtml(entry.last_error || "")}</div>
      </div>
      <div class="actions">
        <button type="button" data-retry-transfer="${escapeHtml(entry.manifest_id)}">Retry</button>
      </div>
    </div>
  `).join("");
}

async function loadTransfers() {
  setStatus("transfers", "Loading transfer queue...");
  const payload = await callTool("youtube_transfer_queue", {
    action: "list",
    response_format: "json",
  }, "Transfer queue");
  renderTransfers(payload);
  setStatus("transfers", `${(payload.entries || []).length} queued transfer(s).`);
}

async function retryTransfer(manifest_id) {
  setStatus("transfers", "Retrying transfer...");
  const payload = await callTool("youtube_transfer_queue", {
    action: "retry",
    manifest_id,
    response_format: "json",
  }, "Transfer retry");
  await loadTransfers();
  renderTransferRetryStatus(payload);
}

async function retryAllTransfers() {
  setStatus("transfers", "Retrying all transfers...");
  const payload = await callTool("youtube_transfer_queue", {
    action: "retry_all",
    response_format: "json",
  }, "Transfer retry all");
  await loadTransfers();
  renderTransferRetryStatus(payload);
}

function renderTransferRetryStatus(payload) {
  const errors = payload.errors || [];
  if (payload.failed) {
    setStatus("transfers", `Retry failed for ${payload.failed} transfer(s): ${errors.join("; ") || "see queued item details"}`);
  } else {
    setStatus("transfers", `Retry complete: ${payload.completed || 0} transfer(s) completed.`);
  }
}

async function pruneTransfers() {
  setStatus("transfers", "Pruning missing staging dirs...");
  await callTool("youtube_transfer_queue", {
    action: "prune",
    response_format: "json",
  }, "Transfer prune");
  await loadTransfers();
}

function isAllowedExternalUrl(url) {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" && ALLOWED_EXTERNAL_ORIGINS.has(parsed.origin);
  } catch {
    return false;
  }
}

async function openAllowedExternal(url) {
  if (!isAllowedExternalUrl(url)) {
    throw new Error("Blocked external link destination.");
  }
  if (typeof app.openLink === "function") {
    return app.openLink({ url });
  }
  window.open(url, "_blank", "noopener,noreferrer");
  return true;
}

async function copyText(text) {
  if (!text) return false;
  if (navigator.clipboard && typeof navigator.clipboard.writeText === "function") {
    await navigator.clipboard.writeText(text);
    return true;
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  return ok;
}

function showConnectionError() {
  if (!isDemo) {
    showStatus("Could not connect to the MCP host.");
  }
}

app.ontoolinput = (params) => {
  const query = params?.arguments?.query ?? params?.query;
  if (query) els.query.value = query;
};

app.ontoolresult = (result) => {
  renderPayload(result);
};

app.onerror = showConnectionError;

document.querySelector(".tabs").addEventListener("click", (event) => {
  const tab = event.target.closest("[data-tab]");
  if (!tab) return;
  showView(tab.dataset.tab);
});

els.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const query = els.query.value.trim();
  if (!query) return;
  showStatus("Searching...");
  try {
    const response = await app.callServerTool({
      name: "youtube_search",
      arguments: { query, limit: 10, response_format: "json" },
    });
    renderPayload(response);
  } catch (error) {
    showStatus(friendlyError(error.message));
  }
});

els.results.addEventListener("click", async (event) => {
  const button = event.target.closest("button[data-action]");
  if (!button) return;
  const url = button.dataset.url;
  const action = button.dataset.action;
  const toolCall = actionToolCall(action, url);
  if (!toolCall) return;
  try {
    const response = await app.callServerTool(toolCall);
    if (response?.isError) {
      throw new Error(toolText(response, "Action failed."));
    }
    prependStatus(friendlyError(toolText(response, "Action completed.")));
  } catch (error) {
    prependStatus(friendlyError(error.message));
  }
});

document.querySelector("#stats-refresh").addEventListener("click", () => {
  loadStats().catch((error) => setStatus("stats", friendlyError(error.message)));
});

document.querySelector("#playlist-refresh").addEventListener("click", () => {
  loadPlaylistCandidates().catch((error) => setStatus("playlist", friendlyError(error.message)));
});
document.querySelector("#playlist-preview").addEventListener("click", () => {
  previewPlaylist().catch((error) => setStatus("playlist", friendlyError(error.message)));
});
document.querySelector("#playlist-apply").addEventListener("click", () => {
  applyPlaylist().catch((error) => setStatus("playlist", friendlyError(error.message)));
});

els.playlistResults.addEventListener("click", (event) => {
  const openButton = event.target.closest("[data-open-url]");
  const copyButton = event.target.closest("[data-copy-text]");
  if (openButton) {
    openAllowedExternal(openButton.dataset.openUrl)
      .catch((error) => setStatus("playlist", friendlyError(error.message)));
  }
  if (copyButton) {
    copyText(copyButton.dataset.copyText)
      .then((ok) => setStatus("playlist", ok ? "Link copied." : "Copy failed."))
      .catch((error) => setStatus("playlist", friendlyError(error.message)));
  }
});

document.querySelector("#transfers-refresh").addEventListener("click", () => {
  loadTransfers().catch((error) => setStatus("transfers", friendlyError(error.message)));
});
document.querySelector("#transfers-retry-all").addEventListener("click", () => {
  retryAllTransfers().catch((error) => setStatus("transfers", friendlyError(error.message)));
});
document.querySelector("#transfers-prune").addEventListener("click", () => {
  pruneTransfers().catch((error) => setStatus("transfers", friendlyError(error.message)));
});
els.transfersResults.addEventListener("click", (event) => {
  const button = event.target.closest("[data-retry-transfer]");
  if (!button) return;
  retryTransfer(button.dataset.retryTransfer)
    .catch((error) => setStatus("transfers", friendlyError(error.message)));
});

if (isDemo) {
  els.query.value = "slow pulp live";
  render(demoResults);
}

app.connect().catch(showConnectionError);
