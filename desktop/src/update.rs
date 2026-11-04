//! The self update, from the GitHub releases of the repository, as the TUI
//! does it (`src/updater.rs`): the same asset, `scout4all-<target>` (`.exe`
//! on windows), so that the old TUIs update to the desktop app and the app
//! goes on updating itself. At startup the app looks for a newer release;
//! when there's one it asks, in the bar, then downloads it, checks that it
//! runs as scout4all, puts it in place of itself and starts again.
//!
//! Only the builds of the releases update (`self-update`, as the TUI): a
//! build of the sources never replaces itself.

/// The name of the program, as `--version` says it and the assets carry
/// it: the TUI's, which the desktop app takes the place of.
pub const BIN_NAME: &str = "scout4all";

/// What `--version` prints: what the self update of every client checks
/// before installing a binary.
pub fn version_line() -> String {
    format!("{BIN_NAME} {}", scout4all_mobile::version())
}

/// The work of `f` on a thread of its own: the updater blocks on the
/// network, which the screen mustn't wait for.
#[cfg(feature = "self-update")]
async fn on_a_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.ok()
}

#[cfg(feature = "self-update")]
mod release {
    use {
        super::BIN_NAME,
        self_update::{Error, ReleaseAsset, backends::github::UpdateBuilder, errors::Result},
        std::{env::consts::EXE_SUFFIX, path::Path, process::Command},
    };

    const GITHUB_API_URL: &str = "https://api.github.com";
    const REPO_OWNER: &str = "naighes";
    const REPO_NAME: &str = "scoutforall";

    /// The name of the release asset holding the bare binary for `target`.
    pub fn binary_asset_name(target: &str) -> String {
        format!("{BIN_NAME}-{target}{EXE_SUFFIX}")
    }

    /// Accepts the downloaded binary only if it runs and says it's
    /// scout4all: a wrong asset never takes the place of the app.
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

    /// The updater, as the app and the tests configure it.
    pub fn configure(api_base_url: &str, current_version: &str) -> UpdateBuilder {
        let asset_name = binary_asset_name(self_update::get_target());
        let mut builder = self_update::backends::github::Update::configure();
        builder
            .api_base_url(api_base_url)
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name(BIN_NAME)
            .current_version(current_version)
            .asset_matcher(move |assets: &[ReleaseAsset]| {
                assets.iter().find(|a| a.name() == asset_name).cloned()
            })
            .verify_binary(verify_binary)
            // asked in the bar of the app, not on the terminal
            .no_confirm(true)
            .show_output(false)
            .show_download_progress(false);
        builder
    }

    /// The version of the latest release, when newer than this one.
    pub fn newer(api_base_url: &str, current_version: &str) -> Result<Option<String>> {
        let releases = configure(api_base_url, current_version)
            .build()?
            .get_latest_release()?;
        Ok(match (releases.is_update_available()?, releases.latest()) {
            (true, Some(latest)) => Some(latest.version().to_string()),
            _ => None,
        })
    }

    pub fn official() -> &'static str {
        GITHUB_API_URL
    }
}

/// The version of a newer release, if there's one: none for the builds of
/// the sources, or when the network doesn't answer.
pub async fn newer() -> Option<String> {
    #[cfg(feature = "self-update")]
    {
        on_a_thread(|| {
            release::newer(release::official(), &scout4all_mobile::version())
                .map_err(|e| eprintln!("no update check: {e}"))
                .ok()
                .flatten()
        })
        .await
        .flatten()
    }
    #[cfg(not(feature = "self-update"))]
    None
}

/// The latest release in place of the app: its version, or why not.
pub async fn install() -> Result<String, String> {
    #[cfg(feature = "self-update")]
    {
        on_a_thread(|| {
            release::configure(release::official(), &scout4all_mobile::version())
                .build()
                .and_then(|u| u.update())
                .map(|status| status.version().to_string())
                .map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_else(|| Err("the update stopped".to_string()))
    }
    #[cfg(not(feature = "self-update"))]
    Err("this build doesn't update itself".to_string())
}

/// The new version started in place of this one, with the same arguments:
/// returns only when it can't be, with why.
pub fn restart() -> String {
    #[cfg(feature = "self-update")]
    {
        match self_update::restart::restart() {
            Err(e) => e.to_string(),
        }
    }
    #[cfg(not(feature = "self-update"))]
    "this build doesn't update itself".to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_version_is_said_as_the_old_tuis_check_it() {
        // `scout4all X.Y.Z`: what the self update of the TUIs, and the
        // release workflow, require of the binary
        let line = super::version_line();
        assert_eq!(line, format!("scout4all {}", env!("CARGO_PKG_VERSION")));
    }
}

// as the TUI's (src/updater.rs): a release served as the GitHub API does
#[cfg(test)]
#[cfg(all(unix, feature = "self-update"))]
mod release_tests {
    use {
        super::{BIN_NAME, release::*},
        self_update::Error,
        std::{
            fs,
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            path::PathBuf,
            thread,
        },
        tempfile::TempDir,
    };

    const NEW_VERSION: &str = "9.9.9";

    fn fake_binary(version: &str) -> Vec<u8> {
        format!("#!/bin/sh\necho \"{BIN_NAME} {version}\"\n").into_bytes()
    }

    fn serve_release(assets: Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let release = serde_json::json!({
            "tag_name": format!("v{NEW_VERSION}"),
            "created_at": "2026-11-04T00:00:00Z",
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
        });
        let releases = serde_json::json!([release.clone()])
            .to_string()
            .into_bytes();
        let latest = release.to_string().into_bytes();
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request_line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut request_line).unwrap();
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or_default();
                let body = match (path.strip_prefix("/assets/"), path.ends_with("/latest")) {
                    (Some(i), _) => &assets[i.parse::<usize>().unwrap()].1,
                    (None, true) => &latest,
                    (None, false) => &releases,
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

    fn installation(dir: &TempDir) -> PathBuf {
        let exe = dir.path().join("installed").join(BIN_NAME);
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, fake_binary("0.13.0")).unwrap();
        exe
    }

    fn published(binary: &[u8]) -> Vec<(String, Vec<u8>)> {
        vec![(
            binary_asset_name(self_update::get_target()),
            binary.to_vec(),
        )]
    }

    #[test]
    fn a_newer_release_is_found_then_installed() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let binary = fake_binary(NEW_VERSION);
        let url = serve_release(published(&binary));
        assert_eq!(
            newer(&url, "0.13.0").unwrap(),
            Some(NEW_VERSION.to_string())
        );
        let status = configure(&url, "0.13.0")
            .bin_install_path(&exe)
            .build()
            .and_then(|u| u.update())
            .unwrap();
        assert_eq!(status.version(), NEW_VERSION);
        assert_eq!(fs::read(&exe).unwrap(), binary);
    }

    #[test]
    fn up_to_date_nothing_is_offered() {
        let url = serve_release(published(&fake_binary(NEW_VERSION)));
        assert_eq!(newer(&url, NEW_VERSION).unwrap(), None);
    }

    #[test]
    fn an_asset_that_is_not_scout4all_is_refused() {
        let dir = TempDir::new().unwrap();
        let exe = installation(&dir);
        let url = serve_release(published(b"{\"bomFormat\":\"CycloneDX\"}"));
        let err = configure(&url, "0.13.0")
            .bin_install_path(&exe)
            .build()
            .and_then(|u| u.update())
            .unwrap_err();
        assert!(matches!(err, Error::VerificationRejected { .. }));
        assert_eq!(fs::read(&exe).unwrap(), fake_binary("0.13.0"));
    }
}
