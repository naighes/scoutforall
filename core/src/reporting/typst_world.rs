use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict, Duration};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::datetime::Time;
use typst_kit::fonts::{self, FontStore};

/// The fonts of the report, embedded so that it looks the same everywhere
/// (SIL Open Font License, see `assets/fonts/LICENSE.md`).
const REPORT_FONTS: [&[u8]; 2] = [
    include_bytes!("../../assets/fonts/SourceSans3-Regular.ttf"),
    include_bytes!("../../assets/fonts/SourceSans3-Semibold.ttf"),
];

/// A minimal typst environment that compiles a single in-memory source,
/// with the data it lays out in `sys.inputs`.
///
/// The report does not import packages nor read files, so any file access
/// is denied; the user-provided content (e.g. team or player names) is data,
/// never part of the source.
pub struct ReportWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    source: Source,
    time: Time,
}

impl ReportWorld {
    pub fn new(content: &str, inputs: Dict) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(REPORT_FONTS.iter().flat_map(|data| {
            Font::iter(Bytes::new(*data)).map(|font| {
                let info = font.info().clone();
                (font, info)
            })
        }));
        // the fallback for the characters the report fonts miss
        fonts.extend(fonts::embedded());
        fonts.extend(fonts::system());
        Self {
            library: LazyHash::new(Library::builder().with_inputs(inputs).build()),
            fonts,
            source: Source::detached(content),
            time: Time::system(),
        }
    }

    pub fn main_source(&self) -> &Source {
        &self.source
    }
}

impl World for ReportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn file(&self, _id: FileId) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}
