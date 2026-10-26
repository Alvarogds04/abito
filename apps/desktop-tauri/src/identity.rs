//! M4-PRE: where the app's identity comes from. A dev (debug) build derives
//! its project from the source checkout it was built in (the legacy dev
//! layout); a release/packaged binary carries NO build-tree path at all and
//! takes the project root only from an explicit, validated setting — or
//! refuses, so the transport says "unavailable" instead of pretending.
use std::path::PathBuf;

/// Installed identity (tauri.conf.json `identifier`), never the Preview's.
pub const APP_IDENTIFIER: &str = "com.abitog.AbitoTauri";
/// Env var the launcher/config sets for an installed app (absolute path).
pub const PROJECT_ROOT_ENV: &str = "ABITO_PROJECT_ROOT";

/// Build-tree path baked by cargo — debug builds only.
#[cfg(debug_assertions)]
pub fn dev_manifest_dir() -> Option<&'static str> {
    option_env!("CARGO_MANIFEST_DIR")
}
#[cfg(not(debug_assertions))]
pub fn dev_manifest_dir() -> Option<&'static str> {
    None
}

fn absolute_normalized(text: &str) -> bool {
    text.starts_with('/')
        && text
            .split('/')
            .skip(1)
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Pure choice of the canonical project root.
/// - `manifest = Some(dir)` (dev): `<dir>/../..`, the worktree root.
/// - `manifest = None` (installed): `env_root` must be absolute and
///   normalized; anything else yields `None`.
pub fn project_root_for(manifest: Option<&str>, env_root: Option<&str>) -> Option<PathBuf> {
    match manifest {
        Some(dir) => {
            let apps = std::fs::canonicalize(dir).ok()?;
            Some(apps.parent()?.parent()?.to_path_buf())
        }
        None => {
            let text = env_root.filter(|t| absolute_normalized(t))?;
            std::fs::canonicalize(text).ok().filter(|p| p.is_dir())
        }
    }
}

/// Project id of this process (sha256 of the canonical root), or `None`.
pub fn project_id_of_this_process() -> Option<String> {
    let env_root = std::env::var(PROJECT_ROOT_ENV).ok();
    let root = project_root_for(dev_manifest_dir(), env_root.as_deref())?;
    Some(crate::protocol::project_id(root.to_str()?))
}
