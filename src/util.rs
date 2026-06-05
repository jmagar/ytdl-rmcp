//! Small shared helpers.

use std::process::Output;

/// Best-effort error text from a failed subprocess: trimmed stderr, falling
/// back to stdout when stderr is empty.
pub fn command_error(out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.trim();
    if err.is_empty() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        err.to_string()
    }
}
