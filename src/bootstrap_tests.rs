use super::*;

#[test]
fn exe_name_matches_platform() {
    let name = exe_name("yt-dlp");
    if cfg!(target_os = "windows") {
        assert_eq!(name, "yt-dlp.exe");
    } else {
        assert_eq!(name, "yt-dlp");
    }
}

#[test]
fn cache_bin_dir_ends_in_bin() {
    assert!(cache_bin_dir().ends_with("bin"));
}

#[test]
fn resolve_override_errors_on_missing_path() {
    let r = resolve_override_or_path(Some("/no/such/binary/xyz"), "FAKE_PATH", "yt-dlp");
    assert!(r.is_err());
}
