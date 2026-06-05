//! YouTube mix/radio URL cleaning.
//!
//! Ports `_strip_mix_params` from the Python `models.py`: a YouTube *mix*
//! (`list=RD…`) embeds a real `v=` video but wraps it in an auto-generated
//! playlist that yt-dlp resolves first — landing on a different, often
//! unavailable, video. Stripping the mix params sends yt-dlp straight to the
//! intended seed video. Real playlists (`list=PL…`) are left untouched.

/// Hosts we treat as YouTube for mix-cleaning purposes.
const YT_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
];

/// `list=` prefixes that denote an auto-generated mix/radio, not a real playlist.
const MIX_PREFIXES: &[&str] = &["RD", "RM", "WL"];

/// Query params that are only meaningful inside a mix/radio session.
const MIX_PARAMS: &[&str] = &["list", "start_radio", "index", "pp"];

/// Return a cleaned URL when `url` is a YouTube mix/radio link, else the input
/// unchanged. Only the query string is rewritten; scheme/host/path are kept.
pub fn strip_mix_params(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return url.to_string();
    };
    let host = parsed.host_str().unwrap_or("");
    if !YT_HOSTS.contains(&host) {
        return url.to_string();
    }

    // Does a `list=` param start with a mix prefix?
    let is_mix = parsed
        .query_pairs()
        .find(|(k, _)| k == "list")
        .map(|(_, v)| MIX_PREFIXES.iter().any(|p| v.starts_with(p)))
        .unwrap_or(false);
    if !is_mix {
        return url.to_string();
    }

    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !MIX_PARAMS.contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    let mut out = parsed.clone();
    out.set_query(None);
    if !kept.is_empty() {
        out.query_pairs_mut().extend_pairs(kept);
    }
    out.to_string()
}

#[cfg(test)]
#[path = "urls_tests.rs"]
mod tests;
