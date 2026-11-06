//! How long a clip of an action lasts, as the user chose: the seconds
//! before it (DataVolley gives a rally a single second, often noted after
//! the action) and after it; and whether how the match stands is shown
//! over the video. Saved in `clips.json` in the folder of the data, on
//! this computer.

use {
    serde::{Deserialize, Serialize},
    std::{fs, path::Path},
};

const FILE: &str = "clips.json";

/// The seconds before an action go from 0, the ones after from 1 (the
/// action seen), both up to this.
pub const MAX_SECONDS: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Margins {
    pub before_s: u32,
    pub after_s: u32,
    /// the set, the score... over the video; on in the files without it
    #[serde(default = "on")]
    pub overlay: bool,
}

fn on() -> bool {
    true
}

impl Default for Margins {
    fn default() -> Self {
        Margins {
            before_s: 5,
            after_s: 5,
            overlay: true,
        }
    }
}

/// One of the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margin {
    Before,
    After,
}

impl Margins {
    /// The ones saved, in their bounds; the default ones without.
    pub fn read(data_dir: &Path) -> Margins {
        fs::read_to_string(data_dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Margins>(&text).ok())
            .map(Margins::bounded)
            .unwrap_or_default()
    }

    pub fn write(&self, data_dir: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(data_dir.join(FILE), text).map_err(|e| e.to_string())
    }

    pub fn get(&self, margin: Margin) -> u32 {
        match margin {
            Margin::Before => self.before_s,
            Margin::After => self.after_s,
        }
    }

    /// With the margin at `seconds`, in its bounds.
    pub fn with(self, margin: Margin, seconds: u32) -> Margins {
        match margin {
            Margin::Before => Margins {
                before_s: seconds,
                ..self
            },
            Margin::After => Margins {
                after_s: seconds,
                ..self
            },
        }
        .bounded()
    }

    fn bounded(self) -> Margins {
        Margins {
            before_s: self.before_s.min(MAX_SECONDS),
            after_s: self.after_s.clamp(1, MAX_SECONDS),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_margins_in_their_bounds() {
        let m = Margins::default();
        assert_eq!((m.before_s, m.after_s), (5, 5));
        assert_eq!(m.with(Margin::Before, 0).before_s, 0);
        assert_eq!(m.with(Margin::After, 0).after_s, 1);
        assert_eq!(m.with(Margin::Before, 99).before_s, MAX_SECONDS);
        assert_eq!(m.with(Margin::After, 12).get(Margin::After), 12);
    }

    #[test]
    fn the_margins_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Margins::read(dir.path()), Margins::default());
        let m = Margins::default()
            .with(Margin::Before, 8)
            .with(Margin::After, 3);
        m.write(dir.path()).unwrap();
        assert_eq!(Margins::read(dir.path()), m);
        // written by hand out of bounds: in them
        std::fs::write(dir.path().join(FILE), r#"{"before_s": 90, "after_s": 0}"#).unwrap();
        let read = Margins::read(dir.path());
        assert_eq!((read.before_s, read.after_s), (MAX_SECONDS, 1));
        assert!(read.overlay, "saved before it: on");
        let off = Margins {
            overlay: false,
            ..Margins::default()
        };
        off.write(dir.path()).unwrap();
        assert!(!Margins::read(dir.path()).overlay);
    }
}
