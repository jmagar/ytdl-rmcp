# YouTube Search MCP UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add YouTube search as both a regular MCP tool and an Aurora-styled MCP App UI backed by the same `yt-dlp` search service.

**Architecture:** Implement a shared `run_search(cfg, SearchInput)` service that shells out to `yt-dlp "ytsearchN:<query>" --dump-single-json --skip-download` and normalizes entries into a stable Rust result model. Expose it through `youtube_search` for regular MCP clients and `youtube_search_ui` for UI-capable hosts, with the UI served as an embedded HTML MCP resource from the single Rust binary.

**Tech Stack:** Rust 2021, `rmcp` 1.7 tools/resources, Tokio process execution, yt-dlp search extractor, serde/schemars, embedded static HTML/CSS/JS using Aurora tokens and Lucide-style inline SVG icons.

---

## File Structure

- Modify: `src/main.rs` — declare new `search_app` module.
- Modify: `src/model.rs` — add `SearchInput`, `SearchResultItem`, `SearchPayload`, `SearchAction`, and defaults.
- Modify: `src/model_tests.rs` — cover search input defaults and limit clamping.
- Modify: `src/downloader.rs` — add `search_youtube` and JSON parsing helpers for `ytsearchN:query`.
- Modify: `src/downloader_tests.rs` — unit-test parsing against fixture JSON without spawning yt-dlp.
- Modify: `src/service.rs` — add shared `run_search`, `run_search_payload`, markdown/JSON rendering.
- Modify: `src/service_tests.rs` — unit-test markdown and JSON payload formatting.
- Modify: `src/mcp.rs` — add `youtube_search` regular tool, `youtube_search_ui` app tool, `resources/list`, `resources/read`, and resource capability.
- Create: `src/search_app.rs` — constants and helper functions for the app resource URI, HTML, UI metadata, and tool-result metadata.
- Create: `src/search_app_tests.rs` — test resource URI, CSP metadata, and embedded HTML contains Aurora/token/UI hooks.
- Create: `assets/youtube-search-app.html` — embedded static MCP App UI, styled with Aurora semantics and wired to MCP App host APIs when available.
- Modify: `README.md` — document both search tools and YouTube-only scope.
- Modify: `skills/ytdl/SKILL.md` — document search usage for agents.

Keep every source file under 500 LOC. If `mcp.rs` grows too large while adding resources, split UI resource helpers into `search_app.rs` rather than moving unrelated tool logic.

## UI Design System

Use the requested UI guidance as follows:

- Aurora: dark-first navy operator shell, semantic `--aurora-*` tokens, Manrope/Inter/JetBrains Mono stack, cyan primary selection/focus, rose only for download actions, Lucide-style line icons at 14-18px.
- Lavra frontend design: make the UI feel like a "dark media command deck": restrained, high-control, memorable through precision and result density rather than a marketing hero.
- Frontend app builder: before implementing `assets/youtube-search-app.html`, generate and accept a visual concept for the primary screen. Treat the accepted concept as the fidelity target and verify with screenshots.

## Task 1: Search Models

**Files:**
- Modify: `src/model.rs`
- Modify: `src/model_tests.rs`

- [ ] **Step 1: Write failing model tests**

Add these tests to `src/model_tests.rs`:

```rust
use super::{ResponseFormat, SearchInput};

#[test]
fn search_input_defaults_limit_and_markdown() {
    let input: SearchInput = serde_json::from_str(r#"{"query":"slow pulp live"}"#).unwrap();

    assert_eq!(input.query, "slow pulp live");
    assert_eq!(input.limit, 10);
    assert_eq!(input.response_format, ResponseFormat::Markdown);
}

#[test]
fn search_input_clamps_limit_to_supported_range() {
    let low: SearchInput = serde_json::from_str(r#"{"query":"x","limit":0}"#).unwrap();
    let high: SearchInput = serde_json::from_str(r#"{"query":"x","limit":100}"#).unwrap();

    assert_eq!(low.effective_limit(), 1);
    assert_eq!(high.effective_limit(), 25);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test search_input -- --nocapture
```

Expected: compile failure naming missing `SearchInput`.

- [ ] **Step 3: Add search structs and defaults**

Add this to `src/model.rs` after `ProbeInput`:

```rust
fn default_search_limit() -> u32 {
    10
}

/// Input for `youtube_search` and `youtube_search_ui`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchInput {
    /// YouTube search terms. This is passed to yt-dlp as `ytsearchN:<query>`.
    pub query: String,
    /// Number of YouTube results to return. Clamped to 1..=25.
    #[serde(default = "default_search_limit")]
    pub limit: u32,
    /// 'markdown' (human-readable) or 'json' (machine-readable).
    #[serde(default)]
    pub response_format: ResponseFormat,
}

impl SearchInput {
    pub fn effective_limit(&self) -> u32 {
        self.limit.clamp(1, 25)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchResultItem {
    pub title: String,
    pub url: String,
    pub video_id: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub view_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchPayload {
    pub query: String,
    pub limit: u32,
    pub results: Vec<SearchResultItem>,
}
```

- [ ] **Step 4: Run tests to verify pass**

Run:

```bash
cargo test search_input -- --nocapture
```

Expected: both new tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/model.rs src/model_tests.rs
git commit -m "feat: add youtube search models"
```

## Task 2: yt-dlp Search Parser

**Files:**
- Modify: `src/downloader.rs`
- Modify: `src/downloader_tests.rs`

- [ ] **Step 1: Write failing parser test**

Add this to `src/downloader_tests.rs`:

```rust
use crate::model::SearchResultItem;

#[test]
fn parse_search_json_extracts_youtube_entries() {
    let json = br#"{
      "id": "slow pulp live",
      "title": "slow pulp live",
      "entries": [
        {
          "id": "abc123",
          "title": "Slow Pulp - Falling Apart Live",
          "webpage_url": "https://www.youtube.com/watch?v=abc123",
          "uploader": "Slow Pulp",
          "duration": 215.0,
          "thumbnail": "https://i.ytimg.com/vi/abc123/hqdefault.jpg",
          "view_count": 42000
        },
        null,
        {
          "id": "def456",
          "title": "Slow Pulp - Idaho Live",
          "url": "https://www.youtube.com/watch?v=def456",
          "channel": "Live Room",
          "duration": 188
        }
      ]
    }"#;

    let results = super::parse_search_json(json).unwrap();

    assert_eq!(
        results,
        vec![
            SearchResultItem {
                title: "Slow Pulp - Falling Apart Live".into(),
                url: "https://www.youtube.com/watch?v=abc123".into(),
                video_id: Some("abc123".into()),
                uploader: Some("Slow Pulp".into()),
                duration: Some(215.0),
                thumbnail: Some("https://i.ytimg.com/vi/abc123/hqdefault.jpg".into()),
                view_count: Some(42000),
            },
            SearchResultItem {
                title: "Slow Pulp - Idaho Live".into(),
                url: "https://www.youtube.com/watch?v=def456".into(),
                video_id: Some("def456".into()),
                uploader: Some("Live Room".into()),
                duration: Some(188.0),
                thumbnail: None,
                view_count: None,
            },
        ]
    );
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test parse_search_json_extracts_youtube_entries -- --nocapture
```

Expected: compile failure for missing `parse_search_json`.

- [ ] **Step 3: Implement parser helper**

Add to `src/downloader.rs` near `probe`:

```rust
pub(crate) fn parse_search_json(bytes: &[u8]) -> Result<Vec<crate::model::SearchResultItem>> {
    let info: serde_json::Value = serde_json::from_slice(bytes)?;
    let entries = info
        .get("entries")
        .and_then(|entries| entries.as_array())
        .cloned()
        .unwrap_or_default();

    let mut results = Vec::new();
    for entry in entries.iter().filter(|entry| !entry.is_null()) {
        let Some(title) = str_field(entry, "title") else {
            continue;
        };
        let Some(url) = str_field(entry, "webpage_url").or_else(|| str_field(entry, "url")) else {
            continue;
        };
        results.push(crate::model::SearchResultItem {
            title,
            url,
            video_id: str_field(entry, "id"),
            uploader: str_field(entry, "uploader").or_else(|| str_field(entry, "channel")),
            duration: entry.get("duration").and_then(|d| d.as_f64()),
            thumbnail: str_field(entry, "thumbnail"),
            view_count: entry.get("view_count").and_then(|v| v.as_u64()),
        });
    }

    Ok(results)
}
```

- [ ] **Step 4: Run parser test**

Run:

```bash
cargo test parse_search_json_extracts_youtube_entries -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src/downloader.rs src/downloader_tests.rs
git commit -m "feat: parse youtube search results"
```

## Task 3: yt-dlp Search Execution

**Files:**
- Modify: `src/downloader.rs`

- [ ] **Step 1: Write failing command-shape test**

Add this to `src/downloader_tests.rs`:

```rust
#[test]
fn search_query_spec_uses_ytsearch_limit_prefix() {
    assert_eq!(super::search_spec("tiny desk", 7), "ytsearch7:tiny desk");
    assert_eq!(super::search_spec("  tiny desk  ", 2), "ytsearch2:tiny desk");
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test search_query_spec_uses_ytsearch_limit_prefix -- --nocapture
```

Expected: compile failure for missing `search_spec`.

- [ ] **Step 3: Implement `search_spec` and `search_youtube`**

Add to `src/downloader.rs` after `parse_search_json`:

```rust
pub(crate) fn search_spec(query: &str, limit: u32) -> String {
    format!("ytsearch{}:{}", limit.clamp(1, 25), query.trim())
}

pub async fn search_youtube(
    ytdlp: &Path,
    query: &str,
    limit: u32,
    extractor_args: Option<&str>,
    timeout: Option<Duration>,
) -> Result<Vec<crate::model::SearchResultItem>> {
    let mut cmd = Command::new(ytdlp);
    cmd.args(["--dump-single-json", "--skip-download", "--no-warnings", "--quiet"]);
    if let Some(extra) = extractor_args {
        cmd.arg("--extractor-args").arg(extra);
    }
    cmd.arg(search_spec(query, limit));

    let output = run_command(&mut cmd, timeout).await?;
    if !output.status.success() {
        bail!("{}", command_error_text(&output.stderr, &output.stdout));
    }

    parse_search_json(&output.stdout)
}
```

- [ ] **Step 4: Run downloader tests**

Run:

```bash
cargo test downloader_tests -- --nocapture
```

Expected: all downloader tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/downloader.rs src/downloader_tests.rs
git commit -m "feat: run yt-dlp youtube search"
```

## Task 4: Search Service and Rendering

**Files:**
- Modify: `src/service.rs`
- Modify: `src/service_tests.rs`

- [ ] **Step 1: Write payload rendering tests**

Add this to `src/service_tests.rs`:

```rust
use crate::model::{ResponseFormat, SearchPayload, SearchResultItem};

#[test]
fn render_search_markdown_lists_results_with_urls() {
    let payload = SearchPayload {
        query: "slow pulp".into(),
        limit: 2,
        results: vec![SearchResultItem {
            title: "Slow Pulp - Falling Apart Live".into(),
            url: "https://www.youtube.com/watch?v=abc123".into(),
            video_id: Some("abc123".into()),
            uploader: Some("Slow Pulp".into()),
            duration: Some(215.0),
            thumbnail: None,
            view_count: Some(42000),
        }],
    };

    let rendered = super::render_search_for_test(&payload, ResponseFormat::Markdown);

    assert!(rendered.contains("YouTube search: slow pulp"));
    assert!(rendered.contains("Slow Pulp - Falling Apart Live"));
    assert!(rendered.contains("https://www.youtube.com/watch?v=abc123"));
    assert!(rendered.contains("3:35"));
}

#[test]
fn render_search_json_has_results_array() {
    let payload = SearchPayload {
        query: "slow pulp".into(),
        limit: 1,
        results: Vec::new(),
    };

    let rendered = super::render_search_for_test(&payload, ResponseFormat::Json);
    let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();

    assert_eq!(value["query"], "slow pulp");
    assert_eq!(value["results"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test render_search_ -- --nocapture
```

Expected: compile failure for missing `render_search_for_test`.

- [ ] **Step 3: Implement service functions**

Add imports in `src/service.rs`:

```rust
use crate::model::{DownloadInput, ProbeInput, ResponseFormat, SearchInput, SearchPayload};
```

Add these functions near `run_probe`:

```rust
pub async fn run_search_payload(cfg: &Config, input: &SearchInput) -> Result<SearchPayload> {
    let query = input.query.trim();
    if query.is_empty() {
        bail!("Search query cannot be empty.");
    }

    let ytdlp = ensure_ytdlp(cfg).await?;
    let limit = input.effective_limit();
    let results = downloader::search_youtube(
        &ytdlp,
        query,
        limit,
        cfg.extractor_args.as_deref(),
        Some(cfg.ytdlp_timeout()),
    )
    .await?;

    Ok(SearchPayload {
        query: query.to_string(),
        limit,
        results,
    })
}

pub async fn run_search(cfg: &Config, input: SearchInput) -> Result<String> {
    let payload = run_search_payload(cfg, &input).await?;
    Ok(render(
        &serde_json::to_value(&payload)?,
        input.response_format,
        render_search_markdown,
    ))
}

fn render_search_markdown(payload: &serde_json::Value) -> String {
    let query = payload["query"].as_str().unwrap_or("");
    let mut out = format!("# YouTube search: {query}\n\n");
    let Some(results) = payload["results"].as_array() else {
        return out;
    };
    if results.is_empty() {
        out.push_str("No results.\n");
        return out;
    }

    for (idx, item) in results.iter().enumerate() {
        let title = item["title"].as_str().unwrap_or("Untitled");
        let url = item["url"].as_str().unwrap_or("");
        let uploader = item["uploader"].as_str().unwrap_or("Unknown channel");
        let duration = item["duration"]
            .as_f64()
            .map(format_duration)
            .unwrap_or_else(|| "unknown duration".into());
        out.push_str(&format!(
            "{}. [{}]({})\n   - {} - {}\n",
            idx + 1,
            title,
            url,
            uploader,
            duration
        ));
    }
    out
}

fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}

#[cfg(test)]
pub(crate) fn render_search_for_test(payload: &SearchPayload, format: ResponseFormat) -> String {
    render(
        &serde_json::to_value(payload).expect("search payload serializes"),
        format,
        render_search_markdown,
    )
}
```

- [ ] **Step 4: Run service tests**

Run:

```bash
cargo test render_search_ -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add src/service.rs src/service_tests.rs
git commit -m "feat: render youtube search results"
```

## Task 5: Regular MCP Tool

**Files:**
- Modify: `src/mcp.rs`

- [ ] **Step 1: Add `SearchInput` import**

Change the model import in `src/mcp.rs`:

```rust
use crate::model::{DownloadInput, ProbeInput, SearchInput};
```

- [ ] **Step 2: Add `youtube_search` tool**

Add this inside the `#[tool_router] impl YtdlServer` block after `youtube_probe`:

```rust
/// Search YouTube through yt-dlp without downloading. Returns result URLs that
/// can be passed to `youtube_probe` or `youtube_download`.
#[tool(
    name = "youtube_search",
    description = "Search YouTube with yt-dlp and return matching video URLs without downloading."
)]
async fn youtube_search(
    &self,
    Parameters(input): Parameters<SearchInput>,
) -> Result<CallToolResult, ErrorData> {
    match service::run_search(&self.cfg, input).await {
        Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
        Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
            "Error: {e}"
        ))])),
    }
}
```

- [ ] **Step 3: Run compile check**

Run:

```bash
cargo check
```

Expected: success.

- [ ] **Step 4: Run a live low-limit search smoke**

Run:

```bash
cargo run --quiet -- --help >/tmp/ytdl-mcp-help.txt
yt-dlp --dump-single-json --skip-download --no-warnings --quiet "ytsearch1:slow pulp live" | jq '.entries | length'
```

Expected: first command exits 0; second command prints `1`. This proves the upstream extractor path works in the current environment before MCP-level smoke testing.

- [ ] **Step 5: Commit**

```bash
git add src/mcp.rs
git commit -m "feat: expose youtube search tool"
```

## Task 6: Embedded MCP App Resource

**Files:**
- Create: `src/search_app.rs`
- Create: `src/search_app_tests.rs`
- Modify: `src/main.rs`
- Modify: `src/mcp.rs`
- Create: `assets/youtube-search-app.html`

- [ ] **Step 1: Write resource metadata tests**

Create `src/search_app_tests.rs`:

```rust
use rmcp::model::ResourceContents;

#[test]
fn app_resource_uri_is_stable() {
    assert_eq!(super::RESOURCE_URI, "ui://ytdl-mcp/youtube-search.html");
}

#[test]
fn app_resource_contains_html_and_aurora_hooks() {
    let result = super::read_app_resource(super::RESOURCE_URI).unwrap();
    let ResourceContents::TextResourceContents {
        text,
        mime_type,
        meta,
        ..
    } = &result.contents[0]
    else {
        panic!("expected text resource");
    };

    assert_eq!(mime_type.as_deref(), Some("text/html"));
    assert!(text.contains("YouTube search"));
    assert!(text.contains("--aurora-page-bg"));
    assert!(meta.as_ref().unwrap().0.contains_key("ui.csp"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test search_app -- --nocapture
```

Expected: compile failure for missing module.

- [ ] **Step 3: Add static HTML shell**

Create `assets/youtube-search-app.html` with this initial complete shell:

```html
<!doctype html>
<html lang="en" class="dark">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>YouTube search</title>
  <style>
    :root {
      --aurora-page-bg: #07131c;
      --aurora-nav-bg: #091923;
      --aurora-panel-medium: #102532;
      --aurora-panel-strong: #132d3d;
      --aurora-control-surface: #0d202c;
      --aurora-hover-bg: color-mix(in srgb, #29b6f6 10%, transparent);
      --aurora-border-default: #1d3d4e;
      --aurora-border-strong: #2a5870;
      --aurora-text-primary: #e6f4fb;
      --aurora-text-muted: #a7bcc9;
      --aurora-accent-primary: #29b6f6;
      --aurora-accent-pink: #f9a8c4;
      --aurora-active-glow: 0 0 0 1px color-mix(in srgb, var(--aurora-accent-primary) 78%, transparent), 0 0 26px color-mix(in srgb, var(--aurora-accent-primary) 22%, transparent);
      --aurora-radius-1: 14px;
      --aurora-radius-2: 18px;
      --aurora-radius-3: 22px;
      --aurora-shadow-strong: 0 24px 70px rgba(0, 0, 0, 0.38);
      color-scheme: dark;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      background: radial-gradient(circle at 20% 0%, rgba(41, 182, 246, 0.13), transparent 32%), var(--aurora-page-bg);
      color: var(--aurora-text-primary);
      font: 14px/1.45 Inter, ui-sans-serif, system-ui, sans-serif;
    }
    .shell { padding: 18px; }
    .panel {
      border: 1px solid var(--aurora-border-strong);
      border-radius: var(--aurora-radius-3);
      background: var(--aurora-panel-strong);
      box-shadow: var(--aurora-shadow-strong), inset 0 1px 0 rgba(255,255,255,0.05);
      overflow: hidden;
    }
    .toolbar {
      display: grid;
      grid-template-columns: 1fr auto;
      gap: 10px;
      padding: 14px;
      background: var(--aurora-panel-medium);
      border-bottom: 1px solid var(--aurora-border-default);
    }
    input {
      min-width: 0;
      height: 40px;
      border-radius: var(--aurora-radius-1);
      border: 1px solid var(--aurora-border-default);
      background: var(--aurora-control-surface);
      color: var(--aurora-text-primary);
      padding: 0 12px;
      font: 560 13px/1 Inter, ui-sans-serif, system-ui, sans-serif;
      outline: none;
    }
    input:focus { border-color: var(--aurora-border-strong); box-shadow: var(--aurora-active-glow); }
    button {
      height: 40px;
      border-radius: var(--aurora-radius-1);
      border: 1px solid var(--aurora-border-strong);
      background: color-mix(in srgb, var(--aurora-accent-primary) 18%, var(--aurora-control-surface));
      color: var(--aurora-text-primary);
      font: 650 13px/1 Inter, ui-sans-serif, system-ui, sans-serif;
      padding: 0 14px;
      cursor: pointer;
    }
    .results { display: grid; gap: 1px; background: var(--aurora-border-default); }
    .result {
      display: grid;
      grid-template-columns: 112px 1fr auto;
      gap: 14px;
      padding: 12px;
      background: var(--aurora-panel-strong);
      border: 1px solid transparent;
    }
    .result[aria-selected="true"] { border-color: var(--aurora-border-strong); box-shadow: inset 3px 0 0 var(--aurora-accent-primary); }
    .thumb {
      width: 112px;
      aspect-ratio: 16 / 9;
      border-radius: 8px;
      object-fit: cover;
      background: var(--aurora-control-surface);
    }
    .title { margin: 0 0 5px; font: 760 14px/1.2 Manrope, Inter, sans-serif; }
    .meta { color: var(--aurora-text-muted); font-size: 12px; }
    .actions { display: flex; align-items: center; gap: 8px; }
    .actions button { height: 32px; padding: 0 10px; }
    .actions .download { border-color: color-mix(in srgb, var(--aurora-accent-pink) 55%, var(--aurora-border-strong)); background: color-mix(in srgb, var(--aurora-accent-pink) 13%, var(--aurora-control-surface)); }
    .empty { padding: 30px; color: var(--aurora-text-muted); text-align: center; }
    @media (max-width: 640px) {
      .toolbar { grid-template-columns: 1fr; }
      .result { grid-template-columns: 88px 1fr; }
      .thumb { width: 88px; }
      .actions { grid-column: 1 / -1; }
    }
  </style>
</head>
<body>
  <main class="shell">
    <section class="panel">
      <form class="toolbar" id="search-form">
        <input id="query" name="query" autocomplete="off" placeholder="Search YouTube" />
        <button type="submit">Search</button>
      </form>
      <div class="results" id="results">
        <div class="empty">Enter a query to search YouTube.</div>
      </div>
    </section>
  </main>
  <script>
    const resultsEl = document.querySelector("#results");
    const form = document.querySelector("#search-form");
    const queryEl = document.querySelector("#query");
    const app = window.mcpApp || window.openai;

    function formatDuration(seconds) {
      if (!seconds) return "unknown duration";
      const total = Math.round(seconds);
      return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
    }

    function render(results) {
      if (!results || results.length === 0) {
        resultsEl.innerHTML = '<div class="empty">No results.</div>';
        return;
      }
      resultsEl.innerHTML = results.map((item, index) => `
        <article class="result" aria-selected="${index === 0}">
          <img class="thumb" src="${item.thumbnail || ""}" alt="" />
          <div>
            <h2 class="title">${item.title}</h2>
            <div class="meta">${item.uploader || "Unknown channel"} - ${formatDuration(item.duration)}</div>
            <div class="meta">${item.url}</div>
          </div>
          <div class="actions">
            <button type="button" data-action="probe" data-url="${item.url}">Probe</button>
            <button type="button" class="download" data-action="audio" data-url="${item.url}">Audio</button>
            <button type="button" class="download" data-action="video" data-url="${item.url}">Video</button>
          </div>
        </article>
      `).join("");
    }

    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      const query = queryEl.value.trim();
      if (!query) return;
      resultsEl.innerHTML = '<div class="empty">Searching...</div>';
      if (app && app.callTool) {
        const response = await app.callTool("youtube_search", { query, limit: 10, response_format: "json" });
        const payload = response.structuredContent || JSON.parse(response.content?.[0]?.text || "{}");
        render(payload.results || []);
      }
    });

    resultsEl.addEventListener("click", async (event) => {
      const button = event.target.closest("button[data-action]");
      if (!button || !app || !app.callTool) return;
      const url = button.dataset.url;
      const action = button.dataset.action;
      if (action === "probe") await app.callTool("youtube_probe", { urls: url });
      if (action === "audio") await app.callTool("youtube_download", { urls: url, mode: "audio" });
      if (action === "video") await app.callTool("youtube_download", { urls: url, mode: "video" });
    });
  </script>
</body>
</html>
```

- [ ] **Step 4: Add `search_app` module**

Create `src/search_app.rs`:

```rust
use rmcp::model::{
    ListResourcesResult, Meta, RawResource, ReadResourceResult, ResourceContents,
};
use serde_json::json;

pub const RESOURCE_URI: &str = "ui://ytdl-mcp/youtube-search.html";
const HTML: &str = include_str!("../assets/youtube-search-app.html");

pub fn list_app_resources() -> ListResourcesResult {
    ListResourcesResult {
        resources: vec![RawResource::new(RESOURCE_URI, "youtube-search")
            .with_title("YouTube search")
            .with_description("Search YouTube and send results to ytdl-mcp actions.")
            .with_mime_type("text/html")
            .no_annotation()],
        next_cursor: None,
    }
}

pub fn read_app_resource(uri: &str) -> Option<ReadResourceResult> {
    if uri != RESOURCE_URI {
        return None;
    }
    let mut meta = Meta::new();
    meta.0.insert(
        "ui.csp".into(),
        json!({
            "connect_domains": ["https://i.ytimg.com", "https://img.youtube.com"],
            "resource_domains": ["https://i.ytimg.com", "https://img.youtube.com"]
        }),
    );
    Some(ReadResourceResult::new(vec![
        ResourceContents::text(HTML, RESOURCE_URI)
            .with_mime_type("text/html")
            .with_meta(meta),
    ]))
}

pub fn tool_meta() -> Meta {
    let mut meta = Meta::new();
    meta.0.insert("ui.resourceUri".into(), json!(RESOURCE_URI));
    meta
}

#[cfg(test)]
#[path = "search_app_tests.rs"]
mod tests;
```

If `RawResource::with_mime_type` or `.no_annotation()` is unavailable in the installed `rmcp`, inspect `/home/jmagar/.cargo/registry/src/index.crates.io-*/rmcp-1.7.0/src/model/resource.rs` and use the equivalent builder or construct `RawResource { uri, name, title, description, mime_type, size, icons, meta }.no_annotation()` directly.

- [ ] **Step 5: Wire module declarations**

Add to `src/main.rs`:

```rust
mod search_app;
```

- [ ] **Step 6: Run resource tests**

Run:

```bash
cargo test search_app -- --nocapture
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/search_app.rs src/search_app_tests.rs assets/youtube-search-app.html
git commit -m "feat: add embedded youtube search app resource"
```

## Task 7: MCP App Tool and Resource Handlers

**Files:**
- Modify: `src/mcp.rs`

- [ ] **Step 1: Add imports**

Update the `rmcp::model` import in `src/mcp.rs`:

```rust
use rmcp::model::{
    CallToolResult, Content, Implementation, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo,
};
```

Add:

```rust
use crate::search_app;
```

- [ ] **Step 2: Add `youtube_search_ui` app tool**

Add to the `#[tool_router] impl YtdlServer` block:

```rust
/// Open the interactive YouTube search MCP App. UI-capable hosts render the
/// embedded Aurora search panel; other hosts receive text fallback results.
#[tool(
    name = "youtube_search_ui",
    description = "Open an interactive YouTube search UI for selecting videos to probe or download."
)]
async fn youtube_search_ui(
    &self,
    Parameters(input): Parameters<SearchInput>,
) -> Result<CallToolResult, ErrorData> {
    match service::run_search_payload(&self.cfg, &input).await {
        Ok(payload) => {
            let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into());
            let mut result = CallToolResult::success(vec![Content::text(text)]);
            result.structured_content = Some(serde_json::to_value(&payload).unwrap_or_default());
            result.meta = Some(search_app::tool_meta());
            Ok(result)
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
            "Error: {e}"
        ))])),
    }
}
```

- [ ] **Step 3: Add resource handlers**

Extend the `#[tool_handler] impl ServerHandler for YtdlServer` block:

```rust
fn list_resources(
    &self,
    _request: Option<PaginatedRequestParams>,
    _context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> impl std::future::Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
    std::future::ready(Ok(search_app::list_app_resources()))
}

fn read_resource(
    &self,
    request: ReadResourceRequestParams,
    _context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> impl std::future::Future<Output = Result<ReadResourceResult, ErrorData>> + Send + '_ {
    std::future::ready(
        search_app::read_app_resource(&request.uri).ok_or_else(|| {
            ErrorData::invalid_params(format!("Unknown resource URI: {}", request.uri), None)
        }),
    )
}
```

If the exact trait signature differs, copy it from `/home/jmagar/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rmcp-1.7.0/src/handler/server.rs` and keep the body identical.

- [ ] **Step 4: Enable resources capability**

Change `get_info` in `src/mcp.rs`:

```rust
ServerInfo::new(
    ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .build(),
)
.with_server_info(Implementation::new("ytdl-mcp", env!("CARGO_PKG_VERSION")))
```

- [ ] **Step 5: Run compile check**

Run:

```bash
cargo check
```

Expected: success.

- [ ] **Step 6: Commit**

```bash
git add src/mcp.rs
git commit -m "feat: expose youtube search app"
```

## Task 8: UI Concept and Fidelity Pass

**Files:**
- Modify: `assets/youtube-search-app.html`
- Create: `docs/superpowers/artifacts/youtube-search-ui-concept.png`
- Create: `docs/superpowers/artifacts/youtube-search-ui-render.png`

- [ ] **Step 1: Generate the accepted UI concept**

Use Image Gen with this prompt:

```text
Create a production UI concept screenshot for an MCP App called "YouTube search" inside an Aurora operator console. Dark-first navy interface, cyan primary focus/selection, rose only for download actions, Manrope display type, Inter compact working UI. The screen is a compact media command deck: top toolbar with search input and Search button, result list with YouTube thumbnails, title, channel, duration, URL metadata, selected result border/glow, and Probe / Audio / Video actions. No marketing hero, no cards inside cards, no emoji, no purple gradients, no glassmorphism. It should feel like Labby/Aurora: operational, polished, dense but readable, with exact code-native UI text.
```

Save the accepted image as:

```text
docs/superpowers/artifacts/youtube-search-ui-concept.png
```

- [ ] **Step 2: Compare current HTML against concept**

Open `assets/youtube-search-app.html` in a browser or serve it with:

```bash
python3 -m http.server 4177 --directory assets
```

Capture the rendered screenshot as:

```text
docs/superpowers/artifacts/youtube-search-ui-render.png
```

- [ ] **Step 3: Use `view_image` on both images**

Inspect both:

```text
docs/superpowers/artifacts/youtube-search-ui-concept.png
docs/superpowers/artifacts/youtube-search-ui-render.png
```

Expected: implementation matches concept on layout, density, copy, tokens, selected state, button hierarchy, thumbnail framing, mobile collapse, and no unapproved visible copy.

- [ ] **Step 4: Patch visual mismatches**

Edit only `assets/youtube-search-app.html`. Keep these rules:

```text
- Use Aurora token variable names for styling decisions.
- Keep rose for download actions only.
- Keep focus and selection as border plus glow.
- Keep all UI text sentence case.
- Keep buttons 13px Inter control text.
- Keep thumbnails stable at 16:9 with 8px radius.
```

- [ ] **Step 5: Re-run image inspection**

Repeat Steps 2 and 3 until the UI would pass design review.

- [ ] **Step 6: Commit**

```bash
git add assets/youtube-search-app.html docs/superpowers/artifacts/youtube-search-ui-concept.png docs/superpowers/artifacts/youtube-search-ui-render.png
git commit -m "design: refine youtube search app ui"
```

## Task 9: Docs and Agent Skill

**Files:**
- Modify: `README.md`
- Modify: `skills/ytdl/SKILL.md`

- [ ] **Step 1: Update README tool table**

Change the README tool table to:

```markdown
| Tool | Purpose |
| --- | --- |
| `youtube_search` | Search YouTube with yt-dlp and return result URLs without downloading. |
| `youtube_search_ui` | Open an interactive YouTube search UI in MCP App-capable hosts. |
| `youtube_download` | Download one or more URLs (audio/video/both) and rsync/scp them to a remote dir. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |
```

- [ ] **Step 2: Add README search parameters**

Add after the `youtube_probe` sentence:

```markdown
### `youtube_search` parameters

| Param | Default | Meaning |
| --- | --- | --- |
| `query` | — (required) | YouTube search text. The server passes this to yt-dlp as `ytsearchN:<query>`. |
| `limit` | `10` | Number of results, clamped to `1..=25`. |
| `response_format` | `markdown` | `markdown` or `json`. |

`youtube_search_ui` accepts the same input and returns the same search payload, plus MCP App metadata for hosts that can render the embedded UI.
```

- [ ] **Step 3: Update skill tool table**

Change the table in `skills/ytdl/SKILL.md` to include:

```markdown
| Tool | Purpose |
| --- | --- |
| `youtube_search` | Search YouTube and return result URLs without downloading. |
| `youtube_search_ui` | Open an interactive YouTube search UI in MCP App-capable hosts. |
| `youtube_download` | Download one or more URLs and transfer them to the remote with rsync or scp. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |
```

- [ ] **Step 4: Add skill examples**

Add examples to `skills/ytdl/SKILL.md`:

````markdown
Search YouTube first:

```text
youtube_search(query="slow pulp live", limit=5)
```

Open the interactive search UI:

```text
youtube_search_ui(query="slow pulp live", limit=10)
```
````

- [ ] **Step 5: Run docs grep**

Run:

```bash
rg -n "youtube_search|youtube_search_ui|ytsearch" README.md skills/ytdl/SKILL.md
```

Expected: both tool names and `ytsearch` are documented.

- [ ] **Step 6: Commit**

```bash
git add README.md skills/ytdl/SKILL.md
git commit -m "docs: document youtube search tools"
```

## Task 10: Full Verification

**Files:**
- No planned source edits unless checks fail.

- [ ] **Step 1: Format check**

Run:

```bash
cargo fmt --all --check
```

Expected: success.

- [ ] **Step 2: Unit tests**

Run:

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 3: Clippy**

Run:

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: success.

- [ ] **Step 4: Live yt-dlp search proof**

Run:

```bash
yt-dlp --dump-single-json --skip-download --no-warnings --quiet "ytsearch1:slow pulp live" | jq -r '.entries[0].webpage_url // .entries[0].url'
```

Expected: prints a YouTube URL.

- [ ] **Step 5: MCP stdio smoke for regular tool**

Run the built binary through an MCP client such as `mcporter` if available. If using a raw JSON-RPC smoke, keep stdin open:

```bash
cargo build
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.0.0"}}}'; sleep 1; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'; printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"youtube_search","arguments":{"query":"slow pulp live","limit":1,"response_format":"json"}}}'; sleep 8; } | ./target/debug/ytdl-mcp
```

Expected: response for id `2` contains `youtube_search` result JSON with at least one URL.

- [ ] **Step 6: MCP resource smoke**

Use the same MCP client to call:

```json
{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}
{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"ui://ytdl-mcp/youtube-search.html"}}
```

Expected: resource list includes `ui://ytdl-mcp/youtube-search.html`; resource read returns `text/html` containing `YouTube search`.

- [ ] **Step 7: Final status**

Run:

```bash
git status --short
```

Expected: clean worktree, unless screenshot artifacts are intentionally left uncommitted for review.

## Self-Review Notes

- Spec coverage: regular `youtube_search` tool is covered by Tasks 1-5 and 10; `youtube_search_ui` MCP App resource/tool is covered by Tasks 6-8 and 10; docs are covered by Task 9.
- Placeholder scan: no task relies on a later unspecified implementation. Two API caveats are explicit and point to exact local `rmcp` source files if the installed builder signatures differ.
- Type consistency: `SearchInput`, `SearchPayload`, and `SearchResultItem` are introduced in Task 1 and reused consistently through service, downloader, MCP tool, and UI payload.
