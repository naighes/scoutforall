//! The video source of a match, step by step: what kind (a camera of the
//! computer, or one on the network), then which camera, seen at once, at
//! what quality, into which folder; or the address of the one on the
//! network, its user and password, tried and seen; then recording.

use {
    super::{
        LiveConfig, SavedNetwork,
        capture::{self, CameraEntry, Capture, Quality, Source, Status},
        network::{NetworkSource, Problem},
    },
    crate::{
        form::{self, Field, Form},
        strings::Strings,
        theme::{Colors, SIZE_BODY, SIZE_SMALL},
        ui,
        video::Picture,
    },
    iced::{
        Alignment, ContentFit, Element, Length,
        advanced::image::Allocation,
        widget::{Column, column, container, image, row, space, text},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    std::{path::PathBuf, sync::Arc},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Camera,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Source,
    Camera,
    Network,
}

/// The fields of the source on the network.
const ADDRESS: usize = 0;
const USERNAME: usize = 1;
const PASSWORD: usize = 2;

#[derive(Debug, Clone)]
pub enum Message {
    Kind(Kind),
    /// whether the system lets the app use the cameras
    Allowed(bool),
    Refresh,
    Camera(usize),
    Quality(Quality),
    ChangeFolder,
    Folder(Option<PathBuf>),
    Start,
    Tick,
    Allocated(Arc<Picture>, image::Handle, Option<Allocation>),
    Form(form::Message),
}

pub enum Outcome {
    Stay,
    Back,
    /// the system asked for the cameras (`Message::Allowed`)
    AskPermission,
    /// a folder for the recordings (`Message::Folder`)
    PickFolder,
    Allocate(Arc<Picture>, image::Handle),
    /// the recording to start: `take_capture`
    Record,
}

pub fn allocate(frame: Arc<Picture>, handle: image::Handle) -> iced::Task<Message> {
    iced_runtime::image::allocate(handle.clone())
        .map(move |allocated| Message::Allocated(frame.clone(), handle.clone(), allocated.ok()))
}

struct Shown {
    frame: Arc<Picture>,
    handle: image::Handle,
    _allocation: Option<Allocation>,
}

pub struct LiveScreen {
    pub team_id: String,
    pub match_id: String,
    opponent: String,
    dir: PathBuf,
    config: LiveConfig,
    step: Step,
    /// in the first step, the kind of source chosen
    kind: Kind,
    permission: Option<bool>,
    cameras: Vec<CameraEntry>,
    selected: usize,
    capture: Option<Capture>,
    status: Option<Status>,
    shown: Option<Shown>,
    requested: u64,
    error: Option<String>,
    /// the address, user and password of the source on the network
    form: Form,
}

/// The problem of a source on the network, in the user's words.
pub fn problem_text(problem: &Problem, s: &Strings) -> String {
    match problem {
        Problem::Invalid => s.problem_invalid.to_string(),
        Problem::NotYet(p) => ui::fill(s.problem_not_yet, &[p.name().to_string()]),
        Problem::Unreachable => s.problem_unreachable.to_string(),
        Problem::Refused => s.problem_refused.to_string(),
        Problem::NotFound => s.problem_not_found.to_string(),
        Problem::Silent => s.problem_silent.to_string(),
        Problem::Hevc => s.problem_hevc.to_string(),
        Problem::NoVideo(what) => ui::fill(s.problem_no_video, std::slice::from_ref(what)),
        Problem::Other(e) => e.clone(),
    }
}

impl LiveScreen {
    pub fn new(team_id: String, match_id: String, opponent: String, dir: PathBuf) -> LiveScreen {
        let config = LiveConfig::read(&dir);
        LiveScreen {
            team_id,
            match_id,
            opponent,
            dir,
            config,
            step: Step::Source,
            kind: Kind::Camera,
            permission: None,
            cameras: vec![],
            selected: 0,
            capture: None,
            status: None,
            shown: None,
            requested: 0,
            error: None,
            form: Form::new("", vec![]),
        }
    }

    /// The form of the source on the network: as used last time, its
    /// password from the keychain.
    fn network_form(&self, s: &Strings) -> Form {
        let saved = self.config.network.clone().unwrap_or_default();
        let password = super::kept_password(&saved.url, &saved.username).unwrap_or_default();
        let mut form = Form::new(
            s.source_network,
            vec![
                Field::text(s.network_address, saved.url)
                    .placeholder("rtsp://192.168.1.20:554/stream1")
                    .hint(s.network_address_hint),
                Field::text(s.username, saved.username),
                Field::secret(s.password, password).hint(s.password_kept),
            ],
        );
        form.actions = Some((s.back.to_string(), s.try_source.to_string()));
        form
    }

    /// The source written in the form.
    fn written(&self) -> NetworkSource {
        NetworkSource {
            url: self.form.text(ADDRESS).trim().to_string(),
            username: self.form.text(USERNAME).trim().to_string(),
            password: self.form.text(PASSWORD),
        }
    }

    /// The source on the network tried: opened, its preview as it comes.
    /// Kept for next time, the password in the keychain.
    fn try_network(&mut self) {
        let source = self.written();
        self.capture = None;
        self.shown = None;
        self.status = None;
        self.requested = 0;
        self.config.network = Some(SavedNetwork {
            url: source.url.clone(),
            username: source.username.clone(),
        });
        self.save_config();
        if let Err(e) = super::keep_password(&source.url, &source.username, &source.password) {
            self.error = Some(e);
        }
        self.capture = Some(Capture::open(Source::Network(source)));
    }

    /// Whether what's on screen is the source written: then Enter records.
    fn tried(&self) -> bool {
        let written = Source::Network(self.written());
        self.capture.as_ref().is_some_and(|c| c.source == written)
    }

    /// The text field the keys go to, in the form.
    pub fn focus_id(&self) -> Option<iced::widget::Id> {
        match self.step {
            Step::Network => Some(self.form.focus_id()),
            _ => None,
        }
    }

    pub fn title(&self, s: &Strings) -> String {
        format!("{} – {}", s.video_source, self.opponent)
    }

    /// Whether the preview wants the frames of the window.
    pub fn awake(&self) -> bool {
        self.capture.is_some()
    }

    /// The camera and how to record, for the app to keep recording once
    /// the screen is left.
    pub fn take_capture(&mut self) -> Option<(Capture, Quality, PathBuf)> {
        let quality = self.config.quality();
        let folder = self.config.folder(&self.dir);
        self.capture.take().map(|c| (c, quality, folder))
    }

    fn save_config(&mut self) {
        if let Err(e) = self.config.write(&self.dir) {
            self.error = Some(e);
        }
    }

    fn choose_kind(&mut self, kind: Kind, s: &Strings) -> Outcome {
        self.kind = kind;
        match kind {
            Kind::Camera => {
                self.step = Step::Camera;
                Outcome::AskPermission
            }
            Kind::Network => {
                self.form = self.network_form(s);
                self.step = Step::Network;
                Outcome::Stay
            }
        }
    }

    fn refresh(&mut self) {
        match capture::cameras() {
            Ok(cameras) => {
                let preferred = self
                    .config
                    .camera
                    .as_ref()
                    .and_then(|name| cameras.iter().position(|c| &c.name == name))
                    .unwrap_or(0);
                self.cameras = cameras;
                self.error = None;
                self.open(preferred);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn open(&mut self, i: usize) {
        let Some(camera) = self.cameras.get(i).cloned() else {
            self.capture = None;
            return;
        };
        let same = self
            .capture
            .as_ref()
            .is_some_and(|c| c.source == Source::Camera(camera.clone()));
        self.selected = i;
        if !same {
            // the camera before let go first: some can't be opened twice
            self.capture = None;
            self.shown = None;
            self.requested = 0;
            self.config.camera = Some(camera.name.clone());
            self.save_config();
            self.capture = Some(Capture::open(Source::Camera(camera)));
        }
    }

    fn tick(&mut self) -> Outcome {
        self.status = self.capture.as_ref().and_then(Capture::status);
        match self.status.as_ref().and_then(|s| s.frame.clone()) {
            Some(frame) if frame.serial > self.requested => {
                self.requested = frame.serial;
                let handle =
                    image::Handle::from_rgba(frame.width, frame.height, frame.rgba.clone());
                Outcome::Allocate(frame, handle)
            }
            _ => Outcome::Stay,
        }
    }

    fn back(&mut self) -> Outcome {
        match self.step {
            Step::Camera | Step::Network => {
                self.capture = None;
                self.shown = None;
                self.status = None;
                self.step = Step::Source;
                Outcome::Stay
            }
            Step::Source => Outcome::Back,
        }
    }

    fn can_start(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.frame.is_some())
    }

    pub fn update(&mut self, message: Message, s: &Strings) -> Outcome {
        match message {
            Message::Kind(kind) => return self.choose_kind(kind, s),
            Message::Form(m) => match self.form.update(m) {
                form::Outcome::Save => return self.enter(),
                form::Outcome::Cancel => return self.back(),
                form::Outcome::Stay | form::Outcome::Changed(_) => {}
            },
            Message::Allowed(true) => {
                self.permission = Some(true);
                self.refresh();
            }
            Message::Allowed(false) => self.permission = Some(false),
            Message::Refresh => {
                self.capture = None;
                self.refresh();
            }
            Message::Camera(i) => self.open(i),
            Message::Quality(q) => {
                self.config.quality = Some(q.key().to_string());
                self.save_config();
            }
            Message::ChangeFolder => return Outcome::PickFolder,
            Message::Folder(Some(folder)) => {
                self.config.folder = Some(folder);
                self.save_config();
            }
            Message::Folder(None) => {}
            Message::Start if self.can_start() && (self.step != Step::Network || self.tried()) => {
                return Outcome::Record;
            }
            Message::Start => {}
            Message::Tick => return self.tick(),
            Message::Allocated(frame, handle, allocation) => {
                let newer = self
                    .shown
                    .as_ref()
                    .is_none_or(|shown| shown.frame.serial < frame.serial);
                if newer {
                    self.shown = Some(Shown {
                        frame,
                        handle,
                        _allocation: allocation,
                    });
                }
            }
        }
        Outcome::Stay
    }

    /// Enter on the network: tried, then (seen) recorded.
    fn enter(&mut self) -> Outcome {
        match (self.tried(), self.can_start()) {
            (true, true) => Outcome::Record,
            (true, false) => Outcome::Stay,
            (false, _) => {
                self.try_network();
                Outcome::Stay
            }
        }
    }

    pub fn key(&mut self, ck: KeyCombination, s: &Strings) -> Outcome {
        let quality = self.config.quality();
        match (self.step, ck) {
            // the form takes the keys, Esc and Enter too
            (Step::Network, ck) => match self.form.key(ck) {
                form::Outcome::Save => self.enter(),
                form::Outcome::Cancel => self.back(),
                form::Outcome::Stay | form::Outcome::Changed(_) => Outcome::Stay,
            },
            (_, k) if k == BACK_KEY => self.back(),
            (Step::Source, k) if k == key!(up) || k == key!(down) => {
                self.kind = match self.kind {
                    Kind::Camera => Kind::Network,
                    Kind::Network => Kind::Camera,
                };
                Outcome::Stay
            }
            (Step::Source, k) if k == key!(enter) => self.choose_kind(self.kind, s),
            (Step::Camera, k) if k == key!(up) => {
                self.open(self.selected.saturating_sub(1));
                Outcome::Stay
            }
            (Step::Camera, k) if k == key!(down) => {
                self.open((self.selected + 1).min(self.cameras.len().saturating_sub(1)));
                Outcome::Stay
            }
            (Step::Camera, k) if k == key!(q) => {
                let at = Quality::ALL.iter().position(|q| *q == quality).unwrap_or(0);
                let next = Quality::ALL[(at + 1) % Quality::ALL.len()];
                self.update(Message::Quality(next), s)
            }
            (Step::Camera, k) if k == key!(f) => Outcome::PickFolder,
            (Step::Camera, k) if k == key!(u) => self.update(Message::Refresh, s),
            (Step::Camera, k) if k == key!(enter) => self.update(Message::Start, s),
            _ => Outcome::Stay,
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let hint = |k: &str, what: &str| (k.to_string(), what.to_string());
        match self.step {
            Step::Source => vec![hint("↑ ↓ Enter", s.video_source), hint("Esc", s.back)],
            Step::Camera => vec![
                hint("↑ ↓", s.camera),
                hint("q", s.quality),
                hint("f", s.recordings_folder),
                hint("u", s.refresh),
                hint("Enter", s.start_recording),
                hint("Esc", s.back),
            ],
            Step::Network => vec![
                hint("↑ ↓ ⇥", s.next),
                hint(
                    "Enter",
                    match self.tried() && self.can_start() {
                        true => s.start_recording,
                        false => s.try_source,
                    },
                ),
                hint("Esc", s.back),
            ],
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        match self.step {
            Step::Source => self.source_view(s, c),
            Step::Camera => self.camera_view(s, c),
            Step::Network => self.network_view(s, c),
        }
    }

    /// The preview, black, with what's wrong or what's awaited in it.
    fn preview<'a>(
        &'a self,
        notice: Option<String>,
        waiting: &'static str,
        c: Colors,
    ) -> Element<'a, Message> {
        let preview: Element<'a, Message> = match (&self.shown, notice) {
            (Some(shown), _) => image(shown.handle.clone())
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            (None, Some(n)) => container(ui::body(n).color(c.danger))
                .padding(24)
                .center(Length::Fill)
                .into(),
            (None, None) => container(ui::body(waiting).color(c.muted))
                .center(Length::Fill)
                .into(),
        };
        container(preview)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..container::Style::default()
            })
            .into()
    }

    fn network_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let status = self.status.as_ref();
        let notice = match (&self.error, status) {
            (Some(e), _) => Some(e.clone()),
            (
                None,
                Some(Status {
                    problem: Some(p), ..
                }),
            ) => Some(problem_text(p, s)),
            (None, Some(Status { error: Some(e), .. })) => Some(e.clone()),
            _ => None,
        };
        let waiting = match self.capture {
            Some(_) => s.connecting,
            None => s.network_address_hint,
        };
        let seen = status
            .and_then(|st| st.size)
            .filter(|_| self.shown.is_some())
            .map(|(w, h)| format!("H.264 · {w}×{h} · {}", s.as_it_comes));
        let folder = self.config.folder(&self.dir);
        let aside = column![
            text(seen.unwrap_or_default())
                .size(SIZE_SMALL)
                .color(c.muted),
            ui::section(s.recordings_folder, c),
            text(folder.display().to_string()).size(SIZE_SMALL),
            ui::pill(
                &ui::capitalized(s.change),
                false,
                Some(Message::ChangeFolder),
                c
            ),
            ui::pill(
                s.start_recording,
                true,
                (self.tried() && self.can_start()).then_some(Message::Start),
                c
            ),
        ]
        .spacing(10);
        row![
            self.preview(notice, waiting, c),
            container(self.form.view(s, c, Message::Form, Some(aside.into())))
                .width(Length::Fixed(440.0))
                .height(Length::Fill),
        ]
        .spacing(16)
        .padding(16)
        .into()
    }

    fn source_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let choice = |kind: Kind, title: &'static str, detail: &'static str, enabled: bool| {
            let body = column![
                text(title).size(SIZE_BODY).color(match enabled {
                    true => c.text,
                    false => c.muted,
                }),
                text(detail).size(SIZE_SMALL).color(c.muted),
            ]
            .spacing(4);
            ui::list_row(body, self.kind == kind, Some(Message::Kind(kind)), c)
        };
        let choices = column![
            choice(Kind::Camera, s.source_camera, s.source_camera_hint, true),
            choice(Kind::Network, s.source_network, s.source_network_hint, true),
        ]
        .spacing(8)
        .width(Length::Fixed(520.0));
        container(ui::panel(Some(s.video_source), choices, c))
            .center(Length::Fill)
            .into()
    }

    fn camera_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let notice: Option<String> = match (&self.permission, &self.error, &self.status) {
            (Some(false), _, _) => Some(s.camera_denied.to_string()),
            (_, Some(e), _) => Some(e.clone()),
            (_, None, Some(Status { error: Some(e), .. })) => Some(e.clone()),
            (Some(true), None, _) if self.cameras.is_empty() => Some(s.no_cameras.to_string()),
            _ => None,
        };
        let preview = self.preview(notice, s.camera_opening, c);
        let size = self
            .status
            .as_ref()
            .and_then(|st| st.size)
            .map(|(w, h)| format!("{w}×{h}"))
            .unwrap_or_default();
        let cameras = Column::with_children(self.cameras.iter().enumerate().map(|(i, cam)| {
            let detail = match i == self.selected {
                true => size.clone(),
                false => String::new(),
            };
            ui::list_row(
                column![
                    text(cam.name.clone()).size(SIZE_BODY),
                    text(detail).size(SIZE_SMALL).color(c.muted),
                ],
                i == self.selected,
                Some(Message::Camera(i)),
                c,
            )
        }))
        .spacing(6);
        let chosen = self.config.quality();
        let qualities = row(Quality::ALL
            .iter()
            .map(|q| ui::pill(q.key(), *q == chosen, Some(Message::Quality(*q)), c)))
        .spacing(8);
        let per_hour = ui::fill(s.per_hour, &[format!("{:.1}", chosen.gb_per_hour())]);
        let folder = self.config.folder(&self.dir);
        let side = column![
            ui::section(s.camera, c),
            cameras,
            ui::pill(s.refresh, false, Some(Message::Refresh), c),
            ui::section(s.quality, c),
            qualities,
            text(per_hour).size(SIZE_SMALL).color(c.muted),
            ui::section(s.recordings_folder, c),
            text(folder.display().to_string()).size(SIZE_SMALL),
            ui::pill(
                &ui::capitalized(s.change),
                false,
                Some(Message::ChangeFolder),
                c
            ),
            space::vertical(),
            ui::pill(
                s.start_recording,
                true,
                self.can_start().then_some(Message::Start),
                c
            ),
        ]
        .spacing(10)
        .align_x(Alignment::Start);
        row![
            preview,
            container(ui::panel(Some(s.source_camera), side, c))
                .width(Length::Fixed(380.0))
                .height(Length::Fill),
        ]
        .spacing(16)
        .padding(16)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use {super::*, scout4all_core::shapes::enums::LanguageEnum};

    fn s() -> &'static Strings {
        crate::strings::strings(LanguageEnum::It)
    }

    fn screen(dir: &tempfile::TempDir) -> LiveScreen {
        // the keychain of the tests, not the user's
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        LiveScreen::new(
            "t".into(),
            "m".into(),
            "Them".into(),
            dir.path().to_path_buf(),
        )
    }

    /// The camera asks for the permission; Esc goes back a step, then
    /// leaves.
    #[test]
    fn the_steps() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = screen(&dir);
        assert!(matches!(
            screen.key(key!(enter), s()),
            Outcome::AskPermission
        ));
        assert_eq!(screen.step, Step::Camera);
        // no picture yet: nothing to record
        assert!(matches!(screen.key(key!(enter), s()), Outcome::Stay));
        screen.update(Message::Allowed(false), s());
        assert!(screen.capture.is_none());
        assert!(matches!(screen.key(BACK_KEY, s()), Outcome::Stay));
        assert_eq!(screen.step, Step::Source);
        assert!(matches!(screen.key(BACK_KEY, s()), Outcome::Back));
    }

    /// On the network: the address written, Enter tries it (kept for the
    /// next time), not recorded until seen; Esc from the form back to
    /// the kinds.
    #[test]
    fn a_source_on_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = screen(&dir);
        screen.key(key!(down), s());
        assert!(matches!(screen.key(key!(enter), s()), Outcome::Stay));
        assert_eq!(screen.step, Step::Network);
        assert!(screen.focus_id().is_some());
        screen.update(
            Message::Form(form::Message::Input(ADDRESS, "127.0.0.1:1/none".into())),
            s(),
        );
        screen.update(
            Message::Form(form::Message::Input(USERNAME, "scout".into())),
            s(),
        );
        assert!(!screen.tried());
        assert!(matches!(screen.key(key!(enter), s()), Outcome::Stay));
        assert!(screen.tried());
        // nothing seen: Enter again doesn't record
        assert!(matches!(screen.key(key!(enter), s()), Outcome::Stay));
        assert!(matches!(screen.update(Message::Start, s()), Outcome::Stay));
        let saved = LiveConfig::read(dir.path()).network.unwrap();
        assert_eq!(
            (saved.url.as_str(), saved.username.as_str()),
            ("127.0.0.1:1/none", "scout")
        );
        // written again: to try again
        screen.update(
            Message::Form(form::Message::Input(ADDRESS, "127.0.0.1:2/none".into())),
            s(),
        );
        assert!(!screen.tried());
        assert!(matches!(screen.key(BACK_KEY, s()), Outcome::Stay));
        assert_eq!(screen.step, Step::Source);
        assert!(screen.capture.is_none());
    }

    /// The quality and the folder kept for the next time.
    #[test]
    fn the_choices_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = screen(&dir);
        screen.update(Message::Kind(Kind::Camera), s());
        screen.key(key!(q), s());
        assert_eq!(screen.config.quality(), Quality::Low);
        screen.update(Message::Folder(Some(dir.path().join("rec"))), s());
        let again = LiveScreen::new("t".into(), "m".into(), "Them".into(), dir.path().into());
        assert_eq!(again.config.quality(), Quality::Low);
        assert_eq!(again.config.folder(dir.path()), dir.path().join("rec"));
    }
}
