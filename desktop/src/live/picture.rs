//! The pictures of a camera made what the encoder takes (I420, the size
//! of the quality chosen) and what the screen shows (RGBA, small).

/// A picture in I420: the luma, then the two chroma planes, a quarter each.
#[derive(Debug, Clone, PartialEq)]
pub struct I420 {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl I420 {
    fn planes(&self) -> (&[u8], &[u8], &[u8]) {
        let luma = self.width * self.height;
        let chroma = luma / 4;
        let (y, rest) = self.data.split_at(luma.min(self.data.len()));
        let (u, v) = rest.split_at(chroma.min(rest.len()));
        (y, u, v)
    }
}

fn even(n: usize) -> usize {
    n & !1
}

/// From YUYV (4:2:2, as the Mac's cameras give it): the chroma of the
/// even rows.
pub fn from_yuyv(src: &[u8], width: usize, height: usize) -> Option<I420> {
    let (w, h) = (even(width), even(height));
    if w == 0 || h == 0 || src.len() < width * height * 2 {
        return None;
    }
    let mut data = vec![0u8; w * h * 3 / 2];
    let (y, uv) = data.split_at_mut(w * h);
    let (u, v) = uv.split_at_mut(w * h / 4);
    for row in 0..h {
        let line = &src[row * width * 2..];
        for pair in 0..w / 2 {
            let at = pair * 4;
            y[row * w + pair * 2] = line[at];
            y[row * w + pair * 2 + 1] = line[at + 2];
            if row % 2 == 0 {
                let c = row / 2 * (w / 2) + pair;
                u[c] = line[at + 1];
                v[c] = line[at + 3];
            }
        }
    }
    Some(I420 {
        width: w,
        height: h,
        data,
    })
}

/// From NV12: the chroma as it is, split in two planes.
pub fn from_nv12(src: &[u8], width: usize, height: usize) -> Option<I420> {
    let (w, h) = (even(width), even(height));
    if w == 0 || h == 0 || src.len() < width * height * 3 / 2 {
        return None;
    }
    let mut data = vec![0u8; w * h * 3 / 2];
    let (y, uv) = data.split_at_mut(w * h);
    let (u, v) = uv.split_at_mut(w * h / 4);
    for row in 0..h {
        y[row * w..row * w + w].copy_from_slice(&src[row * width..row * width + w]);
    }
    let chroma = &src[width * height..];
    for row in 0..h / 2 {
        for col in 0..w / 2 {
            let at = row * width + col * 2;
            u[row * (w / 2) + col] = chroma[at];
            v[row * (w / 2) + col] = chroma[at + 1];
        }
    }
    Some(I420 {
        width: w,
        height: h,
        data,
    })
}

/// From RGB (as the camera's JPEG decoded): BT.601, the chroma of the
/// top left pixel of each four.
pub fn from_rgb(src: &[u8], width: usize, height: usize) -> Option<I420> {
    let (w, h) = (even(width), even(height));
    if w == 0 || h == 0 || src.len() < width * height * 3 {
        return None;
    }
    let mut data = vec![0u8; w * h * 3 / 2];
    let (y, uv) = data.split_at_mut(w * h);
    let (u, v) = uv.split_at_mut(w * h / 4);
    let clamp = |x: i32| u8::try_from(x.clamp(0, 255)).unwrap_or(0);
    for row in 0..h {
        for col in 0..w {
            let at = (row * width + col) * 3;
            let (r, g, b) = (
                i32::from(src[at]),
                i32::from(src[at + 1]),
                i32::from(src[at + 2]),
            );
            y[row * w + col] = clamp(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
            if row % 2 == 0 && col % 2 == 0 {
                let c = row / 2 * (w / 2) + col / 2;
                u[c] = clamp(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128);
                v[c] = clamp(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128);
            }
        }
    }
    Some(I420 {
        width: w,
        height: h,
        data,
    })
}

// a plane of `w`×`h` made `to_w`×`to_h`, bilinear
fn scale_plane(src: &[u8], w: usize, h: usize, to_w: usize, to_h: usize) -> Vec<u8> {
    let mut out = vec![0u8; to_w * to_h];
    // 16.16 fixed point, the centers of the pixels lined up
    let step_x = ((w << 16) / to_w.max(1)) as i64;
    let step_y = ((h << 16) / to_h.max(1)) as i64;
    let (max_x, max_y) = (w as i64 - 1, h as i64 - 1);
    for oy in 0..to_h {
        let sy = ((oy as i64 * step_y) + step_y / 2 - 32768).max(0);
        let (y0, fy) = ((sy >> 16).min(max_y), sy & 0xffff);
        let y1 = (y0 + 1).min(max_y);
        for ox in 0..to_w {
            let sx = ((ox as i64 * step_x) + step_x / 2 - 32768).max(0);
            let (x0, fx) = ((sx >> 16).min(max_x), sx & 0xffff);
            let x1 = (x0 + 1).min(max_x);
            let at = |x: i64, y: i64| i64::from(src[(y * w as i64 + x) as usize]);
            let top = at(x0, y0) * (65536 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (65536 - fx) + at(x1, y1) * fx;
            let value = (top * (65536 - fy) + bottom * fy) >> 32;
            out[oy * to_w + ox] = u8::try_from(value.clamp(0, 255)).unwrap_or(0);
        }
    }
    out
}

/// The picture no taller than `height` (the width as it comes, even),
/// as it is when already small enough.
pub fn fit(picture: I420, height: usize) -> I420 {
    let whole = picture.data.len() == picture.width * picture.height * 3 / 2;
    if picture.height <= height || height < 2 || !whole {
        return picture;
    }
    let to_h = even(height);
    let to_w = even(picture.width * to_h / picture.height).max(2);
    let (y, u, v) = picture.planes();
    let (w, h) = (picture.width, picture.height);
    let data = [
        scale_plane(y, w, h, to_w, to_h),
        scale_plane(u, w / 2, h / 2, to_w / 2, to_h / 2),
        scale_plane(v, w / 2, h / 2, to_w / 2, to_h / 2),
    ]
    .concat();
    I420 {
        width: to_w,
        height: to_h,
        data,
    }
}

/// The picture to show, no wider than `width`: RGBA, nearest pixel.
pub fn preview(picture: &I420, width: usize) -> (usize, usize, Vec<u8>) {
    let to_w = picture.width.min(width).max(1);
    let to_h = (picture.height * to_w / picture.width.max(1)).max(1);
    let (y, u, v) = picture.planes();
    let mut rgba = vec![255u8; to_w * to_h * 4];
    let clamp = |x: i32| u8::try_from(x.clamp(0, 255)).unwrap_or(0);
    for oy in 0..to_h {
        let sy = oy * picture.height / to_h;
        for ox in 0..to_w {
            let sx = ox * picture.width / to_w;
            let c = sy / 2 * (picture.width / 2) + sx / 2;
            let (luma, cb, cr) = (
                i32::from(y.get(sy * picture.width + sx).copied().unwrap_or(0)) - 16,
                i32::from(u.get(c).copied().unwrap_or(128)) - 128,
                i32::from(v.get(c).copied().unwrap_or(128)) - 128,
            );
            let at = (oy * to_w + ox) * 4;
            rgba[at] = clamp((298 * luma + 409 * cr + 128) >> 8);
            rgba[at + 1] = clamp((298 * luma - 100 * cb - 208 * cr + 128) >> 8);
            rgba[at + 2] = clamp((298 * luma + 516 * cb + 128) >> 8);
        }
    }
    (to_w, to_h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_the_camera_formats() {
        // 4×2, YUYV: Y0 U Y1 V
        let yuyv = [
            10, 100, 20, 200, 30, 101, 40, 201, //
            50, 102, 60, 202, 70, 103, 80, 203,
        ];
        let p = from_yuyv(&yuyv, 4, 2).unwrap();
        assert_eq!(p.data, [10, 20, 30, 40, 50, 60, 70, 80, 100, 101, 200, 201]);
        let nv12 = [1, 2, 3, 4, 5, 6, 7, 8, 100, 200, 101, 201];
        let p = from_nv12(&nv12, 4, 2).unwrap();
        assert_eq!(p.data, [1, 2, 3, 4, 5, 6, 7, 8, 100, 101, 200, 201]);
        // white and black
        let rgb = [255; 2 * 2 * 3];
        let p = from_rgb(&rgb, 2, 2).unwrap();
        assert_eq!(p.data, [235, 235, 235, 235, 128, 128]);
        assert!(from_yuyv(&yuyv, 8, 8).is_none());
    }

    #[test]
    fn odd_sizes_made_even() {
        let yuyv = vec![16u8; 6 * 3 * 2];
        let p = from_yuyv(&yuyv, 6, 3).unwrap();
        assert_eq!((p.width, p.height, p.data.len()), (6, 2, 18));
    }

    #[test]
    fn smaller_for_the_quality() {
        let picture = I420 {
            width: 1920,
            height: 1080,
            data: vec![90; 1920 * 1080 * 3 / 2],
        };
        let small = fit(picture.clone(), 720);
        assert_eq!((small.width, small.height), (1280, 720));
        assert_eq!(small.data.len(), 1280 * 720 * 3 / 2);
        // a flat picture stays flat
        assert!(small.data.iter().all(|b| *b == 90));
        assert_eq!(fit(picture.clone(), 1080), picture);
    }

    #[test]
    fn the_preview() {
        let picture = I420 {
            width: 1280,
            height: 720,
            data: [vec![235; 1280 * 720], vec![128; 1280 * 720 / 2]].concat(),
        };
        let (w, h, rgba) = preview(&picture, 640);
        assert_eq!((w, h, rgba.len()), (640, 360, 640 * 360 * 4));
        assert_eq!(&rgba[..4], &[255, 255, 255, 255]);
    }
}
