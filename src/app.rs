use crate::model::{self, Store};
use chrono::NaiveDate;
use eframe::egui::{self, Color32, RichText};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Today,
    Goal(u64),
}

pub struct PlannerApp {
    pub store: Store,
    pub view: View,
    /// The day shown on the Today page.
    pub date: NaiveDate,
    /// Real calendar day as of the last frame, to detect midnight rollover.
    pub today: NaiveDate,
    pub quick_text: String,
    pub quick_goal: usize,
    pub hide_past: bool,
    pub confirm_delete_goal: Option<u64>,
    pub new_column: String,
    last_saved: String,
    pub error: Option<String>,
}

pub fn goal_color(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// A filled colour dot (the default font has no reliable "●" glyph).
pub fn dot(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
}

impl PlannerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store) -> Self {
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        cc.egui_ctx.set_global_style(style);

        let last_saved = serde_json::to_string_pretty(&store).unwrap_or_default();
        let today = model::today();
        Self {
            store,
            view: View::Today,
            date: today,
            today,
            quick_text: String::new(),
            quick_goal: 0,
            hide_past: false,
            confirm_delete_goal: None,
            new_column: String::new(),
            last_saved,
            error: None,
        }
    }

    /// Persist whenever the data differs from what is on disk.
    fn autosave(&mut self) {
        let Ok(json) = serde_json::to_string_pretty(&self.store) else { return };
        if json != self.last_saved {
            match self.store.save_json(&json) {
                Ok(()) => {
                    self.last_saved = json;
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("Could not save: {e}")),
            }
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Planner");
        ui.add_space(8.0);
        let label = format!("Today · {}", self.today.format("%a %-d %b"));
        if ui.selectable_label(self.view == View::Today, label).clicked() {
            self.view = View::Today;
            self.date = self.today;
        }
        ui.separator();
        ui.label(RichText::new("GOALS").small().weak());
        for g in &self.store.goals {
            let count = g.rows.iter().filter(|r| r.covers(self.today) && r.is_day()).count();
            ui.horizontal(|ui| {
                dot(ui, goal_color(g.color), 9.0);
                let mut text = g.name.clone();
                if count > 0 {
                    text += &format!("  ({count})");
                }
                if ui.selectable_label(self.view == View::Goal(g.id), text).clicked() {
                    self.view = View::Goal(g.id);
                }
            });
        }
        ui.add_space(4.0);
        if ui.button("+ New goal").clicked() {
            let id = self.store.new_goal("New goal");
            self.view = View::Goal(id);
        }
        if let Some(err) = &self.error {
            ui.add_space(12.0);
            ui.colored_label(Color32::from_rgb(230, 80, 80), err);
        }
    }
}

impl eframe::App for PlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Follow the calendar if the app stays open past midnight.
        let now = model::today();
        if now != self.today {
            if self.date == self.today {
                self.date = now;
            }
            self.today = now;
        }
        ui.ctx().request_repaint_after(Duration::from_secs(60));

        egui::Panel::left("nav")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Today => crate::today::show(self, ui),
            View::Goal(id) => crate::plan::show(self, ui, id),
        });

        self.autosave();
    }
}

/// egui_extras' date picker works on `jiff` dates; the model uses `chrono`.
pub fn date_picker(ui: &mut egui::Ui, date: &mut NaiveDate, salt: &str) -> egui::Response {
    use chrono::Datelike;
    let original = jiff::civil::date(date.year() as i16, date.month() as i8, date.day() as i8);
    let mut picked = original;
    let response = ui.add(
        egui_extras::DatePickerButton::new(&mut picked)
            .id_salt(salt)
            .format("%a %-d %b %Y")
            .calendar_week(false),
    );
    if picked != original {
        *date = NaiveDate::from_ymd_opt(picked.year().into(), picked.month() as u32, picked.day() as u32)
            .unwrap_or(*date);
    }
    response
}
