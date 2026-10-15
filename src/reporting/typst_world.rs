use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::datetime::Time;
use typst_kit::fonts::{self, FontStore};

/// A minimal typst environment that compiles a single in-memory source.
///
/// The generated report does not import packages nor read files, so any
/// file access is denied: this also prevents user-provided content (e.g.
/// team or player names) from reaching the file system.
pub struct ReportWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    source: Source,
    time: Time,
}

impl ReportWorld {
    pub fn new(content: String) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(fonts::embedded());
        fonts.extend(fonts::system());
        Self {
            library: LazyHash::new(Library::default()),
            fonts,
            source: Source::detached(content),
            time: Time::system(),
        }
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
