use gtk::{glib, prelude::*};
use std::{cell::Cell, f64::consts::TAU, rc::Rc};

#[derive(Clone, Copy, PartialEq)]
pub struct Lighting {
    effect: u8,
    brightness: f64,
    speed: f64,
    palette: u8,
}

impl Lighting {
    pub fn read(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 128
            || bytes[9] != 0
            || bytes[10] == 0
            || !aula_f87pro::LED_EFFECTS.contains(&bytes[10])
        {
            return None;
        }
        let offset = 58 + 2 * usize::from(bytes[10] - 1);
        Some(Self {
            effect: bytes[10],
            brightness: f64::from(bytes[offset].min(4)) / 4.0,
            speed: 0.25 + f64::from((bytes[offset + 1] >> 4).min(4)) * 0.22,
            palette: (bytes[offset + 1] & 15).min(7),
        })
    }

    // shortcut: illustrative motion, compare each algorithm with physical LEDs before claiming fidelity.
    fn color(self, x: f64, y: f64, seconds: f64) -> [f64; 3] {
        let t = seconds * self.speed;
        let radius = ((x - 0.5).powi(2) + (y * 0.35 - 0.175).powi(2)).sqrt();
        let wave = |phase: f64| (0.5 + 0.5 * (TAU * phase).cos()).powi(4);
        let trail = |phase: f64| (1.0 - phase.rem_euclid(1.0)).powi(5);
        let level = match self.effect {
            1 | 3 | 15 => 1.0,
            2 => 0.08 + 0.92 * (0.5 - 0.5 * (TAU * t).cos()),
            4 => trail(t - x - y * 0.2),
            5 => trail(t + x * 7.13 - y),
            7 => wave(x * 2.0 - t),
            8 => wave(t * 0.7 + x * 13.37 + y * 7.17).powi(3),
            10 => wave(x + y * 0.5 - t),
            11 => 0.15 + 0.85 * wave(x * 2.0 - t),
            12 => wave(radius * 3.0 - t),
            13 => wave(y - (TAU * (x - t)).sin() * 0.5),
            16 => trail(t - y - x * 0.15),
            17 => wave(radius * 4.0 - t),
            _ => 0.0,
        } * self.brightness;
        let multicolor = self.palette == 7 || [3, 15, 16, 17].contains(&self.effect);
        let rgb = if multicolor {
            let hue = match self.effect {
                1 => x * 7.13 + y * 13.37,
                3 => -t * 0.4,
                15 => (y * 0.35 - 0.175).atan2(x - 0.5) / TAU - t,
                17 => radius * 2.0 - t,
                _ => x + y * 0.2 - t * 0.4,
            };
            [0.0, 2.0 / 3.0, 1.0 / 3.0].map(|offset| {
                (((hue + offset).rem_euclid(1.0) * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0)
            })
        } else {
            [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 1.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
            ][usize::from(self.palette)]
        };
        rgb.map(|channel| channel * level)
    }
}

pub fn attach(
    overlay: &gtk::Overlay,
    keys: &[(u8, gtk::ToggleButton)],
    lighting: Rc<Cell<Option<Lighting>>>,
    animate: Rc<Cell<bool>>,
) {
    let canvas = gtk::DrawingArea::new();
    canvas.set_can_target(false);
    overlay.add_overlay(&canvas);
    let keys: Vec<_> = keys.iter().map(|(_, key)| key.downgrade()).collect();
    let time = Rc::new(Cell::new(0.0));
    let clock = time.clone();
    let state = lighting.clone();
    canvas.set_draw_func(move |area, context, width, height| {
        let Some(model) = lighting.get() else {
            return;
        };
        for key in keys.iter().filter_map(glib::WeakRef::upgrade) {
            let Some(bounds) = key.compute_bounds(area) else {
                continue;
            };
            let x = f64::from(bounds.x());
            let y = f64::from(bounds.y());
            let w = f64::from(bounds.width());
            let h = f64::from(bounds.height());
            let [r, g, b] = model.color(
                (x + w / 2.0) / f64::from(width.max(1)),
                (y + h / 2.0) / f64::from(height.max(1)),
                clock.get(),
            );
            // Keep labels, selection and pending-change borders readable above the glow.
            context.set_source_rgba(r, g, b, 0.18);
            context.rectangle(x + 3.0, y + 3.0, w - 6.0, h - 7.0);
            let _ = context.fill();
            context.set_source_rgb(r, g, b);
            context.rectangle(x + 5.0, y + h - 5.0, w - 10.0, 2.0);
            let _ = context.fill();
        }
    });
    let last = Cell::new(0);
    let previous = Cell::new(None);
    canvas.add_tick_callback(move |canvas, frame| {
        let now = frame.frame_time();
        if now - last.get() >= 33_333 {
            last.set(now);
            let model = state.get();
            let motion = animate.get()
                && canvas.settings().is_gtk_enable_animations()
                && model.is_some_and(|m| m.effect != 1 && m.brightness > 0.0);
            let next = if motion {
                now as f64 / 1_000_000.0
            } else {
                0.0
            };
            let changed = previous.replace(model) != model;
            let advanced = time.replace(next) != next;
            if changed || advanced {
                canvas.queue_draw();
            }
        }
        glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leds(effect: u8, brightness: u8, speed: u8, palette: u8) -> Vec<u8> {
        let mut bytes = vec![0; 128];
        bytes[10] = effect;
        if (1..=18).contains(&effect) {
            let offset = 58 + 2 * usize::from(effect - 1);
            bytes[offset] = brightness;
            bytes[offset + 1] = speed << 4 | palette;
        }
        bytes
    }

    #[test]
    fn unknown_custom_and_off_have_no_invented_colors() {
        for bytes in [
            vec![],
            leds(0, 4, 0, 0),
            leds(6, 4, 0, 0),
            leds(9, 4, 0, 0),
            leds(14, 4, 0, 0),
            leds(18, 4, 0, 0),
            leds(19, 4, 0, 0),
        ] {
            assert!(Lighting::read(&bytes).is_none());
        }
        let mut custom = leds(1, 4, 0, 0);
        custom[9] = 1;
        assert!(Lighting::read(&custom).is_none());
    }

    #[test]
    fn fixed_palette_and_brightness_match_settings() {
        let red = Lighting::read(&leds(1, 4, 0, 0)).unwrap();
        assert_eq!(red.color(0.4, 0.5, 0.0), [1.0, 0.0, 0.0]);
        assert_eq!(red.color(0.4, 0.5, 0.0), red.color(0.4, 0.5, 9.0));
        let dark = Lighting::read(&leds(1, 0, 0, 0)).unwrap();
        assert_eq!(dark.color(0.4, 0.5, 0.0), [0.0; 3]);
        let multicolor = Lighting::read(&leds(1, 4, 0, 7)).unwrap();
        assert_eq!(
            multicolor.color(0.4, 0.5, 0.0),
            multicolor.color(0.4, 0.5, 9.0)
        );
    }

    #[test]
    fn animated_effects_are_bounded_and_change_over_time() {
        for (effect, palette) in aula_f87pro::LED_EFFECTS
            .iter()
            .copied()
            .filter(|&id| id >= 2)
            .flat_map(|effect| (0..=7).map(move |p| (effect, p)))
        {
            let preview = Lighting::read(&leds(effect, 4, 3, palette)).unwrap();
            let mut changes = false;
            for key in 0..87 {
                let x = f64::from(key % 15) / 14.0;
                let y = f64::from(key / 15) / 5.0;
                let first = preview.color(x, y, 0.0);
                let later = preview.color(x, y, 0.73);
                changes |= first != later;
                assert!(
                    first
                        .into_iter()
                        .chain(later)
                        .all(|c| (0.0..=1.0).contains(&c))
                );
            }
            assert!(changes, "effect {effect}, palette {palette} is static");
        }
    }
}
