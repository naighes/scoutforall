// The match report: lays out the data computed by `model.rs`, passed as
// `sys.inputs.report`. All the styling lives here.
#let report = sys.inputs.report
#let labels = report.labels

// palette: neutral grays, one accent for our team
#let ink = rgb("#1f2328")
#let muted = rgb("#6b7280")
#let rule = rgb("#d0d7de")
#let zebra = rgb("#f4f6f8")
#let accent = rgb("#2b5f9e")
#let accent-light = rgb("#dce7f4")
#let them-light = rgb("#e6e8eb")

#let thin = 0.4pt + rule
#let strong-rule = 0.7pt + ink

#set page(
  paper: "a4",
  margin: (x: 0.8cm, top: 0.9cm, bottom: 1.3cm),
  footer: context {
    set text(6.5pt, fill: muted)
    line(length: 100%, stroke: thin)
    v(-3pt)
    grid(
      columns: (1fr, auto),
      [#labels.created_with #strong[scout4all] · naighes.github.io/scoutforall],
      counter(page).display("1 / 1", both: true),
    )
  },
)
#set text(font: "Source Sans 3", size: 7.5pt, fill: ink, number-width: "tabular")
#set table(stroke: none)
// a single weight for the emphasis: the font embeds regular and semibold
#show strong: set text(weight: "semibold")

#let section(title) = block(
  above: 11pt,
  below: 5pt,
  text(8pt, weight: "semibold", tracking: 0.4pt, upper(title)),
)

#let caption(title) = block(below: 3pt, text(weight: "semibold", title))

// the match: the teams, the sets won, the date
#let header(h) = {
  let (first, second) = h.teams
  let sets(t) = text(28pt, weight: "semibold", fill: if t.us { accent } else { ink }, str(t.sets))
  grid(
    columns: (1fr, auto, 1fr),
    column-gutter: 16pt,
    align: (right + horizon, center + horizon, left + horizon),
    text(15pt, weight: "semibold", first.name),
    stack(dir: ltr, spacing: 10pt, sets(first), text(22pt, fill: muted, "–"), sets(second)),
    text(15pt, weight: "semibold", second.name),
  )
  v(-6pt)
  align(center, text(8pt, fill: muted, h.date))
  v(2pt)
  line(length: 100%, stroke: strong-rule)
}

#let kpis(items) = grid(
  columns: (1fr,) * items.len(),
  column-gutter: 6pt,
  ..items.map(k => block(
    width: 100%,
    fill: zebra,
    radius: 3pt,
    inset: (x: 7pt, y: 6pt),
    stack(
      spacing: 4pt,
      text(6.5pt, fill: muted, tracking: 0.3pt, upper(k.label)),
      text(15pt, weight: "semibold", k.value),
      text(6.5pt, fill: muted, k.detail),
    ),
  )),
)

// a table with a header row and the labels of the rows in the first
// column; the last row is the total when `t.total` is set
#let simple-table(t, columns: auto) = {
  let n = t.columns.len()
  let cells(row, total) = row.map(c => if total { strong(c) } else { c })
  let rows = t.rows.enumerate().map(((i, r)) => {
    let total = t.total and i == t.rows.len() - 1
    if total { (table.hline(stroke: thin), ..cells(r, true)) } else { cells(r, false) }
  })
  block(breakable: false, {
    if t.title != "" { caption(t.title) }
    table(
      columns: if columns == auto { (auto,) + (1fr,) * (n - 1) } else { columns },
      inset: (x: 4pt, y: 2.6pt),
      align: (x, y) => (if x == 0 { left } else { center }) + horizon,
      fill: (x, y) => if y > 0 and calc.even(y) { zebra },
      table.header(..t.columns.map(c => text(fill: muted, c)), table.hline(stroke: strong-rule)),
      ..rows.flatten(),
      table.hline(stroke: thin),
    )
  })
}

// a player in a set: the starting position (filled for the setter), or
// entering from the bench
// placed, so that a row with the symbols is as tall as the others
#let symbol(body) = box(width: 8pt, height: 0.6em, place(center + horizon, body))
#let position(n, setter) = symbol(circle(
  radius: 3.6pt,
  inset: 0pt,
  stroke: 0.6pt + ink,
  fill: if setter { ink } else { none },
  align(center + horizon, text(5pt, weight: "semibold", fill: if setter { white } else { ink }, str(n))),
))
#let bench = symbol(circle(radius: 2.4pt, stroke: 0.6pt + muted))

#let lead-cell(c) = if c.kind == "text" { c.text } else if c.kind == "start" {
  position(c.position, c.setter)
} else if c.kind == "bench" { bench } else { [] }

// a table of statistics: the leading groups, then the ones of the
// statistics, a thin line closing each group
#let stats-table(t) = {
  // a group may have no columns, e.g. the sets of a match not started
  let groups = (t.lead + report.stats_groups).filter(g => g.columns.len() > 0)
  let columns = groups.map(g => g.columns.map(_ => if g.fill { 1fr } else { auto })).flatten()
  let n = columns.len()
  let ends = groups.fold((), (acc, g) => acc + ((acc.at(-1, default: -1) + g.columns.len()),))
  let left-columns = groups
    .enumerate()
    .filter(((i, g)) => g.fill)
    .map(((i, g)) => if i == 0 { 0 } else { ends.at(i - 1) + 1 })
  let rows = t.rows.map(r => {
    let cells = r.lead.map(lead-cell) + r.values
    if r.total { (table.hline(stroke: strong-rule), ..cells.map(c => strong(c))) } else { cells }
  })
  block(breakable: false, {
    section(t.title)
    table(
      columns: columns,
      inset: (x: 2.4pt, y: 2.5pt),
      align: (x, y) => (if x == 0 or left-columns.contains(x) { left } else { center }) + horizon,
      stroke: (x, y) => (right: if ends.contains(x) and x < n - 1 { thin }),
      fill: (x, y) => if y > 1 and calc.odd(y) { zebra },
      table.header(
        ..groups.map(g => table.cell(colspan: g.columns.len(), text(weight: "semibold", g.label))),
        ..groups.map(g => g.columns).flatten().map(c => text(6.5pt, fill: muted, c)),
        table.hline(stroke: strong-rule),
      ),
      ..rows.flatten(),
      table.hline(stroke: strong-rule),
    )
  })
}

// the front row (4, 3, 2) under the net, and the back row (8, 9)
#let court(c) = {
  let zone(n) = c.zones.find(z => z.zone == n)
  let value(z) = stack(spacing: 3pt, text(10pt, weight: "semibold", z.top), text(6.5pt, fill: muted, z.bottom))
  block(breakable: false, {
    caption(c.title)
    table(
      columns: (1fr, 1fr, 1fr),
      rows: (32pt, 32pt),
      align: center + horizon,
      fill: zebra,
      stroke: (x, y) => (
        top: if y == 0 { 1.6pt + ink } else { (paint: muted, thickness: 0.5pt, dash: "dashed") },
        bottom: if y == 1 { thin },
        left: if x == 0 { thin },
        right: if x == 2 { thin },
      ),
      value(zone(4)), value(zone(3)), value(zone(2)),
      [], value(zone(8)), value(zone(9)),
    )
  })
}

// the points of every set in order, a column per point wrapping after
// `per-row`: a row per team, with the score reached by the team scoring
#let point-sequence(s, per-row: 40) = block(breakable: false, {
  section(s.title)
  let point(p, team) = if p == none {
    table.cell(stroke: none, [])
  } else if p.team == team {
    table.cell(fill: if p.us { accent-light } else { them-light }, text(weight: "semibold", str(p.score)))
  } else { [] }
  table(
    columns: (auto,) + (1fr,) * per-row,
    inset: (x: 1pt, y: 1.8pt),
    align: (x, y) => (if x == 0 { left } else { center }) + horizon,
    stroke: thin,
    ..s
      .sets
      .map(points => (
        table.cell(colspan: per-row + 1, stroke: none, inset: (top: 5pt, bottom: 2pt), text(weight: "semibold", points.label)),
        ..points
          .points
          .chunks(per-row)
          .map(chunk => range(2).map(team => (
            table.cell(inset: (x: 3pt), text(6.5pt, s.teams.at(team))),
            ..range(per-row).map(i => point(chunk.at(i, default: none), team)),
          )))
          .flatten(),
      ))
      .flatten(),
  )
})

#let legend = {
  set text(6.5pt, fill: muted)
  block(above: 8pt, {
    stack(
      dir: ltr,
      spacing: 4pt,
      position(1, false),
      labels.starting_position,
      h(6pt),
      position(1, true),
      labels.setter,
      h(6pt),
      bench,
      labels.from_the_bench,
    )
    v(2pt)
    labels.legend
  })
}

// the summary of the match
#header(report.header)
#v(4pt)
#kpis(report.kpis)
#section("set")
#simple-table(report.sets, columns: (auto, auto, 1fr, 1fr, auto, auto))
#stats-table(report.players)
#legend

// then the analysis of the team: every block stays in one piece, on the
// next page when it doesn't fit
#stats-table(report.rotations)
#stats-table(report.set_stats)
#v(8pt)
#grid(columns: (1fr, 1fr, 1fr), column-gutter: 12pt, ..report.courts.map(court))
#v(8pt)
#grid(
  columns: (1fr, 1fr),
  column-gutter: 12pt,
  row-gutter: 10pt,
  simple-table(report.first_rallies), simple-table(report.freeballs),
  stack(spacing: 10pt, simple-table(report.attack_by_phase), simple-table(report.side_out_possessions)),
  grid(
    columns: (1fr, 1fr),
    column-gutter: 8pt,
    simple-table(report.points_won), simple-table(report.points_lost),
  ),
)
#point-sequence(report.point_sequence)
