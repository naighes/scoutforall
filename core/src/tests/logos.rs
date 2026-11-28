//! Logos for the tests of the PDFs, and how to tell they are in one.
use crate::assets;
use crate::reporting::logo::Logo;
use std::io::Cursor;

/// An 8×8 logo of one color on a transparent background.
pub(crate) fn logo(color: [u8; 4]) -> Logo {
    let mut png = Cursor::new(Vec::new());
    // transparent around, as a logo is: its color alone tells its tone
    image::RgbaImage::from_fn(8, 8, |x, _| match x {
        0 | 1 => image::Rgba([0, 0, 0, 0]),
        _ => image::Rgba(color),
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap_or_default();
    assets::normalize_logo(png.get_ref())
        .expect("expected a logo")
        .into()
}

/// The images a PDF draws from.
pub(crate) fn image_count(pdf: &[u8]) -> usize {
    pdf.windows(b"/Subtype/Image".len())
        .filter(|w| w == b"/Subtype/Image")
        .count()
}
