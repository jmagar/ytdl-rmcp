use super::*;

fn sample_envs() -> Vec<(String, String)> {
    vec![
        ("YTDLP_TARGET_PATH".to_string(), "nas:/music".to_string()),
        (
            "YTDLP_EXTRACTOR_ARGS".to_string(),
            "youtube:player_client=android".to_string(),
        ),
    ]
}

#[test]
fn registration_envs_include_required_download_and_extractor_defaults() {
    assert_eq!(
        registration_envs("tootie:/music".into(), "tootie:/movies".into()),
        vec![
            ("YTDLP_TARGET_PATH".to_string(), "tootie:/music".to_string()),
            (
                "YTDLP_EXTRACTOR_ARGS".to_string(),
                "youtube:player_client=android".to_string(),
            ),
            (
                "YTDLP_VIDEO_TARGET_PATH".to_string(),
                "tootie:/movies".to_string(),
            ),
        ]
    );
}

#[test]
fn registration_envs_omit_blank_video_destination() {
    let envs = registration_envs("tootie:/music".into(), "   ".into());
    assert!(!envs.iter().any(|(key, _)| key == "YTDLP_VIDEO_TARGET_PATH"));
    assert!(envs.iter().any(|(key, value)| {
        key == "YTDLP_EXTRACTOR_ARGS" && value == "youtube:player_client=android"
    }));
}

#[test]
fn registration_envs_enable_local_targets_when_prompt_uses_local_path() {
    let envs = registration_envs("/media/music".into(), "   ".into());

    assert!(envs
        .iter()
        .any(|(key, value)| key == "YTDLP_ALLOW_LOCAL_TARGETS" && value == "true"));
}

#[test]
fn claude_places_env_flags_before_separator_and_cmd_after() {
    let envs = sample_envs();
    let args = build_mcp_add_args("claude", "ytdl-rmcp", "/usr/bin/ytdl-rmcp", &envs);
    assert_eq!(
        args,
        vec![
            "mcp",
            "add",
            "-s",
            "user",
            "ytdl-rmcp",
            "-e",
            "YTDLP_TARGET_PATH=nas:/music",
            "-e",
            "YTDLP_EXTRACTOR_ARGS=youtube:player_client=android",
            "--",
            "/usr/bin/ytdl-rmcp",
        ]
    );

    // The `--` separator must come after every `-e` flag and immediately before
    // the command, so env values are never parsed as the trailing command.
    let sep = args.iter().position(|a| a == "--").unwrap();
    let last_e = args.iter().rposition(|a| a == "-e").unwrap();
    assert!(
        last_e < sep,
        "claude: -e flags must precede the -- separator"
    );
    assert_eq!(args.last().unwrap(), "/usr/bin/ytdl-rmcp");
    assert_eq!(args[sep + 1], "/usr/bin/ytdl-rmcp");
}

#[test]
fn codex_uses_env_flag_before_name() {
    let envs = sample_envs();
    let args = build_mcp_add_args("codex", "ytdl-rmcp", "/usr/bin/ytdl-rmcp", &envs);
    assert_eq!(
        args,
        vec![
            "mcp",
            "add",
            "--env",
            "YTDLP_TARGET_PATH=nas:/music",
            "--env",
            "YTDLP_EXTRACTOR_ARGS=youtube:player_client=android",
            "ytdl-rmcp",
            "--",
            "/usr/bin/ytdl-rmcp",
        ]
    );

    // codex uses `--env` (not `-e`) and every `--env` flag must come BEFORE the
    // server name positional.
    assert!(
        !args.iter().any(|a| a == "-e"),
        "codex must use --env, not -e"
    );
    let name = args.iter().position(|a| a == "ytdl-rmcp").unwrap();
    let last_env = args.iter().rposition(|a| a == "--env").unwrap();
    assert!(last_env < name, "codex: --env flags must precede the name");
}

#[test]
fn gemini_places_env_flags_after_name_and_cmd() {
    let envs = sample_envs();
    let args = build_mcp_add_args("gemini", "ytdl-rmcp", "/usr/bin/ytdl-rmcp", &envs);
    assert_eq!(
        args,
        vec![
            "mcp",
            "add",
            "-s",
            "user",
            "ytdl-rmcp",
            "/usr/bin/ytdl-rmcp",
            "-e",
            "YTDLP_TARGET_PATH=nas:/music",
            "-e",
            "YTDLP_EXTRACTOR_ARGS=youtube:player_client=android",
        ]
    );

    // gemini puts the env (`-e`) flags LAST — after both the name and command
    // positionals, and there is no `--` separator.
    assert!(
        !args.iter().any(|a| a == "--"),
        "gemini uses no -- separator"
    );
    let name = args.iter().position(|a| a == "ytdl-rmcp").unwrap();
    let cmd = args.iter().position(|a| a == "/usr/bin/ytdl-rmcp").unwrap();
    let first_e = args.iter().position(|a| a == "-e").unwrap();
    assert!(name < cmd, "gemini: name must precede command");
    assert!(
        cmd < first_e,
        "gemini: env flags must come after the command"
    );
}

#[test]
fn unknown_bin_falls_back_to_claude_shape() {
    let envs = sample_envs();
    let claude = build_mcp_add_args("claude", "ytdl-rmcp", "/bin/x", &envs);
    let other = build_mcp_add_args("something-else", "ytdl-rmcp", "/bin/x", &envs);
    assert_eq!(claude, other);
}

#[test]
fn no_envs_produces_minimal_argv_per_cli() {
    let envs: Vec<(String, String)> = vec![];
    assert_eq!(
        build_mcp_add_args("claude", "ytdl-rmcp", "/bin/x", &envs),
        vec!["mcp", "add", "-s", "user", "ytdl-rmcp", "--", "/bin/x"]
    );
    assert_eq!(
        build_mcp_add_args("codex", "ytdl-rmcp", "/bin/x", &envs),
        vec!["mcp", "add", "ytdl-rmcp", "--", "/bin/x"]
    );
    assert_eq!(
        build_mcp_add_args("gemini", "ytdl-rmcp", "/bin/x", &envs),
        vec!["mcp", "add", "-s", "user", "ytdl-rmcp", "/bin/x"]
    );
}
