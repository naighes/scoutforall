//! The court of the scouting, drawn with the canvas of iced: our half seen
//! from the bench (the net at the top) with the players where they stand,
//! or the court where the ball went while evaluating, with the cursor of
//! the arrows; for our serve, below the net, the zones of our end line it
//! starts from.

use {
    crate::theme::{self, Colors, grade_color},
    iced::{
        Color, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke, Text},
    },
    scout4all_core::shapes::court_zone::ServeStartEnum,
    scout4all_mobile::{CourtCellChoice, CourtPlayer, ServeStartChoice},
};

/// What the court shows.
pub enum Content {
    /// the players; `pickable`, a click chooses one (only when a player
    /// is what's chosen now); `server`, ours about to serve: the ball by
    /// them, in the color of the phase
    Players {
        players: Vec<CourtPlayer>,
        pickable: bool,
        server: Option<(String, Color)>,
    },
    /// rows of cells (one row at the net for our block), the net at the
    /// top when the court is ours; the cell of the cursor; for our serve
    /// the zones it starts from, below the net, and the one of the cursor
    Target {
        rows: Vec<Vec<CourtCellChoice>>,
        ours: bool,
        cursor: Option<(u8, String)>,
        starts: Vec<ServeStartChoice>,
        start_cursor: Option<ServeStartEnum>,
    },
}

pub struct Court {
    pub content: Content,
    pub colors: Colors,
}

#[derive(Debug, Clone)]
pub enum Touch {
    Player(String),
    Cell(u8, String),
    Start(ServeStartEnum),
}

/// Half a court is 9 by 9 meters; a margin around it for the players
/// standing outside to serve or receive.
const METERS: f32 = 9.0;
const MARGIN: f32 = 0.6;
/// Below the net, the zones our serve starts from: as deep, and so far
/// from the net.
const START_DEPTH: f32 = 1.4;
const START_GAP: f32 = 0.5;

impl Court {
    /// The meters below the court: the zones our serve starts from, if
    /// shown.
    fn below(&self) -> f32 {
        match &self.content {
            Content::Target { starts, .. } if !starts.is_empty() => START_GAP + START_DEPTH,
            _ => 0.0,
        }
    }

    /// The square of the court in the bounds, with `below` meters under
    /// it, and the size of a meter.
    fn square(bounds: Size, below: f32) -> (Rectangle, f32) {
        let across = METERS + 2.0 * MARGIN;
        let down = across + below;
        let meter = (bounds.width / across).min(bounds.height / down);
        let origin = Point::new(
            (bounds.width - across * meter) / 2.0 + MARGIN * meter,
            (bounds.height - down * meter) / 2.0 + MARGIN * meter,
        );
        (
            Rectangle::new(origin, Size::new(METERS * meter, METERS * meter)),
            meter,
        )
    }

    /// Each zone our serve starts from with where it's drawn: a row below
    /// the net, as wide as the court.
    fn starts(&self, court: Rectangle, meter: f32) -> Vec<(Rectangle, &ServeStartChoice)> {
        match &self.content {
            Content::Target { starts, .. } => {
                let width = court.width / starts.len().max(1) as f32;
                let y = court.y + court.height + START_GAP * meter;
                starts
                    .iter()
                    .enumerate()
                    .map(|(i, start)| {
                        (
                            Rectangle::new(
                                Point::new(court.x + i as f32 * width, y),
                                Size::new(width, START_DEPTH * meter),
                            ),
                            start,
                        )
                    })
                    .collect()
            }
            Content::Players { .. } => vec![],
        }
    }

    /// Each cell with where it's drawn.
    fn cells(&self, court: Rectangle) -> Vec<(Rectangle, &CourtCellChoice)> {
        match &self.content {
            Content::Players { .. } => vec![],
            Content::Target { rows, .. } => {
                let height = match rows.len() {
                    // the zones at the net: a third of the court deep
                    1 => court.height / 3.0,
                    n => court.height / n as f32,
                };
                rows.iter()
                    .enumerate()
                    .flat_map(|(r, row)| {
                        let width = court.width / row.len().max(1) as f32;
                        row.iter().enumerate().map(move |(c, cell)| {
                            (
                                Rectangle::new(
                                    Point::new(
                                        court.x + c as f32 * width,
                                        court.y + r as f32 * height,
                                    ),
                                    Size::new(width, height),
                                ),
                                cell,
                            )
                        })
                    })
                    .collect()
            }
        }
    }

    /// Where each player is drawn, and the radius.
    fn spots(&self, court: Rectangle, meter: f32) -> Vec<(Point, f32, &CourtPlayer)> {
        match &self.content {
            Content::Players { players, .. } => players
                .iter()
                .map(|p| {
                    (
                        Point::new(court.x + p.x * meter, court.y + p.y * meter),
                        meter * 0.55,
                        p,
                    )
                })
                .collect(),
            Content::Target { .. } => vec![],
        }
    }

    fn touched(&self, bounds: Size, at: Point) -> Option<Touch> {
        let (court, meter) = Self::square(bounds, self.below());
        let pickable = matches!(self.content, Content::Players { pickable: true, .. });
        let player = self
            .spots(court, meter)
            .into_iter()
            .filter(|(_, _, p)| pickable && p.selectable)
            .find(|(center, radius, _)| center.distance(at) <= *radius * 1.2)
            .map(|(_, _, p)| Touch::Player(p.id.clone()));
        let cell = || {
            self.cells(court)
                .into_iter()
                .find(|(r, _)| r.contains(at))
                .map(|(_, c)| Touch::Cell(c.zone, c.subzone.clone()))
        };
        let start = || {
            self.starts(court, meter)
                .into_iter()
                .find(|(r, _)| r.contains(at))
                .map(|(_, z)| Touch::Start(z.value))
        };
        player.or_else(cell).or_else(start)
    }
}

impl canvas::Program<Touch> for Court {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Touch>> {
        match (event, cursor.position_in(bounds)) {
            (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(at)) => self
                .touched(bounds.size(), at)
                .map(|t| Action::publish(t).and_capture()),
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        _state: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor
            .position_in(bounds)
            .and_then(|at| self.touched(bounds.size(), at))
        {
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::default(),
        }
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = self.colors;
        let (court, meter) = Self::square(bounds.size(), self.below());
        frame.fill_rectangle(court.position(), court.size(), c.court);
        let lines = Stroke::default().with_color(c.court_lines).with_width(2.0);
        frame.stroke(&Path::rectangle(court.position(), court.size()), lines);
        match &self.content {
            Content::Players { .. } => self.draw_players(&mut frame, court, meter),
            Content::Target {
                ours,
                cursor,
                start_cursor,
                ..
            } => {
                self.draw_target(&mut frame, court, meter, *ours, cursor.as_ref());
                self.draw_starts(&mut frame, court, meter, *start_cursor);
            }
        }
        vec![frame.into_geometry()]
    }
}

impl Court {
    fn draw_players(&self, frame: &mut Frame, court: Rectangle, meter: f32) {
        let c = self.colors;
        let lines = Stroke::default().with_color(c.court_lines).with_width(2.0);
        // the net on top, thick; the attack line 3 meters from it
        frame.stroke(
            &Path::line(court.position(), Point::new(court.x + court.width, court.y)),
            lines.with_width(5.0),
        );
        let attack = court.y + 3.0 * meter;
        frame.stroke(
            &Path::line(
                Point::new(court.x, attack),
                Point::new(court.x + court.width, attack),
            ),
            lines,
        );
        for (center, radius, p) in self.spots(court, meter) {
            let fill = match (p.libero, p.spotlight) {
                (true, _) => theme::mix(c.button, c.accent, 0.35),
                (false, _) => c.button,
            };
            frame.fill(&Path::circle(center, radius), fill);
            let pickable = matches!(self.content, Content::Players { pickable: true, .. });
            let serving = match &self.content {
                Content::Players {
                    server: Some((id, color)),
                    ..
                } if *id == p.id => Some(*color),
                _ => None,
            };
            let ring = match (serving, pickable && p.selectable, p.grade) {
                (Some(color), _, _) => Some((color, 4.0)),
                (None, true, _) => Some((c.accent, 3.0)),
                (None, false, Some(g)) => Some((grade_color(g, c), 2.0)),
                (None, false, None) => None,
            };
            // the server with the ball, beside them toward the sideline
            if let Some(color) = serving {
                let side = radius * 0.9;
                frame.draw_svg(
                    Rectangle::new(
                        Point::new(center.x + radius * 0.75, center.y - radius * 1.15),
                        Size::new(side, side),
                    ),
                    iced::advanced::svg::Svg::new(iced::advanced::svg::Handle::from_memory(
                        theme::BALL,
                    ))
                    .color(color),
                );
            }
            if let Some((color, width)) = ring {
                frame.stroke(
                    &Path::circle(center, radius),
                    Stroke::default().with_color(color).with_width(width),
                );
            }
            frame.fill_text(Text {
                font: crate::theme::fonts().bold,
                content: p.number.to_string(),
                position: center,
                color: c.text,
                size: (radius * 0.9).into(),
                align_x: iced::widget::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
            // the name, abbreviated if long, and the efficiency below it:
            // three players side by side on 9 meters
            let size = (meter * 0.3).clamp(11.0, 15.0);
            // the letters that fit in 3 meters, a monospaced font
            let room = (3.0 * meter / (size * 0.62)) as usize;
            let short = scout4all_core::names::fitted(&p.name, room);
            let name = match p.setter {
                true => format!("▲ {short}"),
                false => short,
            };
            frame.fill_text(Text {
                font: crate::theme::fonts().regular,
                content: name,
                position: Point::new(center.x, center.y + radius + 3.0),
                color: c.text,
                size: size.into(),
                align_x: iced::widget::text::Alignment::Center,
                ..Text::default()
            });
            if let Some(e) = p.efficiency {
                frame.fill_text(Text {
                    font: crate::theme::fonts().bold,
                    content: format!("{e:.0}%"),
                    position: Point::new(center.x, center.y + radius + 5.0 + size),
                    color: p.grade.map_or(c.muted, |g| grade_color(g, c)),
                    size: size.into(),
                    align_x: iced::widget::text::Alignment::Center,
                    ..Text::default()
                });
            }
        }
    }

    fn draw_target(
        &self,
        frame: &mut Frame,
        court: Rectangle,
        meter: f32,
        ours: bool,
        cursor: Option<&(u8, String)>,
    ) {
        let c = self.colors;
        let lines = Stroke::default().with_color(c.court_lines).with_width(2.0);
        // the net: on top on our court, at the bottom on theirs
        let net_y = match ours {
            true => court.y,
            false => court.y + court.height,
        };
        frame.stroke(
            &Path::line(
                Point::new(court.x, net_y),
                Point::new(court.x + court.width, net_y),
            ),
            lines.with_width(5.0),
        );
        let thin = Stroke::default()
            .with_color(Color {
                a: 0.35,
                ..c.court_lines
            })
            .with_width(1.0);
        for (r, cell) in self.cells(court) {
            let at_cursor = cursor.is_some_and(|(z, s)| *z == cell.zone && *s == cell.subzone);
            if cell.chosen {
                frame.fill_rectangle(
                    r.position(),
                    r.size(),
                    Color {
                        a: 0.55,
                        ..c.accent
                    },
                );
            }
            frame.stroke(&Path::rectangle(r.position(), r.size()), thin);
            if at_cursor {
                frame.stroke(
                    &Path::rectangle(
                        Point::new(r.x + 2.0, r.y + 2.0),
                        Size::new(r.width - 4.0, r.height - 4.0),
                    ),
                    Stroke::default().with_color(c.accent).with_width(3.0),
                );
            }
            frame.fill_text(Text {
                font: crate::theme::fonts().regular,
                content: cell.label.clone(),
                position: r.center(),
                color: match cell.chosen {
                    true => c.text,
                    false => c.muted,
                },
                size: (meter * 0.35).max(11.0).into(),
                align_x: iced::widget::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
        }
    }

    /// The zones our serve starts from, below the net: the end line of
    /// ours, as our server sees it.
    fn draw_starts(
        &self,
        frame: &mut Frame,
        court: Rectangle,
        meter: f32,
        cursor: Option<ServeStartEnum>,
    ) {
        let c = self.colors;
        let starts = self.starts(court, meter);
        if let (Some((first, _)), Some((last, _))) = (starts.first(), starts.last()) {
            frame.fill_rectangle(
                first.position(),
                Size::new(last.x + last.width - first.x, first.height),
                c.court,
            );
        }
        let thin = Stroke::default()
            .with_color(Color {
                a: 0.35,
                ..c.court_lines
            })
            .with_width(1.0);
        for (r, start) in starts {
            if start.chosen {
                frame.fill_rectangle(
                    r.position(),
                    r.size(),
                    Color {
                        a: 0.55,
                        ..c.accent
                    },
                );
            }
            frame.stroke(&Path::rectangle(r.position(), r.size()), thin);
            if cursor == Some(start.value) {
                frame.stroke(
                    &Path::rectangle(
                        Point::new(r.x + 2.0, r.y + 2.0),
                        Size::new(r.width - 4.0, r.height - 4.0),
                    ),
                    Stroke::default().with_color(c.accent).with_width(3.0),
                );
            }
            frame.fill_text(Text {
                font: crate::theme::fonts().bold,
                content: start.label.clone(),
                position: r.center(),
                color: match start.chosen {
                    true => c.text,
                    false => c.muted,
                },
                size: (meter * 0.45).max(12.0).into(),
                align_x: iced::widget::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
        }
    }
}
