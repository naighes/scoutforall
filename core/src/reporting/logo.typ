// The logo of a team (see `logo.rs`) in a square box, whatever its
// proportions; put before every template that shows one.
#let team-logo(logo, size) = box(
  width: size,
  height: size,
  inset: if logo.dark_backdrop { 2pt } else { 0pt },
  radius: 3pt,
  fill: if logo.dark_backdrop { rgb("#1f2328") },
  image(bytes(logo.data), width: 100%, height: 100%, fit: "contain"),
)

