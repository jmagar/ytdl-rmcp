//! Minimal blocking HTTP download (ureq + rustls/ring — cross-compile clean).

use std::fs::File;
use std::io;
use std::path::Path;

use anyhow::{bail, Context, Result};

/// Download `url` to `dest` atomically (via a temp file + rename). Follows
/// redirects (GitHub release `latest/download` links redirect to the asset).
pub fn download_to_file(url: &str, dest: &Path) -> Result<()> {
    let mut res = ureq::get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    if res.status() != 200 {
        bail!("GET {url} returned HTTP {}", res.status());
    }

    let tmp = dest.with_extension("part");
    {
        let mut out = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        let mut reader = res.body_mut().as_reader();
        io::copy(&mut reader, &mut out).with_context(|| format!("write {}", tmp.display()))?;
    }
    std::fs::rename(&tmp, dest).with_context(|| format!("rename into {}", dest.display()))?;
    Ok(())
}
