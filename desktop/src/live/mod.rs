//! Live video: a camera recorded while the match is scouted. Each
//! recording is a file of the match (its video, `video.json`) starting
//! when it started: lined up by itself, as the events are on the same
//! clock.

pub mod capture;
pub mod network;
pub mod picture;
pub mod preview;
pub mod raw;
pub mod rtmp;
pub mod screen;
pub mod space;

use {
    capture::Quality,
    chrono::{DateTime, Local},
    scout4all_core::video::{self, StoredProfile, VideoFile},
    scout4all_mobile::Scout4All,
    serde::{Deserialize, Serialize},
    std::{
        fs,
        path::{Path, PathBuf},
    },
};

const CONFIG_FILE: &str = "live.json";
const SIDECAR_EXTENSION: &str = "json";

/// The source chosen last time, on this computer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveConfig {
    #[serde(default)]
    pub camera: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub folder: Option<PathBuf>,
    /// the sources on the network used, the last first: their passwords
    /// in the keychain
    #[serde(default)]
    pub networks: Vec<SavedNetwork>,
    /// the one source kept before the list (v0.15): read, moved into
    /// `networks`
    #[serde(default, skip_serializing)]
    network: Option<SavedNetwork>,
    /// the port and the key the apps send to (RTMP received)
    #[serde(default)]
    pub receiver: Option<SavedReceiver>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedReceiver {
    pub port: u16,
    pub key: String,
}

/// The sources on the network remembered: the oldest dropped past these.
pub const MAX_NETWORKS: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedNetwork {
    /// what the user calls it; none, its address
    #[serde(default)]
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub username: String,
}

impl SavedNetwork {
    /// The same source: the same address, the same user (its password in
    /// the keychain by these).
    pub fn same(&self, other: &SavedNetwork) -> bool {
        self.url == other.url && self.username == other.username
    }

    /// What it's called in the list: its name, or its address.
    pub fn label(&self) -> &str {
        match self.name.trim() {
            "" => &self.url,
            name => name,
        }
    }
}

/// The name of the app in the keychain of the system.
const KEYCHAIN_SERVICE: &str = "scout4all";

fn keychain_entry(url: &str, username: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("{username}@{url}")).map_err(|e| e.to_string())
}

/// The password of a source on the network kept in the keychain of the
/// system (macOS Keychain, Windows Credential Manager, the keyring of the
/// Linux kernel); none written, none kept.
pub fn keep_password(url: &str, username: &str, password: &str) -> Result<(), String> {
    let entry = keychain_entry(url, username)?;
    match password.is_empty() {
        true => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        },
        false => entry.set_password(password).map_err(|e| e.to_string()),
    }
}

/// The password kept for the source, if any.
pub fn kept_password(url: &str, username: &str) -> Option<String> {
    keychain_entry(url, username)
        .ok()
        .and_then(|e| e.get_password().ok())
}

impl LiveConfig {
    pub fn read(data_dir: &Path) -> LiveConfig {
        fs::read_to_string(data_dir.join(CONFIG_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<LiveConfig>(&text).ok())
            .map(LiveConfig::listed)
            .unwrap_or_default()
    }

    /// The one source of before the list, in the list.
    fn listed(self) -> LiveConfig {
        match (self.networks.is_empty(), self.network.clone()) {
            (true, Some(old)) => LiveConfig {
                networks: vec![old],
                network: None,
                ..self
            },
            _ => LiveConfig {
                network: None,
                ..self
            },
        }
    }

    /// `used` first in the list, in place of the one it was chosen as
    /// (`chosen`: maybe renamed, its address changed) and of any other
    /// with its address and user; the oldest past `MAX_NETWORKS` dropped.
    /// The ones gone, other than `used`: their passwords to forget.
    pub fn use_network(&mut self, used: SavedNetwork, chosen: Option<usize>) -> Vec<SavedNetwork> {
        let (replaced, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.networks)
            .into_iter()
            .enumerate()
            .partition(|(i, n)| Some(*i) == chosen || n.same(&used));
        let others: Vec<SavedNetwork> = others.into_iter().map(|(_, n)| n).collect();
        let kept = others.len().min(MAX_NETWORKS.saturating_sub(1));
        let (others, old) = others.split_at(kept);
        self.networks = [vec![used.clone()], others.to_vec()].concat();
        replaced
            .into_iter()
            .map(|(_, n)| n)
            .chain(old.iter().cloned())
            .filter(|n| !n.same(&used))
            .collect()
    }

    /// The source `i` out of the list: it, its password to forget.
    pub fn forget_network(&mut self, i: usize) -> Option<SavedNetwork> {
        (i < self.networks.len()).then(|| self.networks.remove(i))
    }

    pub fn write(&self, data_dir: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(data_dir.join(CONFIG_FILE), text).map_err(|e| e.to_string())
    }

    pub fn quality(&self) -> Quality {
        self.quality
            .as_deref()
            .and_then(Quality::from_key)
            .unwrap_or_default()
    }

    /// Where the recordings go: chosen, or the videos of the user
    /// (`scout4all` in them), or next to the data.
    pub fn folder(&self, data_dir: &Path) -> PathBuf {
        self.folder.clone().unwrap_or_else(|| {
            dirs::video_dir()
                .map(|d| d.join("scout4all"))
                .unwrap_or_else(|| data_dir.join("recordings"))
        })
    }
}

/// Which match a recording is of, next to it while it's made: a recording
/// left over still knows where it goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub team_id: String,
    pub match_id: String,
    pub started: DateTime<chrono::FixedOffset>,
}

pub fn write_sidecar(base: &Path, sidecar: &Sidecar) -> Result<(), String> {
    let text = serde_json::to_string_pretty(sidecar).map_err(|e| e.to_string())?;
    let path = base.with_extension(SIDECAR_EXTENSION);
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn read_sidecar(base: &Path) -> Option<Sidecar> {
    fs::read_to_string(base.with_extension(SIDECAR_EXTENSION))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

/// The recording `mp4`, started at `started`, a file of the video of the
/// match: after the ones there, in the order of their starts. Its sidecar
/// removed once done.
pub fn attach(
    core: &Scout4All,
    data_dir: &Path,
    base: &Path,
    mp4: &Path,
    started: DateTime<Local>,
) -> Result<(), String> {
    let sidecar = read_sidecar(base).ok_or("the match of the recording is not known")?;
    let (the_match, stored) = core
        .video(&sidecar.team_id, &sidecar.match_id)
        .map_err(|e| e.to_string())?;
    let profile = match stored {
        StoredProfile::Missing => video::VideoProfile::default(),
        StoredProfile::Profile(p) => p,
        StoredProfile::Newer(_) => return Err("video.json of a newer version".to_string()),
    };
    let file = VideoFile {
        path: video::stored_path(data_dir, mp4),
        start: Some(started.fixed_offset()),
    };
    let files = match profile.files.iter().any(|f| f.path == file.path) {
        true => profile.files.clone(),
        false => video::in_order([profile.files.clone(), vec![file]].concat()),
    };
    let profile = video::VideoProfile { files, ..profile };
    core.save_video(&the_match, &profile)
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(base.with_extension(SIDECAR_EXTENSION));
    Ok(())
}

/// The recordings in `folder` cut short (the app closed while recording):
/// made MP4s and given to their matches. What happened to each, by name.
pub fn recover(core: &Scout4All, data_dir: &Path, folder: &Path) -> Vec<Result<String, String>> {
    raw::left_over(folder)
        .into_iter()
        .map(|base| {
            let name = base
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let started = read_sidecar(&base)
                .map(|s| s.started.with_timezone(&Local))
                .ok_or_else(|| format!("{name}: the match of the recording is not known"))?;
            let mp4 = base.with_extension("mp4");
            let made = raw::remux(&base, &mp4).map_err(|e| format!("{name}: {e}"))?;
            let skipped =
                chrono::TimeDelta::milliseconds(i64::try_from(made.skipped_ms).unwrap_or(0));
            attach(core, data_dir, &base, &mp4, started + skipped)
                .map_err(|e| format!("{name}: {e}"))?;
            Ok(name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        openh264::{OpenH264API, encoder::Encoder, formats::YUVBuffer},
    };

    /// A recording of a second, raw as the app closed while making it.
    fn left_raw(base: &Path) {
        let mut encoder = Encoder::with_api_config(
            OpenH264API::from_source(),
            openh264::encoder::EncoderConfig::new(),
        )
        .unwrap();
        let mut writer = raw::RawWriter::create(base).unwrap();
        for i in 0..30u64 {
            let yuv = YUVBuffer::from_vec(vec![60; 64 * 48 * 3 / 2], 64, 48);
            writer
                .write(i * 33, &encoder.encode(&yuv).unwrap().to_vec())
                .unwrap();
        }
        writer.flush().unwrap();
    }

    /// Cut short, then found at the next start: an MP4, a file of its
    /// match from when it started, its raw files and sidecar gone.
    #[test]
    fn a_recording_recovered_into_its_match() {
        let dir = tempfile::tempdir().unwrap();
        let (core, team, m) = crate::video::screen_tests::imported(&dir);
        let folder = dir.path().join("rec");
        fs::create_dir_all(&folder).unwrap();
        let started = Local::now();
        let base = capture::base_name(&folder, started);
        left_raw(&base);
        let sidecar = Sidecar {
            team_id: team.clone(),
            match_id: m.clone(),
            started: started.fixed_offset(),
        };
        write_sidecar(&base, &sidecar).unwrap();
        let name = base.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(recover(&core, dir.path(), &folder), vec![Ok(name)]);
        let (_, stored) = core.video(&team, &m).unwrap();
        let StoredProfile::Profile(profile) = stored else {
            panic!("no profile");
        };
        assert_eq!(profile.files.len(), 1);
        assert_eq!(profile.files[0].start, Some(started.fixed_offset()));
        assert!(base.with_extension("mp4").is_file());
        assert!(read_sidecar(&base).is_none());
        // nothing more the next time
        assert!(recover(&core, dir.path(), &folder).is_empty());
        // given again (saved twice): one file
        write_sidecar(&base, &sidecar).unwrap();
        attach(
            &core,
            dir.path(),
            &base,
            &base.with_extension("mp4"),
            started,
        )
        .unwrap();
        let (_, stored) = core.video(&team, &m).unwrap();
        assert!(matches!(stored, StoredProfile::Profile(p) if p.files.len() == 1));
    }

    /// Without its sidecar the match isn't known: said, left raw.
    #[test]
    fn a_recording_of_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let (core, _, _) = crate::video::screen_tests::imported(&dir);
        let base = dir.path().join("20261106 120000");
        left_raw(&base);
        let recovered = recover(&core, dir.path(), dir.path());
        assert!(matches!(recovered.as_slice(), [Err(_)]));
        assert!(raw::raw_paths(&base).0.is_file());
    }

    #[test]
    fn the_config_kept() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(LiveConfig::read(dir.path()), LiveConfig::default());
        assert_eq!(LiveConfig::default().quality(), Quality::High);
        let config = LiveConfig {
            camera: Some("FaceTime HD Camera".into()),
            quality: Some("1080p".into()),
            folder: Some(dir.path().join("rec")),
            networks: vec![SavedNetwork {
                name: "Palestra".into(),
                url: "rtsp://cam.local/live".into(),
                username: "scout".into(),
            }],
            network: None,
            receiver: Some(SavedReceiver {
                port: 1936,
                key: "abcd1234".into(),
            }),
        };
        config.write(dir.path()).unwrap();
        let read = LiveConfig::read(dir.path());
        assert_eq!(read, config);
        assert_eq!(read.quality(), Quality::Full);
        assert_eq!(read.folder(dir.path()), dir.path().join("rec"));
    }

    fn source(name: &str, url: &str, username: &str) -> SavedNetwork {
        SavedNetwork {
            name: name.into(),
            url: url.into(),
            username: username.into(),
        }
    }

    /// The one source kept by v0.15 read as the first of the list; not
    /// written back in the old way.
    #[test]
    fn the_source_of_before_in_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let old = r#"{"network": {"url": "rtsp://cam.local/live", "username": "scout"}}"#;
        fs::write(dir.path().join(CONFIG_FILE), old).unwrap();
        let read = LiveConfig::read(dir.path());
        assert_eq!(
            read.networks,
            vec![source("", "rtsp://cam.local/live", "scout")]
        );
        assert_eq!(read.networks[0].label(), "rtsp://cam.local/live");
        read.write(dir.path()).unwrap();
        let text = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert!(!text.contains("\"network\""), "{text}");
        assert_eq!(LiveConfig::read(dir.path()), read);
    }

    /// A source used goes first: a new one added, one chosen and renamed
    /// or changed in its place, the same address and user once; the
    /// passwords of the ones gone to forget.
    #[test]
    fn the_sources_used() {
        let mut config = LiveConfig::default();
        let a = source("", "rtsp://a/live", "");
        let b = source("Tribuna", "rtsp://b/live", "admin");
        assert!(config.use_network(a.clone(), None).is_empty());
        assert!(config.use_network(b.clone(), None).is_empty());
        assert_eq!(config.networks, vec![b.clone(), a.clone()]);
        // a chosen, renamed: in its place, first, nothing to forget
        let renamed = source("Campo", "rtsp://a/live", "");
        assert!(config.use_network(renamed.clone(), Some(1)).is_empty());
        assert_eq!(config.networks, vec![renamed.clone(), b.clone()]);
        // b chosen, its user changed: the old user's password to forget
        let changed = source("Tribuna", "rtsp://b/live", "scout");
        assert_eq!(
            config.use_network(changed.clone(), Some(1)),
            vec![b.clone()]
        );
        assert_eq!(config.networks, vec![changed.clone(), renamed.clone()]);
        // written again as new: the same source, once
        assert!(
            config
                .use_network(source("", "rtsp://a/live", ""), None)
                .is_empty()
        );
        assert_eq!(config.networks.len(), 2);
        assert_eq!(config.forget_network(1), Some(changed));
        assert_eq!(config.forget_network(1), None);
        assert_eq!(config.networks.len(), 1);
    }

    /// Past the most remembered, the oldest dropped, its password too.
    #[test]
    fn the_oldest_source_dropped() {
        let mut config = LiveConfig::default();
        let all: Vec<SavedNetwork> = (0..MAX_NETWORKS)
            .map(|i| source("", &format!("rtsp://cam{i}/live"), ""))
            .collect();
        all.iter().for_each(|n| {
            config.use_network(n.clone(), None);
        });
        assert_eq!(config.networks.len(), MAX_NETWORKS);
        let gone = config.use_network(source("", "rtsp://new/live", ""), None);
        assert_eq!(gone, vec![all[0].clone()]);
        assert_eq!(config.networks.len(), MAX_NETWORKS);
        assert_eq!(config.networks[0].url, "rtsp://new/live");
    }
}
