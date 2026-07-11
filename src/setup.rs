//! `ytdl-rmcp setup` — interactive installer.
//!
//! 1. Ensure yt-dlp + ffmpeg are installed (into the cache dir).
//! 2. Prompt for audio/video target destinations.
//! 3. Detect which agent CLIs (claude/codex/gemini) are present.
//! 4. Register this binary into the selected ones via their `mcp add` commands
//!    (rather than hand-editing any JSON/TOML config).

use std::process::Command;

use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect};

use crate::bootstrap;
use crate::config::Config;

const SERVER_NAME: &str = "ytdl-rmcp";
const DEFAULT_EXTRACTOR_ARGS: &str = "youtube:player_client=android";

/// One agent CLI and how it registers a stdio MCP server.
struct Agent {
    /// Binary name on PATH.
    bin: &'static str,
    /// Human label.
    label: &'static str,
}

const AGENTS: &[Agent] = &[
    Agent {
        bin: "claude",
        label: "Claude Code",
    },
    Agent {
        bin: "codex",
        label: "Codex",
    },
    Agent {
        bin: "gemini",
        label: "Gemini CLI",
    },
];

pub async fn run() -> Result<()> {
    eprintln!("ytdl-rmcp setup\n");

    // 1. Install/verify yt-dlp + ffmpeg.
    eprintln!("Checking yt-dlp + ffmpeg…");
    let cfg = Config::from_env_result()?;
    let tools = tokio::task::spawn_blocking(move || bootstrap::ensure(&cfg)).await??;
    eprintln!("  yt-dlp:  {}", tools.ytdlp.display());
    eprintln!(
        "  ffmpeg:  {}\n",
        tools
            .ffmpeg_dir
            .as_deref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "(system)".into())
    );

    // 2. Prompt for config.
    let theme = ColorfulTheme::default();
    let target_path: String = Input::with_theme(&theme)
        .with_prompt("Target path (/path, host:/path, remote:path, or rclone:remote:path)")
        .interact_text()?;
    let video_target_path: String = Input::with_theme(&theme)
        .with_prompt("Video target path (blank = same as target path)")
        .allow_empty(true)
        .interact_text()?;

    // 3. Detect available agent CLIs.
    let available: Vec<&Agent> = AGENTS
        .iter()
        .filter(|a| which::which(a.bin).is_ok())
        .collect();
    if available.is_empty() {
        anyhow::bail!("None of claude / codex / gemini found on PATH. Install one, then re-run.");
    }

    // 4. Choose which to install into (default: all detected).
    let labels: Vec<String> = available
        .iter()
        .map(|a| format!("{} ({})", a.label, a.bin))
        .collect();
    let defaults: Vec<bool> = vec![true; available.len()];
    let chosen = MultiSelect::with_theme(&theme)
        .with_prompt("Install ytdl-rmcp into which agents? (space to toggle, enter to confirm)")
        .items(&labels)
        .defaults(&defaults)
        .interact()?;
    if chosen.is_empty() {
        eprintln!("Nothing selected; exiting.");
        return Ok(());
    }

    // 5. Register into each selected CLI.
    let self_path = std::env::current_exe().context("resolve own path")?;
    let self_path = self_path.to_string_lossy().to_string();
    let envs = registration_envs(target_path, video_target_path);

    eprintln!();
    // `register` shells out via blocking `std::process::Command`; run the whole
    // batch on a blocking thread so we never stall a tokio worker. Each result
    // is reported back so the per-CLI ✓/✗ output is preserved.
    let chosen_agents: Vec<&'static Agent> = chosen.iter().map(|&idx| available[idx]).collect();
    let chosen_len = chosen.len();
    let self_path_owned = self_path.clone();
    let envs_owned = envs.clone();
    let results = tokio::task::spawn_blocking(move || {
        chosen_agents
            .into_iter()
            .map(|agent| (agent, register(agent, &self_path_owned, &envs_owned)))
            .collect::<Vec<_>>()
    })
    .await?;

    let mut failures = 0;
    for (agent, result) in results {
        match result {
            Ok(()) => eprintln!("  ✓ registered with {}", agent.label),
            Err(e) => {
                eprintln!("  ✗ {} failed: {e}", agent.label);
                failures += 1;
            }
        }
    }

    if failures > 0 {
        // Non-zero exit so automation/users don't mistake a failed install for success.
        anyhow::bail!("{failures} of {chosen_len} agent registration(s) failed");
    }
    eprintln!("\nDone. Restart each agent to pick up the new MCP server.");
    Ok(())
}

fn registration_envs(target_path: String, video_target_path: String) -> Vec<(String, String)> {
    let allow_local_targets = is_local_target(&target_path)
        || (!video_target_path.trim().is_empty() && is_local_target(&video_target_path));
    let mut envs: Vec<(String, String)> = vec![
        ("YTDLP_TARGET_PATH".into(), target_path),
        ("YTDLP_EXTRACTOR_ARGS".into(), DEFAULT_EXTRACTOR_ARGS.into()),
    ];
    if !video_target_path.trim().is_empty() {
        envs.push(("YTDLP_VIDEO_TARGET_PATH".into(), video_target_path));
    }
    if allow_local_targets {
        envs.push(("YTDLP_ALLOW_LOCAL_TARGETS".into(), "true".into()));
    }
    envs
}

fn is_local_target(raw: &str) -> bool {
    let raw = raw.trim();
    raw.starts_with('/') || is_windows_absolute_path(raw)
}

fn is_windows_absolute_path(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    raw.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\'))
}

/// Build and run the CLI's `mcp add`. Argument ordering differs per CLI because
/// of how each parses repeated/variadic env flags vs the trailing command:
///   claude: `mcp add -s user <name> -e K=V… -- <cmd>`
///   codex:  `mcp add --env K=V… <name> -- <cmd>`
///   gemini: `mcp add -s user <name> <cmd> -e K=V…`  (env array goes last)
fn register(agent: &Agent, self_path: &str, envs: &[(String, String)]) -> Result<()> {
    let args = build_mcp_add_args(agent.bin, SERVER_NAME, self_path, envs);
    let out = Command::new(agent.bin)
        .args(&args)
        .output()
        .with_context(|| format!("run {} mcp add", agent.bin))?;
    if !out.status.success() {
        anyhow::bail!("{}", crate::util::command_error(&out));
    }
    Ok(())
}

/// Pure construction of the `mcp add` argument vector (everything after the
/// `agent.bin` program name) for a given CLI. The ordering is position-sensitive
/// and differs per CLI — see [`register`]'s doc comment. Extracted as a pure
/// function so the per-CLI ordering can be asserted in tests.
fn build_mcp_add_args(bin: &str, name: &str, cmd: &str, envs: &[(String, String)]) -> Vec<String> {
    let mut args: Vec<String> = vec!["mcp".into(), "add".into()];
    let env_pairs = || envs.iter().map(|(k, v)| format!("{k}={v}"));
    match bin {
        "codex" => {
            for p in env_pairs() {
                args.push("--env".into());
                args.push(p);
            }
            args.push(name.into());
            args.push("--".into());
            args.push(cmd.into());
        }
        "gemini" => {
            args.push("-s".into());
            args.push("user".into());
            args.push(name.into());
            args.push(cmd.into());
            for p in env_pairs() {
                args.push("-e".into());
                args.push(p);
            }
        }
        _ => {
            // claude (and the default shape)
            args.push("-s".into());
            args.push("user".into());
            args.push(name.into());
            for p in env_pairs() {
                args.push("-e".into());
                args.push(p);
            }
            args.push("--".into());
            args.push(cmd.into());
        }
    }
    args
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
