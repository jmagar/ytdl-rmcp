//! `ytdl-rmcp doctor` — a human-readable diagnostic report for triaging a broken
//! install ("my plugin's MCP server is dead").
//!
//! Unlike the stdio server, this is a plain CLI command: it is **not** the
//! JSON-RPC transport, so printing to stdout here is correct and expected.
//!
//! Robustness contract: doctor never panics and never aborts on a sub-check
//! failure — every section prints what it can and otherwise reports the error
//! inline, because partial diagnostics beat no diagnostics.
//!
//! Secret hygiene: this report only ever prints *presence* (`set` / `not set`),
//! never any token, key, or credential value. The redaction discipline lives in
//! [`presence`]/[`yes_no`]; nothing in this module formats a secret's
//! contents.

use anyhow::Result;

use crate::bootstrap::{self, ResolvedTool};
use crate::config::Config;

/// Run the diagnostic and print the report to stdout. Always returns `Ok(())` —
/// a diagnostic that fails to diagnose is itself a failure, so we report
/// problems in-band rather than via a non-zero exit / error.
pub async fn run() -> Result<()> {
    println!("ytdl-rmcp doctor");
    println!("===============");
    println!();

    report_build();
    println!();
    report_platform();
    println!();
    report_tools();
    println!();
    report_config();

    Ok(())
}

fn report_build() {
    println!("Build");
    println!("  version:  {}", env!("CARGO_PKG_VERSION"));
    println!("  git sha:  {}", env!("YTDL_GIT_SHA"));
}

fn report_platform() {
    println!("Platform");
    println!("  os:    {}", std::env::consts::OS);
    println!("  arch:  {}", std::env::consts::ARCH);
}

/// Report where yt-dlp and ffmpeg resolve from, plus the cache/bin dir — all
/// download-free. We only need config for the explicit binary overrides; if
/// config can't load we still probe PATH/cache with no override.
fn report_tools() {
    println!("Tools");
    println!("  cache/bin dir:  {}", bootstrap::cache_bin_dir().display());

    let cfg = Config::from_env_result().ok();
    let ytdlp_override = cfg.as_ref().and_then(|c| c.ytdlp_path.as_deref());
    let ffmpeg_override = cfg.as_ref().and_then(|c| c.ffmpeg_path.as_deref());

    report_tool("yt-dlp", ytdlp_override, "YTDLP_PATH", "yt-dlp");
    report_tool("ffmpeg", ffmpeg_override, "FFMPEG_PATH", "ffmpeg");
}

fn report_tool(label: &str, override_path: Option<&str>, env_var: &str, bin_name: &str) {
    match bootstrap::resolve_no_download(override_path, env_var, bin_name) {
        Ok(ResolvedTool::Found(p)) => println!("  {label}:  found at {}", p.display()),
        Ok(ResolvedTool::Cached(p)) => println!("  {label}:  cached at {}", p.display()),
        Ok(ResolvedTool::WouldBootstrap) => {
            println!("  {label}:  not found (would bootstrap/download on first use)");
        }
        Err(e) => println!("  {label}:  ERROR resolving: {e}"),
    }
}

/// Report which key settings are present. CRITICAL: only presence is printed,
/// never any secret value. If config fails to load, print the error (which is a
/// validation message, not a secret) and continue.
fn report_config() {
    println!("Config");
    let cfg = match Config::from_env_result() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("  ERROR loading config: {e}");
            return;
        }
    };

    println!(
        "  target path (YTDLP_TARGET_PATH):  {}",
        presence(&cfg.target_path)
    );
    println!(
        "  video target (YTDLP_VIDEO_TARGET_PATH):  {}",
        presence(&cfg.video_target_path)
    );
    println!(
        "  staging dir (YTDLP_STAGING_DIR):  {}",
        presence(&cfg.staging_dir)
    );
    println!("  audio format:                  {}", cfg.audio_format);
    println!(
        "  extra ssh opts (YTDLP_SSH_OPTS):  {}",
        if cfg.ssh_opts.is_empty() {
            "not set"
        } else {
            "set"
        }
    );
    println!(
        "  archive dir (YTDLP_ARCHIVE_DIR):  {}",
        presence(&cfg.archive_dir)
    );
    println!(
        "  history path (YTDLP_HISTORY_PATH):  {}",
        presence(&cfg.history_path)
    );

    // Plex needs both url + token to be functional; report each presence and the
    // combined readiness. The token value itself is never printed.
    println!(
        "  plex url (YTDLP_PLEX_URL):     {}",
        presence(&cfg.plex_url)
    );
    println!(
        "  plex token (YTDLP_PLEX_TOKEN):  {}",
        presence(&cfg.plex_token)
    );
    println!(
        "  plex configured (url+token):   {}",
        yes_no(cfg.plex_url.is_some() && cfg.plex_token.is_some())
    );
    println!(
        "  plex playlist:                 {}",
        presence(&cfg.plex_playlist)
    );

    println!(
        "  clean metadata:                {}",
        yes_no(cfg.clean_metadata)
    );
    println!(
        "  acoustid key (YTDLP_ACOUSTID_CLIENT_KEY):  {}",
        presence(&cfg.acoustid_client_key)
    );
    println!(
        "  fpcalc path (FPCALC_PATH):     {}",
        presence(&cfg.fpcalc_path)
    );
    println!(
        "  musicbrainz contact:           {}",
        presence(&cfg.musicbrainz_contact)
    );

    println!(
        "  yt-dlp override (YTDLP_PATH):  {}",
        presence(&cfg.ytdlp_path)
    );
    println!(
        "  ffmpeg override (FFMPEG_PATH):  {}",
        presence(&cfg.ffmpeg_path)
    );
    println!(
        "  extractor args (YTDLP_EXTRACTOR_ARGS):  {}",
        presence(&cfg.extractor_args)
    );
    println!(
        "  yt-dlp sha256 pin:             {}",
        presence(&cfg.ytdlp_sha256)
    );
    println!(
        "  ffmpeg sha256 pin:             {}",
        presence(&cfg.ffmpeg_sha256)
    );

    println!(
        "  auto update:                   {}",
        yes_no(cfg.auto_update)
    );
    println!("  max age days:                  {}", cfg.max_age_days);
    println!(
        "  update pre-release:            {}",
        yes_no(cfg.update_pre)
    );
    println!(
        "  yt-dlp timeout secs:           {}",
        cfg.ytdlp_timeout_secs
    );
    println!(
        "  transfer timeout secs:         {}",
        cfg.transfer_timeout_secs
    );
}

/// Render an `Option` as presence only. A set value is shown masked (`set
/// (••••)`) so the reader knows it's populated without ever leaking the value;
/// an unset value is `not set`. Used for both secrets and plain settings so the
/// report is uniform and no value is ever printed.
pub(crate) fn presence<T>(value: &Option<T>) -> &'static str {
    match value {
        Some(_) => "set (••••)",
        None => "not set",
    }
}

pub(crate) fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
