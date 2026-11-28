// The sheet of a training session and the plan of a microcycle: lays out the
// data computed by `training.rs`, passed as `sys.inputs.sheet`.
#let sheet = sys.inputs.sheet
#let labels = sheet.labels

#let ink = rgb("#1f2328")
#let muted = rgb("#6b7280")
#let rule = rgb("#d0d7de")
#let zebra = rgb("#f4f6f8")
#let accent = rgb("#2b5f9e")

#let thin = 0.4pt + rule
#let strong-rule = 0.7pt + ink

#set page(
  paper: "a4",
  margin: (x: 1.6cm, top: 1.5cm, bottom: 1.8cm),
  footer: context {
    set text(7pt, fill: muted)
    line(length: 100%, stroke: thin)
    v(-3pt)
    grid(
      columns: (1fr, auto, auto),
      column-gutter: 12pt,
      [#labels.created_with #strong[scout4all] · naighes.github.io/scoutforall],
      sheet.printed,
      counter(page).display("1 / 1", both: true),
    )
  },
)
#set text(font: "Source Sans 3", size: 9.5pt, fill: ink)
#set par(leading: 0.55em)
#show strong: set text(weight: "semibold")

#let section(title) = block(
  above: 14pt,
  below: 6pt,
  text(9pt, weight: "semibold", tracking: 0.4pt, upper(title)),
)

// the text of the user keeps its lines
#let lines(body) = body.split("\n").join(linebreak())

#let heading-lines = {
  text(8.5pt, fill: muted, tracking: 0.3pt, upper(sheet.kind))
  v(-2pt)
  text(20pt, weight: "semibold", if sheet.title != "" { sheet.title } else { sheet.team })
  v(-4pt)
  text(11pt, fill: accent, sheet.subtitle)
  v(-2pt)
  text(9pt, fill: muted, [#sheet.team · #labels.season #sheet.season])
}

#let header() = {
  if sheet.logo == none { heading-lines } else {
    grid(
      columns: (auto, 1fr),
      column-gutter: 12pt,
      align: horizon,
      team-logo(sheet.logo, 48pt),
      heading-lines,
    )
  }
  v(2pt)
  line(length: 100%, stroke: strong-rule)
}

#let objectives(text-of) = if text-of != "" {
  section(labels.objectives)
  lines(text-of)
}

// a cancelled session is struck through
#let strike-if(cancelled, body) = if cancelled { strike(text(fill: muted, body)) } else { body }

#let activities(s) = if s.activities.len() > 0 {
  section(labels.activities)
  table(
    columns: (auto, 1fr, auto),
    inset: (x: 5pt, y: 4pt),
    stroke: (x, y) => (bottom: thin),
    align: (x, y) => (if x == 2 { right } else { left }) + top,
    ..s.activities.enumerate().map(((i, a)) => (
      text(fill: muted, str(i + 1) + "."),
      {
        strong(a.name)
        if a.description != "" { linebreak(); text(fill: muted, lines(a.description)) }
      },
      a.minutes,
    )).flatten(),
  )
}

#let participants(s) = if s.participants != none and s.participants.len() > 0 {
  section(labels.participants)
  table(
    columns: (auto, 1fr, auto),
    inset: (x: 5pt, y: 3pt),
    stroke: (x, y) => (bottom: thin),
    fill: (x, y) => if calc.odd(y) { zebra },
    ..s.participants.map(p => (
      text(fill: muted, p.number),
      if p.guest { [#p.name #text(fill: muted)[(#labels.guest)]] } else { p.name },
      p.status,
    )).flatten(),
  )
}

#let notes(s) = if s.notes != none {
  section(labels.notes)
  lines(s.notes)
}

// the summary of the sessions of a microcycle: a row each
#let overview() = {
  section(labels.sessions)
  table(
    columns: (auto, auto, auto, 1fr),
    inset: (x: 5pt, y: 4pt),
    stroke: (x, y) => (bottom: thin),
    table.header(
      ..(labels.day, labels.time, labels.location, labels.objectives).map(c => text(fill: muted, c)),
      table.hline(stroke: strong-rule),
    ),
    ..sheet.sessions.map(s => {
      let cell(body) = strike-if(s.cancelled, body)
      (
        cell(s.day),
        cell(s.time),
        cell(s.location),
        {
          cell(s.objectives)
          if s.cancelled { text(fill: muted, [ (#labels.cancelled)]) }
        },
      )
    }).flatten(),
  )
}

#header()

#if sheet.table {
  objectives(sheet.objectives)
  overview()
  for s in sheet.sessions.filter(s => s.heading != none) {
    block(breakable: true, {
      section(strike-if(s.cancelled, s.heading))
      if s.cancelled { text(fill: muted, labels.cancelled) }
      activities(s)
    })
  }
} else {
  objectives(sheet.objectives)
  for s in sheet.sessions {
    activities(s)
    participants(s)
    notes(s)
  }
}
