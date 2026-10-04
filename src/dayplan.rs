//! Day plan: a 24-hour clock of 48 half-hour slots. Pick a task (a "brush"),
//! then click or drag around the ring to give it time. Dragging over the
//! task's own slots clears them; the eraser clears anything.

use crate::model::{Block, SLOTS, slot_time};
use crate::ui::{Ui, label};
use adw::prelude::*;
use chrono::{NaiveDate, Timelike};
use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    f64::consts::PI,
    rc::Rc,
};

pub type Rgb = (f64, f64, f64);

const PALETTE: [Rgb; 10] = [
    (0.31, 0.55, 0.97),
    (0.96, 0.55, 0.20),
    (0.22, 0.72, 0.47),
    (0.86, 0.34, 0.58),
    (0.58, 0.45, 0.88),
    (0.18, 0.70, 0.76),
    (0.93, 0.74, 0.20),
    (0.89, 0.36, 0.33),
    (0.50, 0.64, 0.28),
    (0.56, 0.57, 0.66),
];

pub fn color(i: usize) -> Rgb {
    PALETTE[i % PALETTE.len()]
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Brush {
    None,
    Row(u64),
    Eraser,
}

/// Something that can be scheduled on the day (a todo or a focus).
pub struct Item {
    pub rid: u64,
    pub title: String,
    pub goal: String,
    pub color: Rgb,
}

/// Minutes → "1h 30m".
pub fn duration(slots: usize) -> String {
    let (h, m) = (slots / 2, if slots % 2 == 1 { 30 } else { 0 });
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// A small filled circle in an item's colour.
pub fn dot(rgb: Rgb, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .valign(gtk::Align::Center)
        .build();
    area.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgb(rgb.0, rgb.1, rgb.2);
        cr.arc(w as f64 / 2.0, h as f64 / 2.0, w.min(h) as f64 / 2.0, 0.0, 2.0 * PI);
        let _ = cr.fill();
    });
    area
}

/// The right-hand "Day plan" panel.
pub fn panel(ui: &Rc<Ui>, date: NaiveDate, items: &Rc<Vec<Item>>) -> gtk::Widget {
    let panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .css_classes(["day-plan"])
        .build();

    let blocks = ui.store.borrow().blocks(date);
    let planned: usize = blocks.iter().map(|b| b.len as usize).sum();
    let unscheduled = items.iter().filter(|i| !blocks.iter().any(|b| b.row == i.rid)).count();
    panel.append(&label("Day plan", &["section-title", "day-plan-title"]));
    let mut summary = format!("{} planned", duration(planned));
    if unscheduled > 0 {
        summary += &format!(" · {unscheduled} unscheduled");
    }
    panel.append(&label(&summary, &["dim-label", "caption"]));

    panel.append(&clock(ui, date, items));

    let brush = ui.brush.get();
    let hint = match brush {
        Brush::None => "Pick a task below, then click or drag around the clock to give it time.".to_string(),
        Brush::Row(rid) => {
            let title = items.iter().find(|i| i.rid == rid).map_or("this task", |i| i.title.as_str());
            format!("Painting “{title}”. Drag over its own slots to clear them.")
        }
        Brush::Eraser => "Eraser: drag over slots to clear them.".to_string(),
    };
    panel.append(&label(&hint, &["dim-label", "caption", "day-plan-hint"]));

    // Task chips: unscheduled first (already in priority order), then scheduled.
    if items.is_empty() {
        panel.append(&label("Nothing to plan for this day yet.", &["dim-label"]));
    } else {
        let chips = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(2)
            .column_spacing(6)
            .row_spacing(6)
            .homogeneous(false)
            .build();
        let time_for = |rid: u64| blocks.iter().filter(|b| b.row == rid).map(|b| b.len as usize).sum::<usize>();
        let mut ordered: Vec<&Item> = items.iter().collect();
        ordered.sort_by_key(|i| time_for(i.rid) > 0);
        for item in ordered {
            let content = gtk::Box::builder().spacing(8).build();
            content.append(&dot(item.color, 10));
            content.append(
                &gtk::Label::builder()
                    .label(&item.title)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(24)
                    .xalign(0.0)
                    .build(),
            );
            let t = time_for(item.rid);
            if t > 0 {
                content.append(&label(&duration(t), &["dim-label", "caption"]));
            }
            let chip = gtk::Button::builder().child(&content).css_classes(["chip"]).tooltip_text(&item.goal).build();
            if brush == Brush::Row(item.rid) {
                chip.add_css_class("chip-active");
            }
            chip.connect_clicked({
                let (ui, rid) = (ui.clone(), item.rid);
                move |_| {
                    let next = if ui.brush.get() == Brush::Row(rid) { Brush::None } else { Brush::Row(rid) };
                    ui.brush.set(next);
                    later_refresh(&ui);
                }
            });
            chips.insert(&chip, -1);
        }
        let eraser = gtk::Button::builder()
            .child(&adw::ButtonContent::builder().icon_name("edit-clear-symbolic").label("Eraser").build())
            .css_classes(["chip"])
            .build();
        if brush == Brush::Eraser {
            eraser.add_css_class("chip-active");
        }
        eraser.connect_clicked({
            let ui = ui.clone();
            move |_| {
                let next = if ui.brush.get() == Brush::Eraser { Brush::None } else { Brush::Eraser };
                ui.brush.set(next);
                later_refresh(&ui);
            }
        });
        chips.insert(&eraser, -1);
        panel.append(&chips);
    }

    // The schedule as a list, with remove buttons.
    if !blocks.is_empty() {
        panel.append(&label("Schedule", &["heading", "day-plan-subtitle"]));
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list", "schedule-list"])
            .build();
        for b in &blocks {
            list.append(&schedule_row(ui, date, b, items));
        }
        panel.append(&list);
    }
    panel.upcast()
}

fn schedule_row(ui: &Rc<Ui>, date: NaiveDate, b: &Block, items: &[Item]) -> gtk::Box {
    let item = items.iter().find(|i| i.rid == b.row);
    let row = gtk::Box::builder().spacing(10).css_classes(["schedule-row"]).build();
    row.append(&dot(item.map_or(PALETTE[9], |i| i.color), 10));
    row.append(&gtk::Label::builder().label(b.label()).css_classes(["numeric", "schedule-time"]).build());
    let title = item.map_or_else(|| ui.store.borrow().row(b.row).map(|(_, r)| first_line(&r.task)).unwrap_or_default(), |i| i.title.clone());
    row.append(
        &gtk::Label::builder()
            .label(title)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
    );
    let remove = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Remove from the plan")
        .css_classes(["flat", "circular"])
        .build();
    remove.connect_clicked({
        let (ui, b) = (ui.clone(), *b);
        move |_| {
            {
                let mut store = ui.store.borrow_mut();
                for s in b.start..b.start + b.len {
                    store.set_slot(date, s, None);
                }
            }
            ui.save_soon();
            later_refresh(&ui);
        }
    });
    row.append(&remove);
    row
}

pub fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// Rebuild the Today page after the current signal handler returns (the
/// handler's own widget is part of what gets rebuilt).
fn later_refresh(ui: &Rc<Ui>) {
    let ui = ui.clone();
    glib::idle_add_local_once(move || ui.refresh_today());
}

// ---------------------------------------------------------------- Clock

#[derive(Clone, Copy)]
enum Mode {
    Paint(u64),
    /// Clear only this row's slots, or (None) any slot.
    Erase(Option<u64>),
}

struct Geometry {
    cx: f64,
    cy: f64,
    outer: f64,
    inner: f64,
}

impl Geometry {
    fn of(w: i32, h: i32) -> Self {
        let size = w.min(h) as f64;
        let outer = size / 2.0 - 30.0;
        Geometry { cx: w as f64 / 2.0, cy: h as f64 / 2.0, outer, inner: outer * 0.6 }
    }

    /// Slot under a point; `ring_only` requires the point to be on the ring.
    fn slot_at(&self, x: f64, y: f64, ring_only: bool) -> Option<u8> {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let r = dx.hypot(dy);
        if ring_only && !(self.inner - 4.0..=self.outer + 10.0).contains(&r) {
            return None;
        }
        // 0 at the top (midnight), clockwise.
        let deg = (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0);
        Some(((deg / 360.0 * SLOTS as f64) as u8).min(SLOTS - 1))
    }
}

fn slot_angle(slot: f64) -> f64 {
    -PI / 2.0 + slot / SLOTS as f64 * 2.0 * PI
}

struct ClockState {
    date: NaiveDate,
    slots: RefCell<BTreeMap<u8, u64>>,
    colors: Vec<(u64, Rgb, String)>,
    hover: Cell<Option<u8>>,
    mode: Cell<Option<Mode>>,
    last: Cell<Option<u8>>,
    start: Cell<(f64, f64)>,
}

impl ClockState {
    fn color_of(&self, rid: u64) -> Rgb {
        self.colors.iter().find(|c| c.0 == rid).map_or(PALETTE[9], |c| c.1)
    }

    fn title_of(&self, rid: u64) -> &str {
        self.colors.iter().find(|c| c.0 == rid).map_or("", |c| c.2.as_str())
    }
}

fn clock(ui: &Rc<Ui>, date: NaiveDate, items: &[Item]) -> gtk::DrawingArea {
    let state = Rc::new(ClockState {
        date,
        slots: RefCell::new(ui.store.borrow().slots(date)),
        colors: items.iter().map(|i| (i.rid, i.color, i.title.clone())).collect(),
        hover: Cell::new(None),
        mode: Cell::new(None),
        last: Cell::new(None),
        start: Cell::new((0.0, 0.0)),
    });
    let is_today = date == ui.today.get();

    let area = gtk::DrawingArea::builder()
        .content_width(340)
        .content_height(340)
        .halign(gtk::Align::Center)
        .css_classes(["clock"])
        .build();
    area.set_cursor_from_name(Some("pointer"));
    area.set_draw_func({
        let state = state.clone();
        move |area, cr, w, h| draw(area, cr, w, h, &state, is_today)
    });

    // Paint / erase by clicking or dragging around the ring.
    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin({
        let (ui, state, area) = (ui.clone(), state.clone(), area.clone());
        move |g, x, y| {
            let geo = Geometry::of(area.width(), area.height());
            let Some(slot) = geo.slot_at(x, y, true) else {
                g.set_state(gtk::EventSequenceState::Denied);
                return;
            };
            let current = state.slots.borrow().get(&slot).copied();
            let mode = match ui.brush.get() {
                Brush::Row(rid) if current == Some(rid) => Mode::Erase(Some(rid)),
                Brush::Row(rid) => Mode::Paint(rid),
                Brush::Eraser => Mode::Erase(None),
                Brush::None => {
                    // No brush: clicking a planned slot picks up its task.
                    if let Some(rid) = current {
                        ui.brush.set(Brush::Row(rid));
                    }
                    g.set_state(gtk::EventSequenceState::Denied);
                    later_refresh(&ui);
                    return;
                }
            };
            g.set_state(gtk::EventSequenceState::Claimed);
            state.mode.set(Some(mode));
            state.start.set((x, y));
            state.last.set(Some(slot));
            apply(&ui, &state, slot, mode);
            area.queue_draw();
        }
    });
    drag.connect_drag_update({
        let (ui, state, area) = (ui.clone(), state.clone(), area.clone());
        move |_, dx, dy| {
            let (Some(mode), Some(last)) = (state.mode.get(), state.last.get()) else { return };
            let (x0, y0) = state.start.get();
            let geo = Geometry::of(area.width(), area.height());
            let Some(slot) = geo.slot_at(x0 + dx, y0 + dy, false) else { return };
            // Walk the short way round so fast drags don't skip slots.
            let n = SLOTS as i32;
            let fwd = (slot as i32 - last as i32).rem_euclid(n);
            let step = if fwd <= n / 2 { 1 } else { -1 };
            let mut s = last as i32;
            while s != slot as i32 {
                s = (s + step).rem_euclid(n);
                apply(&ui, &state, s as u8, mode);
            }
            state.last.set(Some(slot));
            state.hover.set(Some(slot));
            area.queue_draw();
        }
    });
    drag.connect_drag_end({
        let (ui, state) = (ui.clone(), state.clone());
        move |_, _, _| {
            if state.mode.take().is_some() {
                ui.save_soon();
                later_refresh(&ui);
            }
        }
    });
    area.add_controller(drag);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion({
        let (state, area) = (state.clone(), area.clone());
        move |_, x, y| {
            let slot = Geometry::of(area.width(), area.height()).slot_at(x, y, true);
            if state.hover.replace(slot) != slot {
                area.queue_draw();
            }
        }
    });
    motion.connect_leave({
        let (state, area) = (state.clone(), area.clone());
        move |_| {
            state.hover.set(None);
            area.queue_draw();
        }
    });
    area.add_controller(motion);

    // Keep the "now" hand moving.
    if is_today {
        let weak = area.downgrade();
        glib::timeout_add_seconds_local(60, move || match weak.upgrade() {
            Some(a) => {
                a.queue_draw();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }
    area
}

fn apply(ui: &Rc<Ui>, state: &ClockState, slot: u8, mode: Mode) {
    let current = state.slots.borrow().get(&slot).copied();
    let new = match mode {
        Mode::Paint(rid) => Some(rid),
        Mode::Erase(Some(rid)) if current == Some(rid) => None,
        Mode::Erase(Some(_)) => return,
        Mode::Erase(None) => None,
    };
    if new == current {
        return;
    }
    match new {
        Some(rid) => state.slots.borrow_mut().insert(slot, rid),
        None => state.slots.borrow_mut().remove(&slot),
    };
    ui.store.borrow_mut().set_slot(state.date, slot, new);
}

fn draw(area: &gtk::DrawingArea, cr: &gtk::cairo::Context, w: i32, h: i32, state: &ClockState, is_today: bool) {
    let fg = area.color();
    let (fr, fgc, fb) = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
    let geo = Geometry::of(w, h);
    let (cx, cy) = (geo.cx, geo.cy);
    let slots = state.slots.borrow();
    let hover = state.hover.get();
    let gap = 0.012;

    for s in 0..SLOTS {
        let (a0, a1) = (slot_angle(s as f64) + gap, slot_angle(s as f64 + 1.0) - gap);
        cr.new_path();
        cr.arc(cx, cy, geo.outer, a0, a1);
        cr.arc_negative(cx, cy, geo.inner, a1, a0);
        cr.close_path();
        let night = !(12..44).contains(&s); // before 6:00 and after 22:00
        match slots.get(&s) {
            Some(rid) => {
                let (r, g, b) = state.color_of(*rid);
                cr.set_source_rgba(r, g, b, if hover == Some(s) { 1.0 } else { 0.88 });
            }
            None => {
                let base = if night { 0.045 } else { 0.085 };
                cr.set_source_rgba(fr, fgc, fb, if hover == Some(s) { base + 0.12 } else { base });
            }
        }
        let _ = cr.fill();
    }

    // Greyscale text: sub-pixel smoothing gives coloured fringes on a canvas.
    if let Ok(mut fo) = gtk::cairo::FontOptions::new() {
        fo.set_antialias(gtk::cairo::Antialias::Gray);
        cr.set_font_options(&fo);
    }

    // Hour ticks and labels.
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
    cr.set_font_size(11.0);
    for hour in 0..24 {
        let a = slot_angle(hour as f64 * 2.0);
        let (c, s) = (a.cos(), a.sin());
        let len = if hour % 6 == 0 { 8.0 } else { 4.0 };
        cr.set_source_rgba(fr, fgc, fb, if hour % 6 == 0 { 0.55 } else { 0.3 });
        cr.set_line_width(1.2);
        cr.move_to(cx + c * (geo.outer + 3.0), cy + s * (geo.outer + 3.0));
        cr.line_to(cx + c * (geo.outer + 3.0 + len), cy + s * (geo.outer + 3.0 + len));
        let _ = cr.stroke();
        if hour % 3 == 0 {
            let text = format!("{hour:02}");
            if let Ok(ext) = cr.text_extents(&text) {
                let r = geo.outer + 20.0;
                cr.set_source_rgba(fr, fgc, fb, 0.6);
                cr.move_to(cx + c * r - ext.width() / 2.0 - ext.x_bearing(), cy + s * r + ext.height() / 2.0);
                let _ = cr.show_text(&text);
            }
        }
    }

    // Current time.
    if is_today {
        let now = chrono::Local::now();
        let slot = (now.hour() * 60 + now.minute()) as f64 / 30.0;
        let a = slot_angle(slot);
        cr.set_source_rgb(0.90, 0.28, 0.30);
        cr.set_line_width(2.0);
        cr.move_to(cx + a.cos() * (geo.inner - 8.0), cy + a.sin() * (geo.inner - 8.0));
        cr.line_to(cx + a.cos() * (geo.outer + 4.0), cy + a.sin() * (geo.outer + 4.0));
        let _ = cr.stroke();
        cr.arc(cx + a.cos() * (geo.inner - 8.0), cy + a.sin() * (geo.inner - 8.0), 3.0, 0.0, 2.0 * PI);
        let _ = cr.fill();
    }

    // Centre: the hovered slot, else the total.
    let (big, small) = match hover {
        Some(s) => {
            let what = slots.get(&s).map(|rid| ellipsize(state.title_of(*rid), 22)).unwrap_or_else(|| "Free".into());
            (format!("{}–{}", slot_time(s), slot_time(s + 1)), what)
        }
        None => (duration(slots.len()), "planned".to_string()),
    };
    centered_text(cr, cx, cy - 6.0, &big, 18.0, (fr, fgc, fb, 0.9), true);
    centered_text(cr, cx, cy + 16.0, &small, 11.5, (fr, fgc, fb, 0.6), false);
}

fn centered_text(cr: &gtk::cairo::Context, x: f64, y: f64, text: &str, size: f64, rgba: (f64, f64, f64, f64), bold: bool) {
    let weight = if bold { gtk::cairo::FontWeight::Bold } else { gtk::cairo::FontWeight::Normal };
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, weight);
    cr.set_font_size(size);
    if let Ok(ext) = cr.text_extents(text) {
        cr.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
        cr.move_to(x - ext.width() / 2.0 - ext.x_bearing(), y + ext.height() / 2.0);
        let _ = cr.show_text(text);
    }
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
