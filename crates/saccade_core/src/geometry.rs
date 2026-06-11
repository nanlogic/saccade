use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssPx(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DevicePx(pub f32);

/// Unit: CSS px, origin = webview top-left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssPoint {
    pub x: f32,
    pub y: f32,
}

/// Unit: device px, origin = webview top-left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DevicePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CssRect {
    /// Inclusive on the min edge, exclusive on the max edge.
    pub fn contains(&self, p: CssPoint) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && p.x >= self.x
            && p.y >= self.y
            && p.x < self.x + self.w
            && p.y < self.y + self.h
    }

    pub fn center(&self) -> CssPoint {
        CssPoint {
            x: self.x + self.w / 2.0,
            y: self.y + self.h / 2.0,
        }
    }

    pub fn inside(&self, outer: &CssRect) -> bool {
        self.w >= 0.0
            && self.h >= 0.0
            && outer.w >= 0.0
            && outer.h >= 0.0
            && self.x >= outer.x
            && self.y >= outer.y
            && self.x + self.w <= outer.x + outer.w
            && self.y + self.h <= outer.y + outer.h
    }

    pub fn intersect(&self, other: &CssRect) -> Option<CssRect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);

        (x1 > x0 && y1 > y0).then_some(CssRect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportInfo {
    pub width_css: f32,
    pub height_css: f32,
    /// Asserted == 1.0 for benchmark runs; mapping remains general.
    pub device_scale_factor: f32,
    pub page_zoom: f32,
}

/// The only place css<->device conversion happens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CoordinateMapper {
    pub viewport: ViewportInfo,
}

impl CoordinateMapper {
    pub fn css_to_device(&self, p: CssPoint) -> DevicePoint {
        let scale = self.scale();
        DevicePoint {
            x: p.x * scale,
            y: p.y * scale,
        }
    }

    pub fn device_to_css(&self, p: DevicePoint) -> CssPoint {
        let scale = self.scale();
        CssPoint {
            x: p.x / scale,
            y: p.y / scale,
        }
    }

    /// Returns x, y, width, height in device px, floor/ceil expanded to cover the CSS rect.
    pub fn css_rect_to_device_box(&self, r: CssRect) -> (i32, i32, i32, i32) {
        let scale = self.scale();
        let x0 = (r.x * scale).floor() as i32;
        let y0 = (r.y * scale).floor() as i32;
        let x1 = ((r.x + r.w) * scale).ceil() as i32;
        let y1 = ((r.y + r.h) * scale).ceil() as i32;

        (x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    fn scale(&self) -> f32 {
        let scale = self.viewport.device_scale_factor * self.viewport.page_zoom;
        assert!(
            scale.is_finite() && scale > 0.0,
            "viewport scale must be finite and positive"
        );
        scale
    }
}

/// Resolved by calibration: which space Servo input points are interpreted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSpace {
    CssLogical,
    DevicePhysical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_edges_are_min_inclusive_max_exclusive() {
        let rect = CssRect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        };

        assert!(rect.contains(CssPoint { x: 10.0, y: 20.0 }));
        assert!(rect.contains(CssPoint {
            x: 39.999,
            y: 59.999
        }));
        assert!(!rect.contains(CssPoint { x: 40.0, y: 20.0 }));
        assert!(!rect.contains(CssPoint { x: 10.0, y: 60.0 }));
    }

    #[test]
    fn rect_intersection_and_inside_work() {
        let outer = CssRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 80.0,
        };
        let inner = CssRect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        };
        let clipped = CssRect {
            x: 80.0,
            y: 70.0,
            w: 30.0,
            h: 30.0,
        };

        assert!(inner.inside(&outer));
        assert!(!clipped.inside(&outer));
        assert_eq!(
            outer.intersect(&clipped),
            Some(CssRect {
                x: 80.0,
                y: 70.0,
                w: 20.0,
                h: 10.0,
            })
        );
    }

    #[test]
    fn coordinate_mapping_round_trips_for_required_dprs() {
        for dpr in [1.0, 1.5, 2.0, 3.0] {
            let mapper = CoordinateMapper {
                viewport: ViewportInfo {
                    width_css: 1280.0,
                    height_css: 800.0,
                    device_scale_factor: dpr,
                    page_zoom: 1.0,
                },
            };
            let css = CssPoint {
                x: 123.25,
                y: 456.5,
            };
            let device = mapper.css_to_device(css);
            let round_trip = mapper.device_to_css(device);

            assert!((round_trip.x - css.x).abs() <= f32::EPSILON * 16.0);
            assert!((round_trip.y - css.y).abs() <= f32::EPSILON * 16.0);
        }
    }

    #[test]
    fn device_box_expands_to_cover_fractional_css_rect() {
        let mapper = CoordinateMapper {
            viewport: ViewportInfo {
                width_css: 1280.0,
                height_css: 800.0,
                device_scale_factor: 1.5,
                page_zoom: 1.0,
            },
        };

        assert_eq!(
            mapper.css_rect_to_device_box(CssRect {
                x: 10.2,
                y: 20.2,
                w: 5.1,
                h: 7.1,
            }),
            (15, 30, 8, 11)
        );
    }
}
