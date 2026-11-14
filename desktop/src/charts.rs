//! The trend of a figure set by set, as the stats sheet of the app draws
//! it: a line per set from its first rally, a rally a column (the same
//! columns as the points of the sets under it), the match so far dashed
//! across, the band of the level shaded. Drawn with the canvas of iced
//! (chosen over iced_aksel and plotters-iced, 027).

use {
    crate::theme::{Colors, FAIR, LOST, set_color},
    iced::{
        Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Frame, Geometry, LineDash, Path, Stroke, Text},
    },
    scout4all_mobile::{MatchTrend, TrendLine},
};

/// How high the numbers of the rallies under the chart of a match are.
pub const RALLY_LABELS: f32 = 14.0;

/// The points of a trend, ready to draw.
#[derive(Debug, Clone)]
pub struct Trend {
    /// each set: its number, its rallies (the rally from 1, the value),
    /// split where the value is unknown
    pub sets: Vec<(u8, Vec<Vec<(f64, f64)>>)>,
    pub overall: Option<f64>,
    pub band: Option<(f64, f64)>,
    /// the value axis, always with 0 and the band in sight
    pub low: f64,
    pub high: f64,
    /// the rallies of the longest set, at least 25: a column each
    pub points: f64,
    /// the direct errors of the figure along the sets, a red line across
    /// the chart: the rally and how many, of every set
    pub errors: Vec<(f64, u32)>,
}

/// The rallies of a set from the first: its columns.
fn rallies() -> impl Iterator<Item = f64> {
    (1u32..).map(f64::from)
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
                let runs = values.iter().zip(rallies()).fold(
                    vec![vec![]],
                    |mut runs: Vec<Vec<(f64, f64)>>, (v, x)| {
                        match (v, runs.last_mut()) {
                            (Some(v), Some(run)) => run.push((x, *v)),
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
            errors: vec![],
        }
    }

    /// With the direct (unforced) errors of the figure in each rally, of
    /// every set.
    pub fn with_errors(self, line: &TrendLine) -> Trend {
        let errors = line
            .sets
            .iter()
            .flat_map(|set| {
                rallies()
                    .zip(set.unforced.iter())
                    .filter(|(_, n)| **n > 0)
                    .map(|(x, n)| (x, *n))
                    .collect::<Vec<_>>()
            })
            .collect();
        Trend { errors, ..self }
    }

    /// The errors in each rally of the sets, from the first, of every set
    /// together: how dense they are.
    pub fn error_bins(&self) -> Vec<u32> {
        let bins = (self.points.ceil() as usize).max(1);
        self.errors
            .iter()
            .fold(vec![0; bins], |mut counts, (x, n)| {
                let at = (*x as usize).saturating_sub(1).min(bins - 1);
                if let Some(bin) = counts.get_mut(at) {
                    *bin += n;
                }
                counts
            })
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
        let plot = Size::new(bounds.width, (bounds.height - RALLY_LABELS).max(1.0));
        // a rally in the middle of its column
        let x = |p: f64| (f64::from(plot.width) * (p - 0.5) / t.points) as f32;
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
        // the first rally and every 5, with its number below, over its
        // column
        let grid = Stroke::default()
            .with_color(Color { a: 0.3, ..c.muted })
            .with_width(1.0);
        for p in std::iter::once(1).chain((5..=t.points as u32).step_by(5)) {
            let px = x(f64::from(p));
            frame.stroke(
                &Path::line(Point::new(px, 0.0), Point::new(px, plot.height)),
                grid,
            );
            frame.fill_text(Text {
                font: crate::theme::fonts().regular,
                content: p.to_string(),
                position: Point::new(px, plot.height + 1.0),
                color: c.muted,
                size: 11.0.into(),
                align_x: iced::widget::text::Alignment::Center,
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
        // the direct errors, a red line across the chart in the column of
        // their rally (from 1), under the lines of the sets
        let error = Stroke::default()
            .with_color(Color { a: 0.8, ..LOST })
            .with_width(2.0);
        for (i, _) in t.error_bins().iter().enumerate().filter(|(_, n)| **n > 0) {
            let px = x(i as f64 + 1.0);
            frame.stroke(
                &Path::line(Point::new(px, 0.0), Point::new(px, plot.height)),
                error,
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

pub fn canvas_view<'a, M: 'a>(
    trend: Trend,
    colors: Colors,
    width: Length,
    height: Length,
) -> Element<'a, M> {
    iced::widget::canvas(CanvasTrend { trend, colors })
        .width(width)
        .height(height)
        .into()
}

/// A figure match by match, ready to draw.
#[derive(Debug, Clone, Default)]
pub struct MatchesTrend {
    /// each match: the day as shown, the opponent, the value, its touches
    pub matches: Vec<(String, String, Option<f64>, u32)>,
    /// with the matches before
    pub moving: Vec<Option<f64>>,
    pub overall: Option<f64>,
    pub band: Option<(f64, f64)>,
    pub low: f64,
    pub high: f64,
    /// what the hover says of the touches, `{1}` the count
    pub touches: String,
}

impl MatchesTrend {
    pub fn of(line: &MatchTrend, days: Vec<String>, opponents: Vec<String>, touches: &str) -> Self {
        let band = line.band.as_ref().map(|b| (b.low, b.high));
        let known: Vec<f64> = line
            .values
            .iter()
            .chain(line.moving.iter())
            .flatten()
            .copied()
            .chain(line.overall)
            .chain(band.into_iter().flat_map(|(l, h)| [l, h]))
            .collect();
        let low = known.iter().copied().fold(0.0_f64, f64::min);
        let high = known.iter().copied().fold(low + 10.0, f64::max);
        MatchesTrend {
            matches: days
                .into_iter()
                .zip(opponents)
                .zip(line.values.iter().zip(line.touches.iter()))
                .map(|((day, opponent), (value, touches))| (day, opponent, *value, *touches))
                .collect(),
            moving: line.moving.clone(),
            overall: line.overall,
            band,
            low,
            high,
            touches: touches.to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.matches.iter().all(|(_, _, v, _)| v.is_none())
    }
}

/// The value axis on the left, the matches under the chart.
const AXIS: f32 = 40.0;
const LABELS: f32 = 30.0;

pub struct CanvasMatches {
    pub trend: MatchesTrend,
    pub colors: Colors,
}

impl CanvasMatches {
    /// Where the match `i` is drawn across, in a plot `width` wide.
    fn x(&self, i: usize, width: f32) -> f32 {
        let n = self.trend.matches.len().max(1) as f32;
        AXIS + (width - AXIS) * (i as f32 + 0.5) / n
    }

    /// The match under the cursor.
    fn hovered(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<usize> {
        let at = cursor.position_in(bounds)?;
        let n = self.trend.matches.len();
        (n > 0 && at.x >= AXIS).then(|| {
            let slot = (bounds.width - AXIS) / n as f32;
            (((at.x - AXIS) / slot) as usize).min(n - 1)
        })
    }
}

impl<M> canvas::Program<M> for CanvasMatches {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &canvas::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        matches!(
            event,
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
        )
        .then(canvas::Action::request_redraw)
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let t = &self.trend;
        let c = self.colors;
        let width = bounds.width;
        let height = (bounds.height - LABELS).max(1.0);
        // room above for the label of the top line and its points
        let top = 10.0_f32;
        let y = |v: f64| {
            top + (f64::from(height - top) * (1.0 - (v - t.low) / (t.high - t.low))) as f32
        };
        let small = |content: String, position: Point, color: Color, align_x| Text {
            font: crate::theme::fonts().regular,
            content,
            position,
            color,
            size: 11.0.into(),
            align_x,
            ..Text::default()
        };
        use iced::widget::text::Alignment as Align;
        if let Some((low, high)) = t.band {
            frame.fill_rectangle(
                Point::new(AXIS, y(high)),
                Size::new(width - AXIS, y(low) - y(high)),
                Color { a: 0.12, ..FAIR },
            );
        }
        // the value axis: a line every 20, with its value
        let grid = Stroke::default()
            .with_color(Color { a: 0.25, ..c.muted })
            .with_width(1.0);
        let first = (t.low / 20.0).ceil() as i32;
        let last = (t.high / 20.0).floor() as i32;
        for step in first..=last {
            let v = f64::from(step * 20);
            frame.stroke(
                &Path::line(Point::new(AXIS, y(v)), Point::new(width, y(v))),
                match step {
                    0 => Stroke::default().with_color(c.muted).with_width(1.0),
                    _ => grid,
                },
            );
            frame.fill_text(small(
                format!("{v:.0}%"),
                Point::new(AXIS - 6.0, y(v) - 6.0),
                c.muted,
                Align::Right,
            ));
        }
        if let Some(overall) = t.overall {
            frame.stroke(
                &Path::line(Point::new(AXIS, y(overall)), Point::new(width, y(overall))),
                Stroke {
                    line_dash: LineDash {
                        segments: &[6.0, 6.0],
                        offset: 0,
                    },
                    ..Stroke::default()
                        .with_color(Color { a: 0.5, ..c.text })
                        .with_width(1.0)
                },
            );
        }
        // the moving average: thick, under the values
        let runs = |values: Vec<Option<f64>>| {
            values.into_iter().enumerate().fold(
                vec![vec![]],
                |mut runs: Vec<Vec<Point>>, (i, v)| {
                    match (v, runs.last_mut()) {
                        (Some(v), Some(run)) => run.push(Point::new(self.x(i, width), y(v))),
                        (None, Some(run)) if !run.is_empty() => runs.push(vec![]),
                        _ => {}
                    }
                    runs
                },
            )
        };
        let line = |points: &[Point]| {
            Path::new(|b| {
                for (i, p) in points.iter().enumerate() {
                    match i {
                        0 => b.move_to(*p),
                        _ => b.line_to(*p),
                    }
                }
            })
        };
        for run in runs(t.moving.clone()) {
            frame.stroke(
                &line(&run),
                Stroke::default()
                    .with_color(Color {
                        a: 0.55,
                        ..c.accent
                    })
                    .with_width(5.0),
            );
        }
        let values: Vec<Option<f64>> = t.matches.iter().map(|(_, _, v, _)| *v).collect();
        for run in runs(values) {
            frame.stroke(
                &line(&run),
                Stroke::default().with_color(c.text).with_width(1.5),
            );
        }
        let hovered = self.hovered(bounds, cursor);
        // each match: its point, its day and opponent under the chart (as
        // many as fit)
        let slot = (width - AXIS) / t.matches.len().max(1) as f32;
        let every = (60.0 / slot).ceil().max(1.0) as usize;
        for (i, (day, opponent, value, _)) in t.matches.iter().enumerate() {
            let px = self.x(i, width);
            if let Some(v) = value {
                let radius = match hovered == Some(i) {
                    true => 5.0,
                    false => 3.5,
                };
                frame.fill(&Path::circle(Point::new(px, y(*v)), radius), c.text);
            }
            if i % every == 0 || hovered == Some(i) {
                frame.fill_text(small(
                    day.clone(),
                    Point::new(px, height + 2.0),
                    c.muted,
                    Align::Center,
                ));
                frame.fill_text(small(
                    opponent.chars().take(10).collect(),
                    Point::new(px, height + 15.0),
                    c.muted,
                    Align::Center,
                ));
            }
        }
        // what the match under the cursor gave
        if let Some((i, (day, opponent, value, touches))) =
            hovered.and_then(|i| t.matches.get(i).map(|m| (i, m)))
        {
            let px = self.x(i, width);
            frame.stroke(
                &Path::line(Point::new(px, 0.0), Point::new(px, height)),
                Stroke::default()
                    .with_color(Color { a: 0.4, ..c.accent })
                    .with_width(1.0),
            );
            let lines = [
                format!("{day} · {opponent}"),
                format!(
                    "{} · {}",
                    value.map_or("–".to_string(), |v| format!("{v:.0}%")),
                    t.touches.replace("{1}", &touches.to_string())
                ),
            ];
            let box_width = 230.0_f32;
            let left = match px + 12.0 + box_width > width {
                true => px - 12.0 - box_width,
                false => px + 12.0,
            };
            frame.fill_rectangle(
                Point::new(left, 6.0),
                Size::new(box_width, 40.0),
                Color { a: 0.92, ..c.panel },
            );
            for (n, l) in lines.into_iter().enumerate() {
                frame.fill_text(Text {
                    size: 12.0.into(),
                    ..small(
                        l,
                        Point::new(left + 8.0, 10.0 + 16.0 * n as f32),
                        c.text,
                        Align::Left,
                    )
                });
            }
        }
        vec![frame.into_geometry()]
    }
}

pub fn matches_view<'a, M: 'a>(trend: MatchesTrend, colors: Colors) -> Element<'a, M> {
    iced::widget::canvas(CanvasMatches { trend, colors })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
