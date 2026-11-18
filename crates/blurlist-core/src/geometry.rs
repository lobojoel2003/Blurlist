//! Small 2-D geometry toolkit: boxes, landmarks, non-maximum suppression and
//! the similarity transform used to align faces before embedding.

use serde::{Deserialize, Serialize};

/// A point in image pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Axis-aligned rectangle in pixel coordinates (`x`, `y` is the top-left corner).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl BBox {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_corners(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x: x1.min(x2), y: y1.min(y2), w: (x2 - x1).abs(), h: (y2 - y1).abs() }
    }

    pub fn x2(&self) -> f32 {
        self.x + self.w
    }

    pub fn y2(&self) -> f32 {
        self.y + self.h
    }

    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Length of the shorter side; used as the "face size".
    pub fn short_side(&self) -> f32 {
        self.w.min(self.h)
    }

    pub fn intersection(&self, other: &BBox) -> f32 {
        let ix = (self.x2().min(other.x2()) - self.x.max(other.x)).max(0.0);
        let iy = (self.y2().min(other.y2()) - self.y.max(other.y)).max(0.0);
        ix * iy
    }

    /// Intersection over union.
    pub fn iou(&self, other: &BBox) -> f32 {
        let inter = self.intersection(other);
        let union = self.area() + other.area() - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }

    /// Intersection over the smaller of the two areas. Catches the case where a
    /// partial detection sits inside a complete one.
    pub fn io_min(&self, other: &BBox) -> f32 {
        let inter = self.intersection(other);
        let min_area = self.area().min(other.area());
        if min_area <= 0.0 {
            0.0
        } else {
            inter / min_area
        }
    }

    /// Scale width/height around the centre by independent factors.
    pub fn expand(&self, fx: f32, fy: f32) -> BBox {
        let c = self.center();
        let w = self.w * fx;
        let h = self.h * fy;
        BBox::new(c.x - w / 2.0, c.y - h / 2.0, w, h)
    }

    /// Clamp to `[0, width] x [0, height]`.
    pub fn clamp(&self, width: f32, height: f32) -> BBox {
        let x1 = self.x.clamp(0.0, width);
        let y1 = self.y.clamp(0.0, height);
        let x2 = self.x2().clamp(0.0, width);
        let y2 = self.y2().clamp(0.0, height);
        BBox::from_corners(x1, y1, x2, y2)
    }

    /// Integer pixel rectangle (x, y, w, h) covering this box, clipped to the image.
    pub fn to_pixel_rect(&self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let x1 = self.x.floor().max(0.0) as i64;
        let y1 = self.y.floor().max(0.0) as i64;
        let x2 = (self.x2().ceil() as i64).min(width as i64);
        let y2 = (self.y2().ceil() as i64).min(height as i64);
        if x2 <= x1 || y2 <= y1 {
            return None;
        }
        Some((x1 as u32, y1 as u32, (x2 - x1) as u32, (y2 - y1) as u32))
    }

    pub fn scale(&self, s: f32) -> BBox {
        BBox::new(self.x * s, self.y * s, self.w * s, self.h * s)
    }

    pub fn translate(&self, dx: f32, dy: f32) -> BBox {
        BBox::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// Five facial landmarks in the canonical InsightFace / OpenCV order:
/// image-left eye, image-right eye, nose tip, image-left mouth corner,
/// image-right mouth corner.
pub type Landmarks = [Point; 5];

/// A 2x3 affine matrix `[a b tx; c d ty]` mapping source → destination.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub m: [[f32; 3]; 2],
}

impl Affine {
    pub fn apply(&self, p: Point) -> Point {
        let m = &self.m;
        Point::new(m[0][0] * p.x + m[0][1] * p.y + m[0][2], m[1][0] * p.x + m[1][1] * p.y + m[1][2])
    }

    /// Inverse of the affine transform, if it is invertible.
    pub fn invert(&self) -> Option<Affine> {
        let [[a, b, tx], [c, d, ty]] = self.m;
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return None;
        }
        let ia = d / det;
        let ib = -b / det;
        let ic = -c / det;
        let id = a / det;
        Some(Affine { m: [[ia, ib, -(ia * tx + ib * ty)], [ic, id, -(ic * tx + id * ty)]] })
    }
}

/// Least-squares similarity transform (rotation, uniform scale, translation)
/// mapping `src` points onto `dst` points. This is the 2-D special case of the
/// Umeyama method used by InsightFace's `norm_crop` and OpenCV's `alignCrop`.
pub fn estimate_similarity(src: &[Point], dst: &[Point]) -> Option<Affine> {
    let n = src.len();
    if n < 2 || n != dst.len() {
        return None;
    }
    let nf = n as f64;
    let (mut msx, mut msy, mut mdx, mut mdy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (s, d) in src.iter().zip(dst) {
        msx += s.x as f64;
        msy += s.y as f64;
        mdx += d.x as f64;
        mdy += d.y as f64;
    }
    msx /= nf;
    msy /= nf;
    mdx /= nf;
    mdy /= nf;

    let (mut sxx, mut sxy, mut var) = (0.0f64, 0.0f64, 0.0f64);
    for (s, d) in src.iter().zip(dst) {
        let ax = s.x as f64 - msx;
        let ay = s.y as f64 - msy;
        let bx = d.x as f64 - mdx;
        let by = d.y as f64 - mdy;
        sxx += ax * bx + ay * by;
        sxy += ax * by - ay * bx;
        var += ax * ax + ay * ay;
    }
    if var < 1e-12 {
        return None;
    }
    let norm = (sxx * sxx + sxy * sxy).sqrt();
    let scale = norm / var;
    let (cos, sin) = if norm < 1e-12 { (1.0, 0.0) } else { (sxx / norm, sxy / norm) };
    let a = scale * cos;
    let b = -scale * sin;
    let c = scale * sin;
    let d = scale * cos;
    let tx = mdx - (a * msx + b * msy);
    let ty = mdy - (c * msx + d * msy);
    Some(Affine { m: [[a as f32, b as f32, tx as f32], [c as f32, d as f32, ty as f32]] })
}

/// Anything that can take part in non-maximum suppression.
pub trait Scored {
    fn bbox(&self) -> &BBox;
    fn score(&self) -> f32;
}

/// Greedy non-maximum suppression. Keeps the highest scoring item of each
/// overlapping cluster. Two boxes overlap when their IoU exceeds `iou` or the
/// smaller one lies mostly (`io_min`) inside the larger one.
pub fn nms<T: Scored + Clone>(items: &[T], iou: f32, io_min: f32) -> Vec<T> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[b].score().partial_cmp(&items[a].score()).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<T> = Vec::new();
    'outer: for idx in order {
        let cand = &items[idx];
        for k in &keep {
            let a = k.bbox();
            let b = cand.bbox();
            if a.iou(b) > iou || a.io_min(b) > io_min {
                continue 'outer;
            }
        }
        keep.push(cand.clone());
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct S(BBox, f32);
    impl Scored for S {
        fn bbox(&self) -> &BBox {
            &self.0
        }
        fn score(&self) -> f32 {
            self.1
        }
    }

    #[test]
    fn iou_basics() {
        let a = BBox::new(0.0, 0.0, 10.0, 10.0);
        let b = BBox::new(5.0, 0.0, 10.0, 10.0);
        assert!((a.iou(&b) - 50.0 / 150.0).abs() < 1e-6);
        assert_eq!(a.iou(&BBox::new(20.0, 20.0, 5.0, 5.0)), 0.0);
        let inner = BBox::new(2.0, 2.0, 4.0, 4.0);
        assert!((a.io_min(&inner) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn nms_keeps_best_and_separate() {
        let items = vec![
            S(BBox::new(0.0, 0.0, 10.0, 10.0), 0.9),
            S(BBox::new(1.0, 1.0, 10.0, 10.0), 0.8),
            S(BBox::new(50.0, 50.0, 10.0, 10.0), 0.7),
            S(BBox::new(2.0, 2.0, 3.0, 3.0), 0.95), // small box inside the first
        ];
        let kept = nms(&items, 0.4, 0.7);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].1, 0.95);
        assert_eq!(kept[1].1, 0.7);
    }

    #[test]
    fn similarity_recovers_known_transform() {
        let src = [
            Point::new(10.0, 10.0),
            Point::new(30.0, 12.0),
            Point::new(20.0, 25.0),
            Point::new(12.0, 35.0),
            Point::new(28.0, 36.0),
        ];
        let theta = 0.3f32;
        let s = 1.7f32;
        let (tx, ty) = (5.0f32, -3.0f32);
        let dst: Vec<Point> = src
            .iter()
            .map(|p| {
                Point::new(
                    s * (theta.cos() * p.x - theta.sin() * p.y) + tx,
                    s * (theta.sin() * p.x + theta.cos() * p.y) + ty,
                )
            })
            .collect();
        let m = estimate_similarity(&src, &dst).unwrap();
        for (p, q) in src.iter().zip(&dst) {
            let r = m.apply(*p);
            assert!((r.x - q.x).abs() < 1e-3 && (r.y - q.y).abs() < 1e-3);
        }
        let inv = m.invert().unwrap();
        let back = inv.apply(m.apply(src[2]));
        assert!((back.x - src[2].x).abs() < 1e-3 && (back.y - src[2].y).abs() < 1e-3);
    }

    #[test]
    fn pixel_rect_clips() {
        let b = BBox::new(-5.0, -5.0, 20.0, 20.0);
        assert_eq!(b.to_pixel_rect(10, 10), Some((0, 0, 10, 10)));
        assert_eq!(BBox::new(50.0, 50.0, 5.0, 5.0).to_pixel_rect(10, 10), None);
    }
}
