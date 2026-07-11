use super::*;

use std::ffi::OsString;

#[test]
fn remote_mkdir_command_quotes_shell_sensitive_paths() {
    // All inputs are valid absolute paths (a leading-dash path is rejected by
    // RemotePath::parse and is covered separately below); the point here is
    // that shell-sensitive characters in an accepted path are quoted safely.
    let cases = [
        ("/media/music library", "mkdir -p -- '/media/music library'"),
        ("/media/O'Brien", "mkdir -p -- '/media/O'\\''Brien'"),
        (
            "/media/a; touch pwned",
            "mkdir -p -- '/media/a; touch pwned'",
        ),
        (
            "/media/$(touch pwned)",
            "mkdir -p -- '/media/$(touch pwned)'",
        ),
        ("/-dash/child", "mkdir -p -- '/-dash/child'"),
    ];

    for (raw, expected) in cases {
        let path = RemotePath::parse(raw).unwrap();
        assert_eq!(remote_mkdir_command(&path), expected);
    }
}

#[test]
fn remote_path_accepts_normal_absolute_path() {
    assert_eq!(
        RemotePath::parse("/srv/music").unwrap().as_str(),
        "/srv/music"
    );
    // Embedded spaces are fine; they are shell-quoted downstream.
    assert_eq!(
        RemotePath::parse("/media/music library").unwrap().as_str(),
        "/media/music library"
    );
}

#[test]
fn remote_path_rejects_unsafe_values_with_typed_errors() {
    use TransferValidationError as E;
    let field = "remote destination path";

    let cases: [(&str, E); 7] = [
        ("", E::Empty { field }),
        ("   ", E::Empty { field }),
        ("/media/\u{7}bell", E::BadChars { field }),
        ("-oProxyCommand=sh", E::LeadingDash { field }),
        ("relative/path", E::NotAbsolute { field }),
        ("music", E::NotAbsolute { field }),
        ("/music/../../etc", E::Traversal { field }),
    ];

    for (raw, expected) in cases {
        let err = RemotePath::parse_typed(raw).expect_err(&format!("{raw:?} should be rejected"));
        assert_eq!(err, expected, "wrong variant for {raw:?}");
    }
}

#[test]
fn remote_path_rejects_traversal_and_relative_via_public_parse() {
    // Public anyhow-returning surface still rejects these.
    assert!(RemotePath::parse("/music/../etc").is_err());
    assert!(RemotePath::parse("..").is_err());
    assert!(RemotePath::parse("~/.config/autostart").is_err());
    assert!(RemotePath::parse("-leading").is_err());
}

#[test]
fn remote_rejects_option_like_empty_whitespace_and_control_values() {
    for raw in [
        "",
        "   ",
        "-oProxyCommand=sh",
        "host name",
        "host\tname",
        "host\nname",
    ] {
        assert!(
            RemoteSpec::parse(raw).is_err(),
            "{raw:?} should be rejected"
        );
    }
}

#[test]
fn remote_accepts_common_ssh_aliases_and_user_hosts() {
    for raw in [
        "nas",
        "music-box",
        "user@example.com",
        "user.name@host.local:2222",
    ] {
        assert_eq!(RemoteSpec::parse(raw).unwrap().as_str(), raw);
    }
}

#[test]
fn transfer_target_validates_all_boundaries_once() {
    let target = TransferTarget::parse_targets("nas:/audio library", Some("nas:/videos")).unwrap();
    match target.audio_target() {
        TargetPath::Ssh { remote, path } => {
            assert_eq!(remote.as_str(), "nas");
            assert_eq!(path.as_str(), "/audio library");
        }
        other => panic!("expected ssh audio target, got {other:?}"),
    }
    match target.video_target() {
        TargetPath::Ssh { remote, path } => {
            assert_eq!(remote.as_str(), "nas");
            assert_eq!(path.as_str(), "/videos");
        }
        other => panic!("expected ssh video target, got {other:?}"),
    }

    assert!(TransferTarget::parse_targets("-bad:/audio", None).is_err());
    assert!(TransferTarget::parse_targets("nas:   ", None).is_err());
    assert!(TransferTarget::parse_targets("nas:\n/audio", None).is_err());
}

#[test]
fn target_path_accepts_local_ssh_and_rclone_targets() {
    assert!(matches!(
        TargetPath::parse("/srv/music").unwrap(),
        TargetPath::Local(_)
    ));

    match TargetPath::parse("dookie:/srv/music").unwrap() {
        TargetPath::Ssh { remote, path } => {
            assert_eq!(remote.as_str(), "dookie");
            assert_eq!(path.as_str(), "/srv/music");
        }
        other => panic!("expected ssh target, got {other:?}"),
    }

    match TargetPath::parse("gdrive:music/ytdl").unwrap() {
        TargetPath::Rclone(target) => assert_eq!(target.as_str(), "gdrive:music/ytdl"),
        other => panic!("expected rclone target, got {other:?}"),
    }

    match TargetPath::parse("rclone:gdrive:/Music/ytdl").unwrap() {
        TargetPath::Rclone(target) => assert_eq!(target.as_str(), "gdrive:/Music/ytdl"),
        other => panic!("expected explicit rclone target, got {other:?}"),
    }

    match TargetPath::parse("ssh:dookie:/srv/music").unwrap() {
        TargetPath::Ssh { remote, path } => {
            assert_eq!(remote.as_str(), "dookie");
            assert_eq!(path.as_str(), "/srv/music");
        }
        other => panic!("expected explicit ssh target, got {other:?}"),
    }
}

#[test]
fn explicit_absolute_rclone_target_display_keeps_disambiguating_prefix() {
    assert_eq!(
        TargetPath::parse("rclone:gdrive:/Music/ytdl")
            .unwrap()
            .display(),
        "rclone:gdrive:/Music/ytdl"
    );
    assert_eq!(
        TargetPath::parse("gdrive:Music/ytdl").unwrap().display(),
        "gdrive:Music/ytdl"
    );
}

#[test]
fn target_path_does_not_treat_windows_drive_paths_as_remotes() {
    #[cfg(windows)]
    {
        match TargetPath::parse("C:/Users/Jacob/Music").unwrap() {
            TargetPath::Local(path) => assert_eq!(path.as_str(), "C:/Users/Jacob/Music"),
            other => panic!("expected local target, got {other:?}"),
        }
    }

    #[cfg(not(windows))]
    assert!(TargetPath::parse("C:/Users/Jacob/Music").is_err());
}

#[test]
fn target_path_rejects_unsafe_targets() {
    assert!(TargetPath::parse("").is_err());
    assert!(TargetPath::parse("relative/path").is_err());
    assert!(TargetPath::parse("/music/../etc").is_err());
    assert!(
        TargetPath::parse("dookie:relative").is_ok(),
        "rclone targets may be relative to a remote"
    );
    assert!(TargetPath::parse("dookie:/music/../etc").is_err());
    assert!(TargetPath::parse("rclone:gdrive:music/../etc").is_err());
    assert!(TargetPath::parse("-bad:/music").is_err());
    assert!(TargetPath::parse("remote:\npath").is_err());
}

#[test]
fn target_set_uses_video_target_or_falls_back_to_audio_target() {
    let target = TransferTarget::parse_targets("/audio", Some("remote:videos")).unwrap();
    assert_eq!(target.audio_target().display(), "/audio");
    assert_eq!(target.video_target().display(), "remote:videos");

    let target = TransferTarget::parse_targets("dookie:/audio", None).unwrap();
    assert_eq!(target.audio_target().display(), "dookie:/audio");
    assert_eq!(target.video_target().display(), "dookie:/audio");
}

#[test]
fn target_set_reports_whether_local_paths_are_present() {
    let local = TransferTarget::parse_targets("/audio", Some("remote:videos")).unwrap();
    assert!(local.contains_local());

    let remote = TransferTarget::parse_targets("dookie:/audio", Some("remote:videos")).unwrap();
    assert!(!remote.contains_local());
}

#[test]
fn rsync_remote_shell_command_quotes_each_ssh_arg() {
    let opts = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-i".to_string(),
        "/home/me/keys/media key".to_string(),
        "-o".to_string(),
        "ProxyCommand=ssh jump 'nc %h %p'".to_string(),
    ];

    assert_eq!(
        rsync_remote_shell_command(&opts),
        "ssh -o BatchMode=yes -i '/home/me/keys/media key' -o 'ProxyCommand=ssh jump '\\''nc %h %p'\\'''"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn dropped_transfer_command_kills_child_process() {
    use std::process::Command as StdCommand;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("child.pid");
    let script = format!("printf $$ > {}; exec sleep 5", pid_path.display());

    let mut cmd = tokio::process::Command::new("sh");
    cmd.args(["-c", &script]);

    let result = tokio::time::timeout(
        Duration::from_millis(100),
        run_capped(&mut cmd, None, Some(STDERR_CAP)),
    )
    .await;
    assert!(result.is_err(), "command should still be sleeping");

    let pid = std::fs::read_to_string(&pid_path)
        .unwrap()
        .trim()
        .to_string();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let alive = StdCommand::new("kill")
        .args(["-0", &pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(
        !alive.success(),
        "timed-out transfer command left child process {pid} alive"
    );
}

#[test]
fn rclone_target_builds_copy_args() {
    let target = TargetPath::parse("gdrive:music/ytdl").unwrap();
    let TargetPath::Rclone(target) = target else {
        panic!("expected rclone target");
    };

    let args = rclone_copy_args(Path::new("/tmp/staging"), &target);
    assert_eq!(
        args,
        vec![
            OsString::from("copy"),
            OsString::from("/tmp/staging"),
            OsString::from("gdrive:music/ytdl"),
            OsString::from("--create-empty-src-dirs"),
        ]
    );
}

#[tokio::test]
async fn local_copy_rejects_destination_inside_source() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    let nested_dest = src.join("nested");
    tokio::fs::create_dir_all(&src).await.unwrap();
    tokio::fs::write(src.join("song.mp3"), b"audio")
        .await
        .unwrap();

    let err = copy_dir_contents(&src, &nested_dest)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("must not be inside source"));
}

#[cfg(unix)]
#[tokio::test]
async fn local_copy_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    let dest = root.path().join("dest");
    tokio::fs::create_dir_all(&src).await.unwrap();
    tokio::fs::write(root.path().join("outside.mp3"), b"audio")
        .await
        .unwrap();
    symlink(
        root.path().join("outside.mp3"),
        src.join("outside-link.mp3"),
    )
    .unwrap();

    let err = copy_dir_contents(&src, &dest)
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("refuses to follow symlink"));
}
