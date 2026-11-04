//! The court of the scouting, drawn with the canvas of iced: our half seen
//! from the bench (the net at the top) with the players where they stand,
//! or the court where the ball went while evaluating, with the cursor of
//! the arrows.

use {
    crate::theme::{Colors, grade_color},
    iced::{
        Color, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke, Text},
    },
    scout4all_mobile::{CourtCellChoice, CourtPlayer},
};

/// What the court shows.
pub enum Content {
    Players(Vec<CourtPlayer>),
    /// rows of cells (one row at the net for our block), the net at the
    /// top when the court is ours; the cell of the cursor
    Target {
        rows: Vec<Vec<CourtCellChoice>>,
        ours: bool,
        cursor: Option<(u8, String)>,
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
}

/// Half a court is 9 by 9 meters; a margin around it for the players
/// standing outside to serve or receive.
const METERS: f32 = 9.0;
const MARGIN: f32 = 0.6;

impl Court {
    /// The square of the court in the bounds, and the size of a meter.
    fn square(bounds: Size) -> (Rectangle, f32) {
        let side = bounds.width.min(bounds.height);
        let meter = side / (METERS + 2.0 * MARGIN);
        let origin = Point::new(
            (bounds.width - side) / 2.0 + MARGIN * meter,
            (bounds.height - side) / 2.0 + MARGIN * meter,
        );
        (
            Rectangle::new(origin, Size::new(METERS * meter, METERS * meter)),
            meter,
        )
    }

    /// Each cell with where it's drawn.
    fn cells(&self, court: Rectangle) -> Vec<(Rectangle, &CourtCellChoice)> {
        match &self.content {
            Content::Players(_) => vec![],
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
            Content::Players(players) => players
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
        let (court, meter) = Self::square(bounds);
        let player = self
            .spots(court, meter)
            .into_iter()
            .filter(|(_, _, p)| p.selectable)
            .find(|(center, radius, _)| center.distance(at) <= *radius * 1.2)
            .map(|(_, _, p)| Touch::Player(p.id.clone()));
        let cell = || {
            self.cells(court)
                .into_iter()
                .find(|(r, _)| r.contains(at))
                .map(|(_, c)| Touch::Cell(c.zone, c.subzone.clone()))
        };
        player.or_else(cell)
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
        let (court, meter) = Self::square(bounds.size());
        frame.fill_rectangle(court.position(), court.size(), c.court);
        let lines = Stroke::default().with_color(c.court_lines).with_width(2.0);
        frame.stroke(&Path::rectangle(court.position(), court.size()), lines);
        match &self.content {
            Content::Players(_) => self.draw_players(&mut frame, court, meter),
            Content::Target { ours, cursor, .. } => {
                self.draw_target(&mut frame, court, meter, *ours, cursor.as_ref())
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
                (true, _) => Color { a: 0.9, ..c.muted },
                (false, _) => c.raised,
            };
            frame.fill(&Path::circle(center, radius), fill);
            let ring = match (p.selectable, p.grade) {
                (true, _) => Some((c.accent, 3.0)),
                (false, Some(g)) => Some((grade_color(g), 2.0)),
                (false, None) => None,
            };
            if let Some((color, width)) = ring {
                frame.stroke(
                    &Path::circle(center, radius),
                    Stroke::default().with_color(color).with_width(width),
                );
            }
            frame.fill_text(Text {
                content: p.number.to_string(),
                position: center,
                color: c.text,
                size: (radius * 0.9).into(),
                align_x: iced::widget::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
            let below = Point::new(center.x, center.y + radius + 2.0);
            let name = match (p.setter, p.efficiency) {
                (true, _) => format!("▲ {}", p.name),
                (false, Some(e)) => format!("{} {:.0}%", p.name, e),
                (false, None) => p.name.clone(),
            };
            frame.fill_text(Text {
                content: name,
                position: below,
                color: p.grade.map_or(c.muted, grade_color),
                size: (meter * 0.32).max(11.0).into(),
                align_x: iced::widget::text::Alignment::Center,
                ..Text::default()
            });
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
}
