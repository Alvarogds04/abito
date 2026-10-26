//! Bounded Finder discovery for user-managed macOS runtimes; no shell execution.
use std::{fs, io::Read, path::{Path, PathBuf}};

const MAX_VERSIONS: usize = 128;

pub(crate) fn candidates(name: &str, home: &Path) -> Vec<PathBuf> {
    if !home.is_absolute() || !matches!(name, "node" | "pi") { return Vec::new() }
    let mut paths = vec![home.join(".volta/bin").join(name)];
    if let Some(bin) = nvm_bin(home) { paths.push(bin.join(name)); }
    paths
}

fn version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
    let mut number = || {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) { return None }
        part.parse::<u32>().ok()
    };
    let tuple = (number()?, number()?, number()?);
    if parts.next().is_some() { return None }
    Some(tuple)
}

fn alias(path: &Path) -> Option<String> {
    if !fs::symlink_metadata(path).ok()?.is_file() { return None }
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.take(257).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 256 { return None }
    Some(String::from_utf8(bytes).ok()?.trim().to_owned())
}

fn nvm_bin(home: &Path) -> Option<PathBuf> {
    let nvm = home.join(".nvm");
    let versions = nvm.join("versions/node");
    if let Some(mut value) = alias(&nvm.join("alias/default")) {
        // One additional alias file only; reject absolute paths, traversal and
        // shell syntax. NVM's common lts/<name> aliases are ordinary files.
        if version(&value).is_none() && value.split('/').count() <= 2
            && value.split('/').all(|p| !p.is_empty() && p != "." && p != ".."
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'*'))) {
            value = alias(&nvm.join("alias").join(&value)).unwrap_or_default();
        }
        if version(&value).is_some() {
            let name = if value.starts_with('v') { value } else { format!("v{value}") };
            let bin = versions.join(name).join("bin");
            if bin.is_dir() { return Some(bin) }
        }
    }
    let entries: Vec<_> = fs::read_dir(versions).ok()?.take(MAX_VERSIONS + 1).collect();
    if entries.len() > MAX_VERSIONS { return None }
    entries.into_iter().filter_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_str()?;
        if !name.starts_with('v') { return None }
        let v = version(name)?;
        let bin = entry.path().join("bin");
        bin.is_dir().then_some((v, bin))
    }).max_by_key(|(v, _)| *v).map(|(_, bin)| bin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("abito-nvm-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn version(&self, version: &str) -> PathBuf {
            let bin = self.0.join(".nvm/versions/node").join(version).join("bin");
            fs::create_dir_all(&bin).unwrap();
            bin
        }
        fn alias(&self, name: &str, value: &str) {
            let path = self.0.join(".nvm/alias").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        }
    }
    impl Drop for Home { fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); } }

    #[test]
    fn finder_adds_volta_and_exact_nvm_default_for_node_and_pi() {
        let home = Home::new();
        let default = home.version("v22.1.0");
        home.version("v24.0.0");
        home.alias("default", "v22.1.0\n");
        for name in ["node", "pi"] {
            assert_eq!(candidates(name, &home.0), vec![home.0.join(".volta/bin").join(name), default.join(name)]);
        }
        assert!(candidates("other", &home.0).is_empty());
    }

    #[test]
    fn finder_resolves_one_alias_level_then_uses_highest_numeric_version() {
        let home = Home::new();
        let selected = home.version("v22.9.0");
        let highest = home.version("v22.10.0");
        home.version("v9.99.0");
        home.version("v999.bad.0");
        home.alias("default", "lts/jod\n");
        home.alias("lts/jod", "22.9.0\n");
        assert_eq!(nvm_bin(&home.0), Some(selected));
        // No recursive aliases or shell expressions, even if a matching path exists.
        for value in ["stable", "../../outside", "$(echo v999.0.0)", "v404.0.0"] {
            home.alias("lts/jod", value);
            assert_eq!(nvm_bin(&home.0), Some(highest.clone()));
        }
        fs::remove_file(home.0.join(".nvm/alias/default")).unwrap();
        assert_eq!(nvm_bin(&home.0), Some(highest));
    }

    #[test]
    fn finder_empty_home_is_bounded_and_never_uses_relative_home() {
        let home = Home::new();
        assert_eq!(nvm_bin(&home.0), None);
        assert_eq!(candidates("node", &home.0), vec![home.0.join(".volta/bin/node")]);
        assert!(candidates("node", std::path::Path::new("relative")).is_empty());
        for n in 0..129 { home.version(&format!("v1.0.{n}")); }
        assert_eq!(nvm_bin(&home.0), None, "overflow fails closed instead of scanning unbounded versions");
    }
}
