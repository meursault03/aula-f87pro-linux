use super::lighting_preview::{self, Lighting};
use aula_f87pro::KEYS;
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc};

// Widths include the gap: all rows share the same quarter-key grid.
const ROWS: &[&[(u8, i32)]] = &[
    &[
        (1, 4),
        (0, 4),
        (2, 4),
        (3, 4),
        (4, 4),
        (5, 4),
        (0, 2),
        (6, 4),
        (7, 4),
        (8, 4),
        (9, 4),
        (0, 2),
        (10, 4),
        (11, 4),
        (12, 4),
        (13, 4),
        (0, 2),
        (28, 4),
        (29, 4),
        (30, 4),
    ],
    &[
        (14, 4),
        (15, 4),
        (16, 4),
        (17, 4),
        (18, 4),
        (19, 4),
        (20, 4),
        (21, 4),
        (22, 4),
        (23, 4),
        (24, 4),
        (25, 4),
        (26, 4),
        (27, 8),
        (0, 2),
        (46, 4),
        (47, 4),
        (48, 4),
    ],
    &[
        (31, 6),
        (32, 4),
        (33, 4),
        (34, 4),
        (35, 4),
        (36, 4),
        (37, 4),
        (38, 4),
        (39, 4),
        (40, 4),
        (41, 4),
        (42, 4),
        (43, 4),
        (44, 6),
        (0, 2),
        (45, 4),
        (62, 4),
        (63, 4),
    ],
    &[
        (49, 7),
        (50, 4),
        (51, 4),
        (52, 4),
        (53, 4),
        (54, 4),
        (55, 4),
        (56, 4),
        (57, 4),
        (58, 4),
        (59, 4),
        (60, 4),
        (61, 9),
    ],
    &[
        (64, 9),
        (65, 4),
        (66, 4),
        (67, 4),
        (68, 4),
        (69, 4),
        (70, 4),
        (71, 4),
        (72, 4),
        (73, 4),
        (74, 4),
        (75, 11),
        (0, 6),
        (76, 4),
    ],
    &[
        (78, 5),
        (79, 5),
        (80, 5),
        (81, 25),
        (82, 5),
        (83, 5),
        (84, 5),
        (77, 5),
        (0, 2),
        (85, 4),
        (86, 4),
        (87, 4),
    ],
];

pub struct Board {
    pub widget: gtk::Overlay,
    pub keys: Vec<(u8, gtk::ToggleButton)>,
}

pub fn build(
    on_select: impl Fn(u8) + Clone + 'static,
    lighting: Rc<Cell<Option<Lighting>>>,
    animate: Rc<Cell<bool>>,
) -> Board {
    let board = gtk::Grid::new();
    board.set_column_homogeneous(true);
    board.set_row_spacing(5);
    board.set_width_request(74 * 9);
    board.add_css_class("keyboard");
    board.set_halign(gtk::Align::Center);
    let mut group: Option<gtk::ToggleButton> = None;
    let mut keys = Vec::new();
    for (row_index, row) in ROWS.iter().enumerate() {
        let mut column = 0;
        for &(id, width) in *row {
            if id == 0 {
                column += width;
                continue;
            }
            let name = KEYS[usize::from(id) - 1].0;
            let short = match name {
                "Backspace" => "⌫",
                "CapsLock" => "Caps",
                "LShift" | "RShift" => "Shift",
                "LCtrl" | "RCtrl" => "Ctrl",
                "LAlt" | "RAlt" => "Alt",
                "LWin" => "Super",
                "SrcLock" => "ScrLk",
                "PrtSc" => "PrtSc",
                "Space" => "",
                "App" => "Menu",
                "Up" => "↑",
                "Left" => "←",
                "Down" => "↓",
                "Right" => "→",
                _ => name,
            };
            let key = gtk::ToggleButton::with_label(short);
            key.add_css_class("keycap");
            key.set_margin_end(4);
            key.set_height_request(38);
            key.set_tooltip_text(Some(name));
            key.update_property(&[gtk::accessible::Property::Label(name)]);
            if let Some(first) = &group {
                key.set_group(Some(first));
            } else {
                group = Some(key.clone());
            }
            if id == 50 {
                key.set_active(true);
            }
            let select = on_select.clone();
            key.connect_clicked(move |key| {
                if key.is_active() {
                    select(id);
                }
            });
            board.attach(&key, column, row_index as i32, width, 1);
            column += width;
            keys.push((id, key));
        }
    }
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&board));
    lighting_preview::attach(&overlay, &keys, lighting, animate);
    Board {
        widget: overlay,
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_physical_key_occurs_once() {
        let mut ids: Vec<_> = ROWS
            .iter()
            .flat_map(|r| r.iter().map(|&(id, _)| id))
            .filter(|&id| id != 0)
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=87).collect::<Vec<_>>());
    }
}
