use adw::gtk::{gdk::RGBA, Align, DrawingArea};
use adw::prelude::*;
use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct OtpCountdownCircle {
    area: DrawingArea,
    fraction: Rc<Cell<f64>>,
}

impl OtpCountdownCircle {
    pub(super) fn new() -> Self {
        let area = DrawingArea::new();
        area.set_content_width(16);
        area.set_content_height(16);
        area.set_valign(Align::Center);
        area.set_visible(false);

        let fraction = Rc::new(Cell::new(0.0_f64));
        let fraction_for_draw = fraction.clone();
        area.set_draw_func(move |area, cr, width, height| {
            let fraction = fraction_for_draw.get().clamp(0.0, 1.0);
            let radius = (f64::from(width.min(height)) / 2.0) - 2.0;
            let center_x = f64::from(width) / 2.0;
            let center_y = f64::from(height) / 2.0;

            cr.set_line_width(2.0);
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.18);
            cr.arc(center_x, center_y, radius, 0.0, TAU);
            let _ = cr.stroke();

            let accent = area
                .style_context()
                .lookup_color("accent_color")
                .unwrap_or_else(default_accent_color);
            cr.set_source_rgba(
                f64::from(accent.red()),
                f64::from(accent.green()),
                f64::from(accent.blue()),
                f64::from(accent.alpha()),
            );
            cr.arc(
                center_x,
                center_y,
                radius,
                -FRAC_PI_2,
                TAU.mul_add(fraction, -FRAC_PI_2),
            );
            let _ = cr.stroke();
        });

        Self { area, fraction }
    }

    pub(super) const fn widget(&self) -> &DrawingArea {
        &self.area
    }

    pub(super) fn set_visible(&self, visible: bool) {
        self.area.set_visible(visible);
    }

    pub(super) fn set_fraction(&self, fraction: f64) {
        self.fraction.set(fraction.clamp(0.0, 1.0));
        self.area.queue_draw();
    }

    pub(super) fn set_tooltip_text(&self, tooltip: Option<&str>) {
        self.area.set_tooltip_text(tooltip);
    }
}

const fn default_accent_color() -> RGBA {
    RGBA::new(0.18, 0.55, 0.92, 1.0)
}
