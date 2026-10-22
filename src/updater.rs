//! Self update from the GitHub releases of the repository.
//!
//! For every target, the release workflow (`.github/workflows/publish.yml`)
//! publishes the bare binary as `scout4all-<target>` (`.exe` on windows),
//! plus the archives for manual downloads (`.tar.gz`, and `.zip` on
//! windows), which contain just `scout4all` (`scout4all.exe`) and its SBOM.
//! Older clients pick the first asset whose name contains their target, so
//! no other asset may carry the target in its name: the SBOM, which used to
//! be published as `scout4all-<target>.cdx.json`, was installed in place of
//! the executable.

use scout4all_core::{localization::current_labels, logging::logger::log_error};
use self_update::{
    Error, ReleaseAsset, VersionStatus, backends::github::UpdateBuilder, cargo_crate_version,
    errors::Result, restart,
};
use std::{env::consts::EXE_SUFFIX, path::Path, process::Command};

const BIN_NAME: &str = env!("CARGO_PKG_NAME");
const GITHUB_API_URL: &str = "https://api.github.com";

/// The name of the release asset holding the bare binary for `target`.
pub fn binary_asset_name(target: &str) -> String {
    format!("{BIN_NAME}-{target}{EXE_SUFFIX}")
}

fn repository() -> (&'static str, &'static str) {
    env!("CARGO_PKG_REPOSITORY")
        .trim_start_matches("https://github.com/")
        .split_once('/')
        .unwrap_or(("naighes", "scoutforall"))
}

/// Accepts the downloaded binary only if it runs and reports itself as
/// scout4all, so that a wrong asset can never replace the installed one.
fn verify_binary(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        // the bare binary is downloaded without the executable bit
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| Error::verification_rejected(format!("cannot run the new binary: {e}")))
        .and_then(|output| {
            match (
                output.status.success(),
                String::from_utf8_lossy(&output.stdout).starts_with(&format!("{BIN_NAME} ")),
            ) {
                (true, true) => Ok(()),
                _ => Err(Error::verification_rejected(
                    "the new binary is not a scout4all executable",
                )),
            }
        })
}

/// The updater configuration shared by the application and the tests.
fn configure(api_base_url: &str, current_version: &str) -> UpdateBuilder {
    let (repo_owner, repo_name) = repository();
    let asset_name = binary_asset_name(self_update::get_target());
    let mut builder = self_update::backends::github::Update::configure();
    builder
        .api_base_url(api_base_url)
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .bin_name(BIN_NAME)
        .current_version(current_version)
        .asset_matcher(move |assets: &[ReleaseAsset]| {
            assets.iter().find(|a| a.name() == asset_name).cloned()
        })
        .verify_binary(verify_binary);
    builder
}

/// Updates the running executable to the latest release, after asking the
/// user, and restarts into the new version.
pub fn check_update() {
    let labels = current_labels();
    let status = configure(GITHUB_API_URL, cargo_crate_version!())
        .show_download_progress(true)
        .build()
        .and_then(|u| u.update());
    match status {
        Ok(VersionStatus::Updated(version)) => {
            println!("{} {}", labels.updated_to_version, version);
            // the process still runs the old code: replace it with the new
            // binary; if that fails, the new version starts on next launch
            let Err(err) = restart::restart();
            log_error(&format!("cannot restart after the update: {err}"));
        }
        Ok(_) | Err(Error::Aborted) => {}
        Err(err) => {
            log_error(&format!("self update failed: {err}"));
            println!("{}: {}", labels.update_check_failed, err);
        }
    }
}

// apart, so that clippy sees a test module (see clippy.toml)
#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        path::PathBuf,
        thread,
    };
    use tempfile::TempDir;

    const NEW_VERSION: &str = "9.9.9";
    const SBOM: &[u8] = br#"{"bomFormat":"CycloneDX"}"#;

    fn fake_binary(version: &str) -> Vec<u8> {
        format!("#!/bin/sh\necho \"{BIN_NAME} {version}\"\n").into_bytes()
    }

    /// An archive as built by the release workflow: the bare binary and its SBOM.
    fn archive(dir: &Path, name: &str, binary: &[u8]) -> Vec<u8> {
        let content = dir.join(format!("{name}.content"));
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join(BIN_NAME), binary).unwrap();
        fs::write(content.join(format!("{BIN_NAME}.cdx.json")), SBOM).unwrap();
        let path = dir.join(name);
        let status = Command::new("tar")
            .arg("czf")
            .arg(&path)
            .arg("-C")
            .arg(&content)
            .args([BIN_NAME, &format!("{BIN_NAME}.cdx.json")])
            .status()
            .unwrap();
        assert!(status.success());
        fs::read(path).unwrap()
    }

    /// The assets the release workflow publishes for the current target.
    fn published_assets(dir: &Path, binary: &[u8]) -> Vec<(String, Vec<u8>)> {
        let target = self_update::get_target();
        let archive_name = format!("{BIN_NAME}-{target}.tar.gz");
        vec![
            (binary_asset_name(target), binary.to_vec()),
            (archive_name.clone(), archive(dir, &archive_name, binary)),
        ]
    }

    /// The assets published up to v0.4.2, listed in the order of the API.
    fn legacy_assets(dir: &Path, binary: &[u8]) -> Vec<(String, Vec<u8>)> {
        let target = self_update::get_target();
        let archive_name = format!("{BIN_NAME}-{target}.tar.gz");
        let mut versioned = binary.to_vec();
        versioned.extend_from_slice(b"# versioned name in the archive\n");
        vec![
            (format!("{BIN_NAME}-{target}.cdx.json"), SBOM.to_vec()),
            (
                archive_name.clone(),
                archive(dir, &archive_name, &versioned),
            ),
        ]
    }

    /// Serves a single release, carrying `assets`, as the GitHub API does.
    fn serve_release(assets: Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let release = serde_json::json!([{
            "tag_name": format!("v{NEW_VERSION}"),
            "created_at": "2026-10-18T00:00:00Z",
            "name": format!("v{NEW_VERSION}"),
            "body": "",
            "html_url": format!("{base_url}/release"),
            "assets": assets
                .iter()
                .enumerate()
                .map(|(i, (name, _))| serde_json::json!({
                    "name": name,
                    "url": format!("{base_url}/assets/{i}"),
                }))
                .collect::<Vec<_>>(),
        }])
        .to_string()
        .into_bytes();
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request_line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut request_line).unwrap();
                // skip the headers
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or_default();
                let body = match path.strip_prefix("/assets/") {
                    Some(i) => &assets[i.parse::<usize>().unwrap()].1,
                    None => &release,
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
        base_url
    }

    /// The executable of an installed v0.4.2.
    fn installation(dir: &TempDir) -> PathBuf {
        let exe = dir.path().join("installed").join(BIN_NAME);
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, fake_binary("0.4.2")).unwrap();
        exe
    }

    fn update(mut builder: UpdateBuilder, exe: &Path) -> Result<VersionStatus> {
        builder
            .bin_install_path(exe)
            .no_confirm(true)
            .show_output(false)
            .show_download_progress(false)
            .build()
            .and_then(|u| u.update())
    }

    /// The configuration of the clients released up to v0.4.2.
    fn legacy_client(api_base_url: &str) -> UpdateBuilder {
        let (repo_owner, repo_name) = repository();
        let mut builder = self_update::backends::github::Update::configure();
        builder
            .api_base_url(api_base_url)
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name(BIN_NAME)
            .current_version("0.4.2");
        builder
    }

    #[test]
    fn installs_the_published_binary() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let binary = fake_binary(NEW_VERSION);
        let url = serve_release(published_assets(dir.path(), &binary));
        let status = update(configure(&url, "0.4.2"), &exe).unwrap();
        assert!(matches!(status, VersionStatus::Updated(v) if v == NEW_VERSION));
        assert_eq!(fs::read(&exe).unwrap(), binary);
    }

    #[test]
    fn up_to_date_leaves_the_installation_untouched() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let url = serve_release(published_assets(dir.path(), &fake_binary(NEW_VERSION)));
        let status = update(configure(&url, NEW_VERSION), &exe).unwrap();
        assert!(status.is_up_to_date());
        assert_eq!(fs::read(&exe).unwrap(), fake_binary("0.4.2"));
    }

    #[test]
    fn never_installs_the_sbom_of_a_legacy_release() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let url = serve_release(legacy_assets(dir.path(), &fake_binary(NEW_VERSION)));
        assert!(update(configure(&url, "0.4.2"), &exe).is_err());
        assert_eq!(fs::read(&exe).unwrap(), fake_binary("0.4.2"));
    }

    #[test]
    fn rejects_an_asset_that_is_not_scout4all() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let target = self_update::get_target();
        let url = serve_release(vec![(binary_asset_name(target), SBOM.to_vec())]);
        let err = update(configure(&url, "0.4.2"), &exe).unwrap_err();
        assert!(matches!(err, Error::VerificationRejected { .. }));
        assert_eq!(fs::read(&exe).unwrap(), fake_binary("0.4.2"));
    }

    #[test]
    fn legacy_client_installs_the_sbom_of_a_legacy_release() {
        // the bug fixed by the new release layout: kept to prove the
        // scenario the other legacy tests guard against is real
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let url = serve_release(legacy_assets(dir.path(), &fake_binary(NEW_VERSION)));
        update(legacy_client(&url), &exe).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), SBOM);
    }

    #[test]
    fn legacy_client_installs_the_published_binary() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let binary = fake_binary(NEW_VERSION);
        let url = serve_release(published_assets(dir.path(), &binary));
        let status = update(legacy_client(&url), &exe).unwrap();
        assert!(matches!(status, VersionStatus::Updated(v) if v == NEW_VERSION));
        assert_eq!(fs::read(&exe).unwrap(), binary);
    }

    #[test]
    fn legacy_client_installs_the_binary_from_the_published_archive() {
        // whichever asset the API lists first, a legacy client gets the binary
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let binary = fake_binary(NEW_VERSION);
        let assets = published_assets(dir.path(), &binary)
            .into_iter()
            .rev()
            .collect();
        let url = serve_release(assets);
        update(legacy_client(&url), &exe).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), binary);
    }
}
