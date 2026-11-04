//! The trend of a figure set by set, as the stats sheet of the app draws
//! it: a line per set from its first rally, the match so far dashed
//! across, the band of the level shaded. Drawn with the canvas of iced
//! (chosen over iced_aksel and plotters-iced, 027).

use {
    crate::theme::{Colors, FAIR, set_color},
    iced::{
        Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Frame, Geometry, LineDash, Path, Stroke, Text},
    },
    scout4all_mobile::TrendLine,
};

/// The points of a trend, ready to draw.
#[derive(Debug, Clone)]
pub struct Trend {
    /// each set: its number, its rallies (where the set had got to, the
    /// value), split where the value is unknown
    pub sets: Vec<(u8, Vec<Vec<(f64, f64)>>)>,
    pub overall: Option<f64>,
    pub band: Option<(f64, f64)>,
    /// the value axis, always with 0 and the band in sight
    pub low: f64,
    pub high: f64,
    /// a whole set, longer at deuce
    pub points: f64,
}

impl Trend {
    pub fn of(line: &TrendLine, recent: bool) -> Trend {
        let sets: Vec<(u8, Vec<Vec<(f64, f64)>>)> = line
            .sets
            .iter()
            .map(|set| {
                let values = match recent {
                    true => &set.recent,
                    false => &set.values,
                };
                let runs = values.iter().zip(set.progress.iter()).fold(
                    vec![vec![]],
                    |mut runs: Vec<Vec<(f64, f64)>>, (v, x)| {
                        match (v, runs.last_mut()) {
                            (Some(v), Some(run)) => run.push((*x, *v)),
                            (None, Some(run)) if !run.is_empty() => runs.push(vec![]),
                            _ => {}
                        }
                        runs
                    },
                );
                (set.set_number, runs)
            })
            .collect();
        let overall = match recent {
            true => line.overall_recent,
            false => line.overall,
        };
        let band = line.band.as_ref().map(|b| (b.low, b.high));
        let known: Vec<f64> = sets
            .iter()
            .flat_map(|(_, runs)| runs.iter().flatten().map(|(_, v)| *v))
            .chain(overall)
            .chain(band.into_iter().flat_map(|(l, h)| [l, h]))
            .collect();
        let low = known.iter().copied().fold(0.0_f64, f64::min);
        let high = known.iter().copied().fold(low + 10.0, f64::max);
        let points = sets
            .iter()
            .flat_map(|(_, runs)| runs.iter().flatten().map(|(x, _)| *x))
            .fold(25.0_f64, f64::max);
        Trend {
            sets,
            overall,
            band,
            low,
            high,
            points,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sets
            .iter()
            .all(|(_, runs)| runs.iter().all(Vec::is_empty))
    }
}

pub struct CanvasTrend {
    pub trend: Trend,
    pub colors: Colors,
}

impl<M> canvas::Program<M> for CanvasTrend {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let t = &self.trend;
        let c = self.colors;
        // room for the labels of the points under the chart
        let label = 14.0;
        let plot = Size::new(bounds.width, (bounds.height - label).max(1.0));
        let x = |p: f64| (f64::from(plot.width) * p / t.points) as f32;
        let y = |v: f64| (f64::from(plot.height) * (1.0 - (v - t.low) / (t.high - t.low))) as f32;
        if let Some((low, high)) = t.band {
            frame.fill_rectangle(
                Point::new(0.0, y(high)),
                Size::new(plot.width, y(low) - y(high)),
                Color { a: 0.12, ..FAIR },
            );
        }
        let axis = Stroke::default().with_color(c.muted).with_width(1.0);
        frame.stroke(
            &Path::line(Point::new(0.0, y(0.0)), Point::new(plot.width, y(0.0))),
            axis,
        );
        // every 5 points, with its number below
        let grid = Stroke::default()
            .with_color(Color { a: 0.3, ..c.muted })
            .with_width(1.0);
        for p in (0..=t.points as u32).step_by(5) {
            let px = x(f64::from(p));
            if p > 0 {
                frame.stroke(
                    &Path::line(Point::new(px, 0.0), Point::new(px, plot.height)),
                    grid,
                );
            }
            frame.fill_text(Text {
                font: crate::theme::fonts().regular,
                content: p.to_string(),
                position: Point::new(px, plot.height + 1.0),
                color: c.muted,
                size: 11.0.into(),
                // the first and the last inside the chart
                align_x: match (p, f64::from(p + 5) > t.points) {
                    (0, _) => iced::widget::text::Alignment::Left,
                    (_, true) => iced::widget::text::Alignment::Right,
                    (_, false) => iced::widget::text::Alignment::Center,
                },
                ..Text::default()
            });
        }
        if let Some(overall) = t.overall {
            frame.stroke(
                &Path::line(
                    Point::new(0.0, y(overall)),
                    Point::new(plot.width, y(overall)),
                ),
                Stroke {
                    line_dash: LineDash {
                        segments: &[6.0, 6.0],
                        offset: 0,
                    },
                    ..Stroke::default()
                        .with_color(Color { a: 0.6, ..c.text })
                        .with_width(1.5)
                },
            );
        }
        for (set, runs) in &t.sets {
            let color = set_color(*set);
            for run in runs {
                let path = Path::new(|b| {
                    for (i, (px, v)) in run.iter().enumerate() {
                        match i {
                            0 => b.move_to(Point::new(x(*px), y(*v))),
                            _ => b.line_to(Point::new(x(*px), y(*v))),
                        }
                    }
                });
                frame.stroke(&path, Stroke::default().with_color(color).with_width(2.0));
            }
            if let Some((px, v)) = runs.iter().flatten().last() {
                frame.fill(&Path::circle(Point::new(x(*px), y(*v)), 3.0), color);
            }
        }
        vec![frame.into_geometry()]
    }
}

pub fn canvas_view<'a, M: 'a>(trend: Trend, colors: Colors, height: Length) -> Element<'a, M> {
    iced::widget::canvas(CanvasTrend { trend, colors })
        .width(Length::Fill)
        .height(height)
        .into()
}
