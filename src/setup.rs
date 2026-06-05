//! `ytdl-mcp setup` — interactive installer.
//!
//! 1. Ensure yt-dlp + ffmpeg are installed (into the cache dir).
//! 2. Prompt for the SSH remote + audio/video destinations.
//! 3. Detect which agent CLIs (claude/codex/gemini) are present.
//! 4. Register this binary into the selected ones via their `mcp add` commands
//!    (rather than hand-editing any JSON/TOML config).

use std::process::Command;

use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect};

use crate::bootstrap;
use crate::config::Config;

const SERVER_NAME: &str = "ytdl-mcp";

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
    eprintln!("ytdl-mcp setup\n");

    // 1. Install/verify yt-dlp + ffmpeg.
    eprintln!("Checking yt-dlp + ffmpeg…");
    let cfg = Config::from_env();
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
    let remote: String = Input::with_theme(&theme)
        .with_prompt("SSH remote (alias or user@host)")
        .interact_text()?;
    let audio_dest: String = Input::with_theme(&theme)
        .with_prompt("Audio destination (absolute remote dir)")
        .interact_text()?;
    let video_dest: String = Input::with_theme(&theme)
        .with_prompt("Video destination (blank = same as audio)")
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
        .with_prompt("Install ytdl-mcp into which agents? (space to toggle, enter to confirm)")
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
    let mut envs: Vec<(String, String)> = vec![
        ("YTDLP_REMOTE".into(), remote),
        ("YTDLP_REMOTE_PATH".into(), audio_dest),
    ];
    if !video_dest.trim().is_empty() {
        envs.push(("YTDLP_VIDEO_REMOTE_PATH".into(), video_dest));
    }

    eprintln!();
    for &idx in &chosen {
        let agent = available[idx];
        match register(agent, &self_path, &envs) {
            Ok(()) => eprintln!("  ✓ registered with {}", agent.label),
            Err(e) => eprintln!("  ✗ {} failed: {e}", agent.label),
        }
    }

    eprintln!("\nDone. Restart each agent to pick up the new MCP server.");
    Ok(())
}

/// Build and run the CLI's `mcp add`. Argument ordering differs per CLI because
/// of how each parses repeated/variadic env flags vs the trailing command:
///   claude: `mcp add -s user <name> -e K=V… -- <cmd>`
///   codex:  `mcp add --env K=V… <name> -- <cmd>`
///   gemini: `mcp add -s user <name> <cmd> -e K=V…`  (env array goes last)
fn register(agent: &Agent, self_path: &str, envs: &[(String, String)]) -> Result<()> {
    let mut cmd = Command::new(agent.bin);
    cmd.arg("mcp").arg("add");
    let env_pairs = || envs.iter().map(|(k, v)| format!("{k}={v}"));
    match agent.bin {
        "codex" => {
            for p in env_pairs() {
                cmd.arg("--env").arg(p);
            }
            cmd.arg(SERVER_NAME).arg("--").arg(self_path);
        }
        "gemini" => {
            cmd.args(["-s", "user"]).arg(SERVER_NAME).arg(self_path);
            for p in env_pairs() {
                cmd.arg("-e").arg(p);
            }
        }
        _ => {
            // claude (and the default shape)
            cmd.args(["-s", "user"]).arg(SERVER_NAME);
            for p in env_pairs() {
                cmd.arg("-e").arg(p);
            }
            cmd.arg("--").arg(self_path);
        }
    }

    let out = cmd
        .output()
        .with_context(|| format!("run {} mcp add", agent.bin))?;
    if !out.status.success() {
        anyhow::bail!("{}", crate::util::command_error(&out));
    }
    Ok(())
}
