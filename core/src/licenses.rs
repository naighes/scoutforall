//! The parts made by others that the app carries inside it, with their
//! licenses: each asks for its text to travel with the app, so the apps show
//! them in the settings. The text of a license stays as written, in English.
//!
//! The libraries the program is built with (Rust crates, Android libraries)
//! aren't here yet.

use crate::localization::Labels;

/// What a part is for, in the words of the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    FaceDetection,
    BackgroundRemoval,
    ReportFont,
}

impl Purpose {
    pub const fn label(self, labels: &Labels) -> &'static str {
        match self {
            Purpose::FaceDetection => labels.third_party_face_detection,
            Purpose::BackgroundRemoval => labels.third_party_background_removal,
            Purpose::ReportFont => labels.third_party_report_font,
        }
    }
}

/// A part made by others, carried inside the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThirdParty {
    pub name: &'static str,
    pub purpose: Purpose,
    /// where it comes from
    pub source: &'static str,
    /// the name of the license
    pub license: &'static str,
    /// the full text of the license, with the notices that go with it
    pub text: &'static str,
}

/// The parts made by others the app carries: the models of `faces` and the
/// font of the reports.
pub const THIRD_PARTIES: [ThirdParty; 3] = [
    ThirdParty {
        name: "SeetaFace Engine",
        purpose: Purpose::FaceDetection,
        source: "https://github.com/seetaface/SeetaFaceEngine",
        license: "BSD 2-Clause",
        text: include_str!("../models/seeta_fd_frontal_v1.0.LICENSE"),
    },
    ThirdParty {
        name: "U-2-Net (U2NetP)",
        purpose: Purpose::BackgroundRemoval,
        source: "https://github.com/xuebinqin/U-2-Net",
        license: "Apache License 2.0",
        text: include_str!("../models/u2netp.LICENSE"),
    },
    ThirdParty {
        name: "Source Sans 3",
        purpose: Purpose::ReportFont,
        source: "https://github.com/adobe-fonts/source-sans",
        license: "SIL Open Font License 1.1",
        text: include_str!("../assets/fonts/LICENSE.md"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{localization::labels, shapes::enums::LanguageEnum};

    #[test]
    fn each_part_carries_the_whole_text_of_its_license() {
        let [seeta, u2net, font] = THIRD_PARTIES;
        // the notice the BSD license asks to reproduce, its conditions
        assert!(seeta.text.contains("Chinese Academy of Sciences"));
        assert!(seeta.text.contains("Redistributions in binary form"));
        assert!(u2net.text.contains("Apache License"));
        assert!(u2net.text.contains("END OF TERMS AND CONDITIONS"));
        assert!(font.text.contains("Copyright 2010-2024 Adobe"));
        assert!(font.text.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }

    #[test]
    fn each_purpose_is_said_in_every_language() {
        for language in LanguageEnum::ALL {
            let labels = labels(language);
            let mut purposes: Vec<_> = THIRD_PARTIES
                .iter()
                .map(|p| p.purpose.label(labels))
                .collect();
            assert!(purposes.iter().all(|p| !p.is_empty()), "{language}");
            purposes.dedup();
            assert_eq!(purposes.len(), THIRD_PARTIES.len(), "{language}");
        }
    }
}
