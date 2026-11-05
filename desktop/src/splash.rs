//! The splash, while the data is read: the ball comes in with a turn, as a
//! serve does (the splash of the Android app), then the name and an amber
//! line drawn as the line of a court.

use {
    crate::{theme, ui},
    iced::{
        Alignment, Element, Length, Radians,
        widget::{column, container, space, text},
    },
    std::time::{Duration, Instant},
};

/// How long the ball takes to come in, as on Android.
const SPIN: Duration = Duration::from_millis(700);
/// The splash stays at least this long, not to flash by.
const SHOWN: Duration = Duration::from_millis(1200);
const BALL: f32 = 150.0;
const NAME: f32 = 34.0;

/// The time counts from the first frame: the window takes a while to
/// open.
pub struct Splash {
    started: Option<Instant>,
    now: Instant,
}

/// From 0 to 1 between `from` and `from + length`, slowing at the end.
fn eased(elapsed: Duration, from: Duration, length: Duration) -> f32 {
    let t = elapsed.saturating_sub(from).as_secs_f32() / length.as_secs_f32();
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

impl Splash {
    pub fn new() -> Self {
        Splash {
            started: None,
            now: Instant::now(),
        }
    }

    pub fn tick(&mut self, now: Instant) {
        self.started = self.started.or(Some(now));
        self.now = now;
    }

    fn elapsed(&self) -> Duration {
        self.started
            .map_or(Duration::ZERO, |started| self.now.duration_since(started))
    }

    pub fn done(&self) -> bool {
        self.elapsed() >= SHOWN
    }

    pub fn view<'a, M: 'a>(&self) -> Element<'a, M> {
        let c = theme::DARK;
        let elapsed = self.elapsed();
        let spin = eased(elapsed, Duration::ZERO, SPIN);
        let name = eased(
            elapsed,
            Duration::from_millis(350),
            Duration::from_millis(500),
        );
        let size = BALL * (0.7 + 0.3 * spin);
        let ball = container(
            ui::ball(size, c)
                .rotation(Radians(-120.0_f32.to_radians() * (1.0 - spin)))
                .opacity(spin.max(0.2)),
        )
        .width(Length::Fixed(BALL))
        .height(Length::Fixed(BALL))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);
        let width = NAME * 9112.0 / 1512.0;
        let court_line = container(space::horizontal().width(0))
            .width(Length::Fixed(width * name))
            .height(Length::Fixed(3.0))
            .style(move |_| container::Style {
                background: Some(c.accent.into()),
                ..container::Style::default()
            });
        container(
            column![
                ball,
                space::vertical().height(28),
                ui::wordmark(NAME, c).opacity(name),
                container(court_line)
                    .width(Length::Fixed(width))
                    .align_x(Alignment::Center),
                text(format!("v{}", scout4all_mobile::version()))
                    .size(theme::SIZE_SMALL)
                    .color(iced::Color { a: name, ..c.muted }),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(c.bar.into()),
            ..container::Style::default()
        })
        .into()
    }
}
