//! The video source of a match, step by step: what kind (a camera of the
//! computer, one on the network, an app sending to this computer), then
//! which camera, seen at once, at what quality, into which folder; or the
//! address of the one on the network (its user and password, when it asks
//! for them), tried and seen; or where the app sends, and its key, the
//! video seen as it comes; then recording.

use {
    super::{
        LiveConfig, SavedNetwork, SavedReceiver,
        capture::{self, CameraEntry, Capture, Quality, Source, Status},
        network::{self, NetworkSource, Problem, Protocol},
        rtmp::{self, Receiver},
        space::{self as disk, Room},
    },
    crate::{
        form::{self, Field, Form},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
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
    Receiver,
}

const KINDS: [Kind; 3] = [Kind::Camera, Kind::Network, Kind::Receiver];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Source,
    Camera,
    Network,
    Receiver,
}

/// The fields of the source on the network: the user and the password
/// shown with the authentication on, and not for RTMP (in its address).
const ADDRESS: usize = 0;
const AUTHENTICATION: usize = 1;
const USERNAME: usize = 2;
const PASSWORD: usize = 3;

/// The fields of the app sending to this computer.
const PORT: usize = 0;
const KEY: usize = 1;

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
    /// the address, user and password of the source on the network; the
    /// port and the key of the app sending
    form: Form,
    /// this computer on its network, for the app to send to
    here: Option<String>,
    /// the QR code that sets Larix to send here, and its link
    qr: Option<(String, image::Handle)>,
    /// the space free in the folder of the recordings
    free: Option<u64>,
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
        Problem::Closed => s.problem_closed.to_string(),
        Problem::Encrypted => s.problem_encrypted.to_string(),
        Problem::PortTaken(port) => ui::fill(s.problem_port_taken, &[port.to_string()]),
        Problem::WrongKey(from) => ui::fill(s.problem_wrong_key, std::slice::from_ref(from)),
        Problem::Other(e) => e.clone(),
    }
}

/// The address of a source on the network to keep, and what goes in the
/// keychain: its password; for RTMP, the query of its address (a user, a
/// password in it), not kept in a file.
fn kept_apart(network: &NetworkSource) -> (String, String) {
    let rtmp = network::parse(&network.url).is_ok_and(|(_, p, _)| p == Protocol::Rtmp);
    match (rtmp, network.url.split_once('?')) {
        (true, Some((url, query))) => (url.to_string(), format!("?{query}")),
        _ => (network.url.clone(), network.password.clone()),
    }
}

/// The address and the password as written, from the ones kept: RTMP's
/// query back in its address.
fn given_back(saved: &SavedNetwork, kept: String) -> (String, String) {
    match (saved.username.is_empty(), kept.starts_with('?')) {
        (true, true) => (format!("{}{kept}", saved.url), String::new()),
        _ => (saved.url.clone(), kept),
    }
}

impl LiveScreen {
    pub fn new(team_id: String, match_id: String, opponent: String, dir: PathBuf) -> LiveScreen {
        let config = LiveConfig::read(&dir);
        let free = disk::free(&config.folder(&dir));
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
            here: None,
            qr: None,
            free,
        }
    }

    /// The QR code for the address written, made again when it changes.
    fn refresh_qr(&mut self) {
        let host = self.here.clone().unwrap_or_else(|| "127.0.0.1".to_string());
        let link = match self.written() {
            Some(Source::Receiver(r)) => Some(rtmp::larix_link(&r.address(&host))),
            _ => None,
        };
        let same = self.qr.as_ref().map(|(l, _)| l) == link.as_ref();
        if !same {
            self.qr = link.and_then(|link| {
                rtmp::qr_rgba(&link, 4)
                    .map(|(w, h, rgba)| (link, image::Handle::from_rgba(w, h, rgba)))
            });
        }
    }

    /// The form of the source on the network: as used last time, its
    /// password from the keychain; the authentication on if it had a user.
    fn network_form(&self, s: &Strings) -> Form {
        let saved = self.config.network.clone().unwrap_or_default();
        let kept = super::kept_password(&saved.url, &saved.username).unwrap_or_default();
        let (url, password) = given_back(&saved, kept);
        let asked = !saved.username.is_empty();
        let mut form = Form::new(
            s.source_network,
            vec![
                Field::text(s.network_address, url)
                    .placeholder("rtsp://192.168.1.20:554/stream1")
                    .hint(s.network_address_hint),
                Field::check(s.authentication, asked, false).hint(s.authentication_hint),
                Field::text(s.username, saved.username),
                Field::secret(s.password, password).hint(s.password_kept),
            ],
        );
        form.actions = Some((
            (theme::icons::BACK, s.back.to_string()),
            (theme::icons::PLAY, s.try_source.to_string()),
        ));
        form.shown = Some(Self::network_fields(&form));
        form
    }

    /// The fields of the source on the network shown: no user for RTMP
    /// (it's in the address), none unless asked for.
    fn network_fields(form: &Form) -> std::ops::Range<usize> {
        let rtmp = network::parse(&form.text(ADDRESS)).is_ok_and(|(_, p, _)| p == Protocol::Rtmp);
        match (rtmp, Self::authenticated(form)) {
            (true, _) => ADDRESS..AUTHENTICATION,
            (false, false) => ADDRESS..USERNAME,
            (false, true) => ADDRESS..PASSWORD + 1,
        }
    }

    fn authenticated(form: &Form) -> bool {
        matches!(
            form.fields.get(AUTHENTICATION).map(|f| &f.value),
            Some(form::Value::Check { on: true, .. })
        )
    }

    /// The form of the app sending: the port and the key of last time, or
    /// RTMP's and a new one.
    fn receiver_form(&self, s: &Strings) -> Form {
        let saved = self.saved_receiver();
        let mut form = Form::new(
            s.source_receiver,
            vec![
                Field::text(s.receiver_port, saved.port.to_string()),
                Field::text(s.receiver_key, saved.key).hint(s.receiver_key_hint),
            ],
        );
        form.actions = Some((
            (theme::icons::BACK, s.back.to_string()),
            (theme::icons::PLAY, s.receiver_listen.to_string()),
        ));
        form
    }

    fn saved_receiver(&self) -> SavedReceiver {
        self.config
            .receiver
            .clone()
            .unwrap_or_else(|| SavedReceiver {
                port: rtmp::RTMP_PORT,
                key: rtmp::new_key(),
            })
    }

    /// The source written in the form: on the network, the user only if
    /// it's asked for; the app, on a port that is one.
    fn written(&self) -> Option<Source> {
        match self.step {
            Step::Network => {
                let asked = Self::network_fields(&self.form).contains(&USERNAME);
                Some(Source::Network(NetworkSource {
                    url: self.form.text(ADDRESS).trim().to_string(),
                    username: match asked {
                        true => self.form.text(USERNAME).trim().to_string(),
                        false => String::new(),
                    },
                    password: match asked {
                        true => self.form.text(PASSWORD),
                        false => String::new(),
                    },
                }))
            }
            Step::Receiver => {
                let port = self.form.text(PORT).trim().parse::<u16>().ok()?;
                let key = self.form.text(KEY).trim().to_string();
                (port > 0 && !key.is_empty()).then_some(Source::Receiver(Receiver { port, key }))
            }
            Step::Source | Step::Camera => None,
        }
    }

    /// The source written tried: opened, its preview as it comes. Kept
    /// for next time, the password in the keychain.
    fn try_written(&mut self, s: &Strings) {
        let Some(source) = self.written() else {
            self.form.error = Some(format!("{}: 1–65535", s.receiver_port));
            return;
        };
        self.form.error = None;
        self.capture = None;
        self.shown = None;
        self.status = None;
        self.requested = 0;
        match &source {
            Source::Network(network) => {
                let (url, password) = kept_apart(network);
                self.config.network = Some(SavedNetwork {
                    url: url.clone(),
                    username: network.username.clone(),
                });
                if let Err(e) = super::keep_password(&url, &network.username, &password) {
                    self.error = Some(e);
                }
            }
            Source::Receiver(receiver) => {
                self.config.receiver = Some(SavedReceiver {
                    port: receiver.port,
                    key: receiver.key.clone(),
                });
            }
            Source::Camera(_) => {}
        }
        self.save_config();
        self.capture = Some(Capture::open(source));
    }

    /// Whether what's on screen is the source written: then Enter records.
    fn tried(&self) -> bool {
        let written = self.written();
        self.capture
            .as_ref()
            .is_some_and(|c| Some(&c.source) == written.as_ref())
    }

    /// The text field the keys go to, in the form.
    pub fn focus_id(&self) -> Option<iced::widget::Id> {
        match self.step {
            Step::Network | Step::Receiver => Some(self.form.focus_id()),
            Step::Source | Step::Camera => None,
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
            // listening at once: the app sends when it likes
            Kind::Receiver => {
                self.form = self.receiver_form(s);
                self.step = Step::Receiver;
                self.here = rtmp::this_computer().map(|ip| ip.to_string());
                self.try_written(s);
                self.refresh_qr();
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
        self.free = disk::free(&self.config.folder(&self.dir));
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
            Step::Camera | Step::Network | Step::Receiver => {
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
        self.status.as_ref().is_some_and(|s| s.frame.is_some()) && self.room().can_record()
    }

    /// What the free space allows: the camera's bits as chosen, the
    /// network's as they come.
    fn room(&self) -> Room {
        let bits = match self.step {
            Step::Camera => Some(u64::from(self.config.quality().bits_per_second())),
            _ => self.status.as_ref().and_then(|st| st.bits_per_second),
        };
        Room::of(self.free, bits)
    }

    /// The free space said, in red when a match doesn't fit.
    fn room_text(&self, s: &Strings) -> Option<(String, bool)> {
        match self.room() {
            Room::Unknown => None,
            Room::Enough { free } => Some((ui::fill(s.space_free, &[disk::gb(free)]), false)),
            Room::Short { free, needed } => Some((
                ui::fill(s.space_short, &[disk::gb(free), disk::gb(needed)]),
                true,
            )),
            Room::Full { free } => Some((ui::fill(s.space_full, &[disk::gb(free)]), true)),
        }
    }

    fn room_view<'a>(&self, s: &Strings, c: Colors) -> Element<'a, Message> {
        match self.room_text(s) {
            Some((said, short)) => text(said)
                .size(SIZE_SMALL)
                .color(match short {
                    true => c.danger,
                    false => c.muted,
                })
                .into(),
            None => space::vertical().height(0).into(),
        }
    }

    pub fn update(&mut self, message: Message, s: &Strings) -> Outcome {
        match message {
            Message::Kind(kind) => return self.choose_kind(kind, s),
            Message::Form(m) => {
                let outcome = self.form.update(m);
                return self.form_outcome(outcome, s);
            }
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
                self.free = disk::free(&folder);
                self.config.folder = Some(folder);
                self.save_config();
            }
            Message::Folder(None) => {}
            Message::Start if self.can_start() && (self.step == Step::Camera || self.tried()) => {
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

    /// Enter on the network or the app: tried, then (seen) recorded.
    fn enter(&mut self, s: &Strings) -> Outcome {
        match (self.tried(), self.can_start()) {
            (true, true) => Outcome::Record,
            (true, false) => Outcome::Stay,
            (false, _) => {
                self.try_written(s);
                Outcome::Stay
            }
        }
    }

    /// What the form did: the fields shown again as the address and the
    /// authentication change.
    fn form_outcome(&mut self, outcome: form::Outcome, s: &Strings) -> Outcome {
        match outcome {
            form::Outcome::Save => self.enter(s),
            form::Outcome::Cancel => self.back(),
            form::Outcome::Changed(_) if self.step == Step::Network => {
                self.form.shown = Some(Self::network_fields(&self.form));
                Outcome::Stay
            }
            form::Outcome::Changed(_) if self.step == Step::Receiver => {
                self.refresh_qr();
                Outcome::Stay
            }
            form::Outcome::Stay | form::Outcome::Changed(_) => Outcome::Stay,
        }
    }

    pub fn key(&mut self, ck: KeyCombination, s: &Strings) -> Outcome {
        let quality = self.config.quality();
        match (self.step, ck) {
            // the form takes the keys, Esc and Enter too
            (Step::Network | Step::Receiver, ck) => {
                let outcome = self.form.key(ck);
                self.form_outcome(outcome, s)
            }
            (_, k) if k == BACK_KEY => self.back(),
            (Step::Source, k) if k == key!(up) || k == key!(down) => {
                self.kind = ui::cycle(&KINDS, &self.kind, k == key!(down)).unwrap_or(self.kind);
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
            Step::Network | Step::Receiver => vec![
                hint("↑ ↓ ⇥", s.next),
                hint(
                    "Enter",
                    match (self.tried() && self.can_start(), self.step) {
                        (true, _) => s.start_recording,
                        (false, Step::Receiver) => s.receiver_listen,
                        (false, _) => s.try_source,
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
            Step::Receiver => self.receiver_view(s, c),
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

    /// What's wrong with the source on the network, if anything.
    fn notice(&self, s: &Strings) -> Option<String> {
        match (&self.error, self.status.as_ref()) {
            (Some(e), _) => Some(e.clone()),
            (
                None,
                Some(Status {
                    problem: Some(p), ..
                }),
            ) => Some(problem_text(p, s)),
            (None, Some(Status { error: Some(e), .. })) => Some(e.clone()),
            _ => None,
        }
    }

    /// What the source gives, once seen.
    fn seen(&self, s: &Strings) -> String {
        self.status
            .as_ref()
            .and_then(|st| st.size)
            .filter(|_| self.shown.is_some())
            .map(|(w, h)| format!("H.264 · {w}×{h} · {}", s.as_it_comes))
            .unwrap_or_default()
    }

    fn network_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let notice = self.notice(s);
        let waiting = match self.capture {
            Some(_) => s.connecting,
            None => s.network_address_hint,
        };
        let folder = self.config.folder(&self.dir);
        let aside = column![
            text(self.seen(s)).size(SIZE_SMALL).color(c.muted),
            ui::section(s.recordings_folder, c),
            text(folder.display().to_string()).size(SIZE_SMALL),
            self.room_view(s, c),
            ui::pill(
                &ui::capitalized(s.change),
                false,
                Some(Message::ChangeFolder),
                c
            ),
            ui::answers(
                None,
                theme::icons::RECORD,
                s.start_recording,
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

    /// Where the app sends (the server and the key apart, as OBS wants
    /// them, and whole, as Larix), who's sending, the folder.
    fn receiver_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let status = self.status.as_ref();
        let sender = status.and_then(|st| st.sender.clone());
        let notice = self.notice(s).filter(|_| sender.is_none());
        let host = self.here.clone().unwrap_or_else(|| "127.0.0.1".to_string());
        let written = match self.written() {
            Some(Source::Receiver(r)) => Some(r),
            _ => None,
        };
        let line = |label: &'static str, value: String| {
            column![
                text(label).size(SIZE_SMALL).color(c.muted),
                // an address has no spaces: cut anywhere, not out of sight
                text(value)
                    .size(SIZE_BODY)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            ]
            .spacing(2)
        };
        let address: Element<'a, Message> = match written {
            Some(r) => column![
                line(s.receiver_server, r.server(&host)),
                line(s.receiver_key, r.key.clone()),
                line(s.receiver_whole, r.address(&host)),
            ]
            .spacing(8)
            .into(),
            None => space::vertical().height(0).into(),
        };
        let state = match (&sender, self.capture.is_some()) {
            (Some(from), _) => ui::fill(s.receiver_from, std::slice::from_ref(from)),
            (None, true) => s.receiver_waiting.to_string(),
            (None, false) => String::new(),
        };
        let qr: Element<'a, Message> = match &self.qr {
            Some((_, handle)) => column![
                image(handle.clone())
                    .width(Length::Fixed(180.0))
                    .filter_method(image::FilterMethod::Nearest),
                text(s.receiver_qr).size(SIZE_SMALL).color(c.muted),
            ]
            .spacing(6)
            .into(),
            None => space::vertical().height(0).into(),
        };
        let folder = self.config.folder(&self.dir);
        let aside = column![
            ui::section(s.receiver_in_app, c),
            qr,
            address,
            text(s.receiver_firewall).size(SIZE_SMALL).color(c.muted),
            text(state).size(SIZE_SMALL),
            text(self.seen(s)).size(SIZE_SMALL).color(c.muted),
            ui::section(s.recordings_folder, c),
            text(folder.display().to_string()).size(SIZE_SMALL),
            self.room_view(s, c),
            ui::pill(
                &ui::capitalized(s.change),
                false,
                Some(Message::ChangeFolder),
                c
            ),
            ui::answers(
                None,
                theme::icons::RECORD,
                s.start_recording,
                (self.tried() && self.can_start()).then_some(Message::Start),
                c
            ),
        ]
        .spacing(10);
        row![
            self.preview(notice, s.receiver_waiting, c),
            container(self.form.view(s, c, Message::Form, Some(aside.into())))
                .width(Length::Fixed(600.0))
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
            choice(
                Kind::Receiver,
                s.source_receiver,
                s.source_receiver_hint,
                true
            ),
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
            self.room_view(s, c),
            ui::pill(
                &ui::capitalized(s.change),
                false,
                Some(Message::ChangeFolder),
                c
            ),
            space::vertical(),
            ui::answers(
                None,
                theme::icons::RECORD,
                s.start_recording,
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
        // no user until the authentication is on
        assert_eq!(screen.form.shown, Some(ADDRESS..USERNAME));
        screen.update(Message::Form(form::Message::Choose(AUTHENTICATION, 1)), s());
        assert_eq!(screen.form.shown, Some(ADDRESS..PASSWORD + 1));
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

    /// The user and the password asked for when the source had them,
    /// not for RTMP (in its address), and not tried when hidden.
    #[test]
    fn the_authentication() {
        let dir = tempfile::tempdir().unwrap();
        let config = LiveConfig {
            network: Some(SavedNetwork {
                url: "127.0.0.1:1/cam".into(),
                username: "admin".into(),
            }),
            ..Default::default()
        };
        config.write(dir.path()).unwrap();
        let mut screen = screen(&dir);
        screen.update(Message::Kind(Kind::Network), s());
        assert_eq!(screen.form.shown, Some(ADDRESS..PASSWORD + 1));
        screen.update(
            Message::Form(form::Message::Input(
                ADDRESS,
                "rtmp://server/live/key".into(),
            )),
            s(),
        );
        assert_eq!(screen.form.shown, Some(ADDRESS..AUTHENTICATION));
        let Some(Source::Network(written)) = screen.written() else {
            panic!("no source");
        };
        assert_eq!(written.username, "");
        // off by the keys: Tab to it, space
        screen.update(
            Message::Form(form::Message::Input(ADDRESS, "127.0.0.1:1/cam".into())),
            s(),
        );
        screen.key(key!(tab), s());
        assert_eq!(screen.form.focused, AUTHENTICATION);
        screen.key(key!(space), s());
        assert_eq!(screen.form.shown, Some(ADDRESS..USERNAME));
        let Some(Source::Network(written)) = screen.written() else {
            panic!("no source");
        };
        assert_eq!(
            (written.username.as_str(), written.password.as_str()),
            ("", "")
        );
    }

    /// The query of an RTMP address (its user and password) kept in the
    /// keychain, not in the file; given back the next time.
    #[test]
    fn a_password_in_the_address() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = screen(&dir);
        screen.update(Message::Kind(Kind::Network), s());
        let url = "rtmp://127.0.0.1:1/live/cam?user=scout&pass=segreta";
        screen.update(
            Message::Form(form::Message::Input(ADDRESS, url.into())),
            s(),
        );
        screen.key(key!(enter), s());
        let text = std::fs::read_to_string(dir.path().join(super::super::CONFIG_FILE)).unwrap();
        assert!(!text.contains("segreta"), "{text}");
        let saved = LiveConfig::read(dir.path()).network.unwrap();
        assert_eq!(saved.url, "rtmp://127.0.0.1:1/live/cam");
        // what the keychain keeps (its mock in the tests keeps nothing)
        let Some(Source::Network(written)) = screen.written() else {
            panic!("no source");
        };
        let (_, kept) = kept_apart(&written);
        assert_eq!(kept, "?user=scout&pass=segreta");
        assert_eq!(given_back(&saved, kept), (url.to_string(), String::new()));
        // RTSP: its own password, the address as it is
        let rtsp = NetworkSource {
            url: "cam.local/live".into(),
            username: "admin".into(),
            password: "p?w".into(),
        };
        assert_eq!(kept_apart(&rtsp), ("cam.local/live".into(), "p?w".into()));
    }

    /// An app sending: listening at once on the port and with the key of
    /// last time; changed, Enter listens again; a port that isn't one
    /// said; nothing seen, nothing recorded.
    #[test]
    fn an_app_sending() {
        let dir = tempfile::tempdir().unwrap();
        let config = LiveConfig {
            receiver: Some(SavedReceiver {
                port: 19361,
                key: "chiave01".into(),
            }),
            ..Default::default()
        };
        config.write(dir.path()).unwrap();
        let mut screen = screen(&dir);
        screen.key(key!(down), s());
        screen.key(key!(down), s());
        assert_eq!(screen.kind, Kind::Receiver);
        assert!(matches!(screen.key(key!(enter), s()), Outcome::Stay));
        assert_eq!(screen.step, Step::Receiver);
        assert!(screen.tried());
        let link = screen.qr.as_ref().unwrap().0.clone();
        assert!(link.contains("chiave01"), "{link}");
        assert!(matches!(screen.key(key!(enter), s()), Outcome::Stay));
        assert!(matches!(screen.update(Message::Start, s()), Outcome::Stay));
        screen.update(
            Message::Form(form::Message::Input(PORT, "19362".into())),
            s(),
        );
        assert!(!screen.tried());
        // the QR follows what's written
        assert!(screen.qr.as_ref().unwrap().0.contains("19362"));
        screen.key(key!(enter), s());
        assert!(screen.tried());
        assert_eq!(LiveConfig::read(dir.path()).receiver.unwrap().port, 19362);
        screen.update(
            Message::Form(form::Message::Input(PORT, "porta".into())),
            s(),
        );
        screen.key(key!(enter), s());
        assert!(screen.form.error.is_some());
        assert!(matches!(screen.key(BACK_KEY, s()), Outcome::Stay));
        assert_eq!(screen.step, Step::Source);
        // round: from the last kind to the first
        screen.key(key!(down), s());
        assert_eq!(screen.kind, Kind::Camera);
    }

    /// A key made the first time, kept.
    #[test]
    fn a_key_the_first_time() {
        let dir = tempfile::tempdir().unwrap();
        let screen = screen(&dir);
        let receiver = screen.saved_receiver();
        assert_eq!(receiver.port, rtmp::RTMP_PORT);
        assert_eq!(receiver.key.len(), 8);
    }

    /// The free space said before recording: what a match needs at the
    /// quality chosen (the camera) or as the source comes (the network);
    /// not started without even the reserve.
    #[test]
    fn the_space_before_recording() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = screen(&dir);
        screen.update(Message::Kind(Kind::Camera), s());
        let seen = Status {
            frame: Some(Arc::new(Picture {
                serial: 1,
                width: 2,
                height: 2,
                rgba: bytes::Bytes::from(vec![0; 16]),
            })),
            ..Status::default()
        };
        screen.status = Some(seen.clone());
        // 720p: 6.3 GB for a match, with its MP4
        screen.free = Some(100_000_000_000);
        assert_eq!(
            screen.room_text(s()),
            Some(("100.0 GB liberi".to_string(), false))
        );
        screen.free = Some(4_000_000_000);
        let (said, short) = screen.room_text(s()).unwrap();
        assert!(
            short && said.contains("4.0") && said.contains("6.8"),
            "{said}"
        );
        assert!(matches!(
            screen.update(Message::Start, s()),
            Outcome::Record
        ));
        // lower: fits
        screen.key(key!(q), s());
        assert_eq!(screen.room_text(s()).map(|(_, short)| short), Some(false));
        screen.free = Some(disk::RESERVE - 1);
        assert_eq!(screen.room_text(s()).map(|(_, short)| short), Some(true));
        assert!(matches!(screen.update(Message::Start, s()), Outcome::Stay));
        // on the network: from its bitrate, once counted
        screen.update(Message::Kind(Kind::Network), s());
        screen.free = Some(4_000_000_000);
        screen.status = Some(seen.clone());
        assert_eq!(screen.room_text(s()).map(|(_, short)| short), Some(false));
        screen.status = Some(Status {
            bits_per_second: Some(8_000_000),
            ..seen
        });
        assert_eq!(screen.room_text(s()).map(|(_, short)| short), Some(true));
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
