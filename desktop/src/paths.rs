//! Where our serves went from where they started, on the whole court drawn
//! with the canvas of iced: their half at the top as our server sees it,
//! ours below, the zones of our end line under it; an arrow from each zone
//! our serves started from to each zone of their court they went to, the
//! more the thicker, in the color of their efficiency (grey on too few to
//! tell). Their zones lightly lit by how many went there. Under the mouse,
//! what an arrow tells; a click on a zone of our end line shows only its
//! arrows, on an arrow picks its serves.

use {
    crate::theme::{Colors, grade_color},
    iced::{
        Color, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke, Text},
    },
    scout4all_core::shapes::court_zone::{CourtZoneEnum, ServeStartEnum},
    scout4all_mobile::{ServePathStats, ServeStartStats, TargetStats},
};

/// The court is 9 by 18 meters; a margin around it, and under our end line
/// the zones our serve starts from.
const WIDTH: f32 = 9.0;
const LENGTH: f32 = 18.0;
const MARGIN: f32 = 0.5;
const GAP: f32 = 0.4;
const STRIP: f32 = 2.6;

#[derive(Debug, Clone)]
pub enum Pick {
    Start(ServeStartEnum),
    Path(ServeStartEnum, CourtZoneEnum),
}

pub struct PathsChart<'a> {
    pub paths: &'a [ServePathStats],
    pub starts: &'a [ServeStartStats],
    /// their court, a row each from their end line
    pub targets: &'a [Vec<TargetStats>],
    /// the zone of our end line picked, the arrow picked
    pub start: Option<ServeStartEnum>,
    pub path: Option<(ServeStartEnum, CourtZoneEnum)>,
    pub colors: Colors,
}

/// What's under the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Hover {
    #[default]
    None,
    Start(usize),
    Path(usize),
}

/// Where the court is drawn: the top left corner of the margin and the
/// size of a meter.
#[derive(Debug, Clone, Copy)]
struct Place {
    origin: Point,
    meter: f32,
}

impl Place {
    fn of(bounds: Size) -> Self {
        let across = WIDTH + 2.0 * MARGIN;
        let down = LENGTH + GAP + STRIP + 2.0 * MARGIN;
        let meter = (bounds.width / across).min(bounds.height / down);
        Place {
            origin: Point::new(
                (bounds.width - across * meter) / 2.0,
                (bounds.height - down * meter) / 2.0,
            ),
            meter,
        }
    }

    /// The point `x`, `y` meters from the top left corner of the court.
    fn at(&self, x: f32, y: f32) -> Point {
        Point::new(
            self.origin.x + (MARGIN + x) * self.meter,
            self.origin.y + (MARGIN + y) * self.meter,
        )
    }

    fn rect(&self, x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle::new(
            self.at(x, y),
            Size::new(width * self.meter, height * self.meter),
        )
    }

    /// The zone of our end line, `i` from the left.
    fn start(&self, i: usize, of: usize) -> Rectangle {
        let width = WIDTH / of.max(1) as f32;
        self.rect(i as f32 * width, LENGTH + GAP, width, STRIP)
    }
}

impl PathsChart<'_> {
    /// The most of an arrow, to size the others by.
    fn most(&self) -> f64 {
        self.paths.iter().map(|p| p.count).fold(0.0, f64::max).max(1.0)
    }

    /// The ends of an arrow: from the top of its zone of our end line to
    /// the middle of its zone of theirs.
    fn ends(&self, place: Place, p: &ServePathStats) -> (Point, Point) {
        (
            place.at(p.from_x as f32, LENGTH + GAP),
            place.at(p.to_x as f32, p.to_y as f32),
        )
    }

    fn width(&self, place: Place, p: &ServePathStats) -> f32 {
        let share = (p.count / self.most()) as f32;
        (0.06 + 0.26 * share) * place.meter
    }

    fn color(&self, p: &ServePathStats) -> Color {
        match p.efficiency {
            Some(e) => efficiency_color(e, self.colors),
            None => self.colors.muted,
        }
    }

    fn hovered(&self, bounds: Size, at: Point) -> Hover {
        let place = Place::of(bounds);
        let start = (0..self.starts.len()).find(|i| place.start(*i, self.starts.len()).contains(at));
        // the nearest arrow within reach, the thinnest drawn last on top
        let path = self
            .paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (from, to) = self.ends(place, p);
                (i, distance(at, from, to), self.width(place, p))
            })
            .filter(|(_, d, w)| *d <= w.max(8.0))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _, _)| i);
        match (start, path) {
            (Some(i), _) => Hover::Start(i),
            (None, Some(i)) => Hover::Path(i),
            (None, None) => Hover::None,
        }
    }
}

/// The color of an efficiency, -100 to 100: red to green.
fn efficiency_color(efficiency: f64, c: Colors) -> Color {
    grade_color((efficiency + 100.0) / 200.0, c)
}

/// How far `at` is from the segment from `a` to `b`.
fn distance(at: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = match length > 0.0 {
        true => (((at.x - a.x) * dx + (at.y - a.y) * dy) / length).clamp(0.0, 1.0),
        false => 0.0,
    };
    at.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// How many, whole or with the half of two cells of two zones.
pub fn count(count: f64) -> String {
    match count.fract() == 0.0 {
        true => format!("{count:.0}"),
        false => format!("{count:.1}"),
    }
}

impl canvas::Program<Pick> for PathsChart<'_> {
    type State = Hover;

    fn update(
        &self,
        state: &mut Hover,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Pick>> {
        let at = cursor.position_in(bounds);
        match (event, at) {
            (Event::Mouse(mouse::Event::CursorMoved { .. }), Some(at)) => {
                let hover = self.hovered(bounds.size(), at);
                (hover != *state).then(|| {
                    *state = hover;
                    Action::request_redraw()
                })
            }
            (Event::Mouse(mouse::Event::CursorMoved { .. }), None) if *state != Hover::None => {
                *state = Hover::None;
                Some(Action::request_redraw())
            }
            (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(at)) => {
                match self.hovered(bounds.size(), at) {
                    Hover::Start(i) => self
                        .starts
                        .get(i)
                        .map(|s| Action::publish(Pick::Start(s.value)).and_capture()),
                    Hover::Path(i) => self
                        .paths
                        .get(i)
                        .map(|p| Action::publish(Pick::Path(p.start, p.zone)).and_capture()),
                    Hover::None => None,
                }
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Hover,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match state {
            Hover::None => mouse::Interaction::default(),
            Hover::Start(_) | Hover::Path(_) => mouse::Interaction::Pointer,
        }
    }

    fn draw(
        &self,
        state: &Hover,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let place = Place::of(bounds.size());
        self.draw_court(&mut frame, place);
        self.draw_starts(&mut frame, place, *state);
        self.draw_paths(&mut frame, place, *state);
        self.draw_numbers(&mut frame, place);
        self.draw_told(&mut frame, place, *state);
        vec![frame.into_geometry()]
    }
}

impl PathsChart<'_> {
    fn draw_court(&self, frame: &mut Frame, place: Place) {
        let c = self.colors;
        let court = place.rect(0.0, 0.0, WIDTH, LENGTH);
        frame.fill_rectangle(court.position(), court.size(), c.court);
        // their zones lit by how many went there
        let most = self
            .targets
            .iter()
            .flatten()
            .map(|t| t.count)
            .fold(0.0, f64::max)
            .max(1.0);
        for (r, row) in self.targets.iter().enumerate() {
            for (col, t) in row.iter().enumerate() {
                let zone = place.rect(col as f32 * 3.0, r as f32 * 3.0, 3.0, 3.0);
                if t.count > 0.0 {
                    let alpha = 0.08 + 0.32 * (t.count / most) as f32;
                    frame.fill_rectangle(zone.position(), zone.size(), Color { a: alpha, ..c.accent });
                }
                frame.fill_text(Text {
                    font: crate::theme::fonts().regular,
                    content: t.label.clone(),
                    position: Point::new(zone.x + 4.0, zone.y + 3.0),
                    color: c.muted,
                    size: (place.meter * 0.32).clamp(9.0, 13.0).into(),
                    ..Text::default()
                });
            }
        }
        let thin = Stroke::default()
            .with_color(Color { a: 0.35, ..c.court_lines })
            .with_width(1.0);
        for i in 1..3 {
            let x = i as f32 * 3.0;
            frame.stroke(&Path::line(place.at(x, 0.0), place.at(x, 9.0)), thin);
            frame.stroke(&Path::line(place.at(0.0, x), place.at(WIDTH, x)), thin);
        }
        let lines = Stroke::default().with_color(c.court_lines).with_width(2.0);
        frame.stroke(&Path::rectangle(court.position(), court.size()), lines);
        // the attack lines, 3 meters from the net; the net
        for y in [6.0, 12.0] {
            frame.stroke(&Path::line(place.at(0.0, y), place.at(WIDTH, y)), lines);
        }
        frame.stroke(
            &Path::line(place.at(-MARGIN / 2.0, 9.0), place.at(WIDTH + MARGIN / 2.0, 9.0)),
            lines.with_width(4.0),
        );
    }

    fn draw_starts(&self, frame: &mut Frame, place: Place, hover: Hover) {
        let c = self.colors;
        let size = (place.meter * 0.45).clamp(10.0, 16.0);
        for (i, s) in self.starts.iter().enumerate() {
            let r = place.start(i, self.starts.len());
            let picked = self.start == Some(s.value);
            let fill = match (picked, hover == Hover::Start(i)) {
                (true, _) => Color { a: 0.55, ..c.accent },
                (false, true) => c.button,
                (false, false) => c.court,
            };
            let inner = Rectangle::new(
                Point::new(r.x + 2.0, r.y),
                Size::new(r.width - 4.0, r.height),
            );
            frame.fill_rectangle(inner.position(), inner.size(), fill);
            frame.stroke(
                &Path::rectangle(inner.position(), inner.size()),
                Stroke::default().with_color(c.border).with_width(1.0),
            );
            // its name; how many, their efficiency, the errors (which went
            // nowhere on their court), when there are
            let small = size * 0.75;
            let lines = [
                Some((s.label.clone(), c.text, size, true)),
                (s.count > 0).then(|| (s.count.to_string(), c.text, small, false)),
                s.efficiency.map(|e| (format!("{e:+.0}%"), efficiency_color(e, c), small, true)),
                (s.errors > 0).then(|| (format!("= {}", s.errors), grade_color(0.0, c), small, false)),
            ];
            for (n, (content, color, size, bold)) in lines.into_iter().flatten().enumerate() {
                frame.fill_text(Text {
                    font: match bold {
                        true => crate::theme::fonts().bold,
                        false => crate::theme::fonts().regular,
                    },
                    content,
                    position: Point::new(r.center_x(), r.y + r.height * (0.17 + 0.22 * n as f32)),
                    color,
                    size: size.into(),
                    align_x: iced::widget::text::Alignment::Center,
                    align_y: iced::alignment::Vertical::Center,
                    ..Text::default()
                });
            }
        }
    }

    /// In each zone of their court served to, at its top right corner over
    /// the arrows: how many went there and, from a few on, their
    /// efficiency.
    fn draw_numbers(&self, frame: &mut Frame, place: Place) {
        let c = self.colors;
        let size = (place.meter * 0.4).clamp(10.0, 15.0);
        for (r, row) in self.targets.iter().enumerate() {
            for (col, t) in row.iter().enumerate().filter(|(_, t)| t.count > 0.0) {
                let lines: Vec<(String, Color)> = [
                    Some((count(t.count), c.text)),
                    t.efficiency.map(|e| (format!("{e:+.0}%"), efficiency_color(e, c))),
                ]
                .into_iter()
                .flatten()
                .collect();
                let widest = lines.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0);
                let width = widest as f32 * size * 0.62 + 8.0;
                let height = lines.len() as f32 * size * 1.2 + 4.0;
                let corner = place.at(col as f32 * 3.0 + 3.0, r as f32 * 3.0);
                let pill = Rectangle::new(
                    Point::new(corner.x - width - 3.0, corner.y + 3.0),
                    Size::new(width, height),
                );
                frame.fill(
                    &Path::rounded_rectangle(pill.position(), pill.size(), 4.0.into()),
                    Color { a: 0.85, ..c.panel },
                );
                for (n, (content, color)) in lines.into_iter().enumerate() {
                    frame.fill_text(Text {
                        font: crate::theme::fonts().bold,
                        content,
                        position: Point::new(
                            pill.x + pill.width - 4.0,
                            pill.y + 2.0 + size * (0.6 + 1.2 * n as f32),
                        ),
                        color,
                        size: size.into(),
                        align_x: iced::widget::text::Alignment::Right,
                        align_y: iced::alignment::Vertical::Center,
                        ..Text::default()
                    });
                }
            }
        }
    }

    fn draw_paths(&self, frame: &mut Frame, place: Place, hover: Hover) {
        let c = self.colors;
        // the fewest last: on top of the thicker ones
        for (i, p) in self.paths.iter().enumerate() {
            let (from, to) = self.ends(place, p);
            let width = self.width(place, p);
            let picked = self.path == Some((p.start, p.zone));
            let lit = picked || hover == Hover::Path(i);
            if lit {
                frame.stroke(
                    &Path::line(from, to),
                    Stroke::default()
                        .with_color(c.text)
                        .with_width(width + 4.0),
                );
            }
            let color = self.color(p);
            // the line stops where the head starts
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            let length = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / length, dy / length);
            let head = (width * 2.2).max(8.0);
            let base = Point::new(to.x - ux * head, to.y - uy * head);
            frame.stroke(
                &Path::line(from, base),
                Stroke::default().with_color(color).with_width(width),
            );
            let half = head * 0.6;
            let arrow = Path::new(|b| {
                b.move_to(to);
                b.line_to(Point::new(base.x - uy * half, base.y + ux * half));
                b.line_to(Point::new(base.x + uy * half, base.y - ux * half));
                b.close();
            });
            frame.fill(&arrow, color);
        }
    }

    /// What the arrow under the mouse tells, at the top, over everything.
    fn draw_told(&self, frame: &mut Frame, place: Place, hover: Hover) {
        let c = self.colors;
        let told = match hover {
            Hover::Path(i) => self.paths.get(i),
            Hover::None | Hover::Start(_) => None,
        };
        if let Some(p) = told {
            let efficiency = p
                .efficiency
                .map_or(String::new(), |e| format!(" · {e:+.0}%"));
            let text = format!("{} · {} ({:.0}%){efficiency}", p.label, count(p.count), p.share);
            let size = (place.meter * 0.42).clamp(11.0, 15.0);
            let at = place.at(0.0, 0.0);
            let box_ = Rectangle::new(
                Point::new(at.x + 2.0, at.y + 2.0),
                Size::new(text.chars().count() as f32 * size * 0.6 + 12.0, size * 1.5),
            );
            frame.fill_rectangle(box_.position(), box_.size(), c.panel);
            frame.fill_text(Text {
                font: crate::theme::fonts().bold,
                content: text,
                position: Point::new(box_.x + 6.0, box_.center_y()),
                color: c.text,
                size: size.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
        }
    }
}
