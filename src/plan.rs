//! Long-term plan table for one goal (Notion-style).

use crate::app::{PlannerApp, View, date_picker, goal_color};
use chrono::Duration;
use eframe::egui::{self, RichText};

const CELL_WIDTH: f32 = 260.0;

pub fn show(app: &mut PlannerApp, ui: &mut egui::Ui, goal_id: u64) {
    let Some(gi) = app.store.goals.iter().position(|g| g.id == goal_id) else {
        app.view = View::Today;
        return;
    };
    let today = app.today;

    // Header: name, colour, delete.
    ui.add_space(6.0);
    let mut delete_goal = false;
    ui.horizontal(|ui| {
        let goal = &mut app.store.goals[gi];
        ui.color_edit_button_srgb(&mut goal.color);
        ui.add(
            egui::TextEdit::singleline(&mut goal.name)
                .font(egui::TextStyle::Heading)
                .desired_width(400.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.confirm_delete_goal == Some(goal_id) {
                if ui.button("Cancel").clicked() {
                    app.confirm_delete_goal = None;
                }
                if ui.button(RichText::new("Really delete?").color(egui::Color32::RED)).clicked() {
                    delete_goal = true;
                }
            } else if ui.button("🗑 Delete goal").clicked() {
                app.confirm_delete_goal = Some(goal_id);
            }
        });
    });
    if delete_goal {
        app.store.goals.remove(gi);
        app.confirm_delete_goal = None;
        app.view = View::Today;
        return;
    }

    // Toolbar.
    ui.horizontal(|ui| {
        let goal = &app.store.goals[gi];
        let next_day = goal.next_free_date(false, today);
        let next_range = goal.next_free_date(true, today);
        if ui.button("+ Day").on_hover_text(format!("Add a todo for {}", next_day.format("%-d %b"))).clicked() {
            add_rows(app, gi, next_day, 1, 0);
        }
        if ui.button("+ 7 days").on_hover_text("Add a row for each of the next 7 days").clicked() {
            add_rows(app, gi, next_day, 7, 0);
        }
        if ui.button("+ Week range").on_hover_text("Add a focus row spanning 7 days").clicked() {
            add_rows(app, gi, next_range, 1, 6);
        }
        ui.separator();
        ui.checkbox(&mut app.hide_past, "Hide past rows");
        if ui.button("Sort by date").clicked() {
            app.store.goals[gi].sort_rows();
        }
    });

    egui::CollapsingHeader::new("Table columns").show(ui, |ui| {
        let goal = &mut app.store.goals[gi];
        let mut remove = None;
        for (ci, name) in goal.columns.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                if ci == 0 {
                    ui.label(RichText::new("(title — shown as the checkbox text)").small().weak());
                } else if ui.small_button("Remove").clicked() {
                    remove = Some(ci);
                }
            });
        }
        if let Some(ci) = remove {
            goal.remove_column(ci);
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.new_column).hint_text("New column name").desired_width(200.0));
            if ui.button("Add column").clicked() && !app.new_column.trim().is_empty() {
                goal.add_column(std::mem::take(&mut app.new_column).trim().to_string());
            }
        });
    });
    ui.separator();

    // The table itself.
    let goal = &mut app.store.goals[gi];
    let color = goal_color(goal.color);
    let mut delete_row = None;
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        egui::Grid::new(("plan", goal_id))
            .striped(true)
            .num_columns(goal.columns.len() + 4)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new("Done").strong());
                ui.label(RichText::new("Dates").strong());
                for c in &goal.columns {
                    ui.label(RichText::new(c).strong());
                }
                ui.end_row();

                for (ri, row) in goal.rows.iter_mut().enumerate() {
                    if app.hide_past && row.end < today {
                        continue;
                    }
                    // Marker for rows active today; ranges vs days.
                    let tint = if row.covers(today) { color } else { ui.visuals().weak_text_color() };
                    let size = if row.is_day() { egui::vec2(8.0, 8.0) } else { egui::vec2(8.0, 22.0) };
                    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, tint);
                    resp.on_hover_text(match (row.is_day(), row.covers(today)) {
                        (true, true) => "Daily todo — today",
                        (true, false) => "Daily todo",
                        (false, true) => "Range / focus — active today",
                        (false, false) => "Range / focus",
                    });

                    let mut done = row.done();
                    if ui.checkbox(&mut done, "").changed() {
                        row.set_done(done, today);
                    }

                    ui.vertical(|ui| {
                        let (s, e) = (format!("s{}", row.id), format!("e{}", row.id));
                        date_picker(ui, &mut row.start, &s);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("to").small().weak());
                            date_picker(ui, &mut row.end, &e);
                        });
                        if row.end < row.start {
                            row.end = row.start;
                        }
                    });

                    for cell in &mut row.cells {
                        ui.scope(|ui| {
                            ui.set_min_width(CELL_WIDTH);
                            ui.add(
                                egui::TextEdit::multiline(cell)
                                    .desired_rows(1)
                                    .desired_width(CELL_WIDTH),
                            );
                        });
                    }

                    if ui.small_button("🗑").on_hover_text("Delete row").clicked() {
                        delete_row = Some(ri);
                    }
                    ui.end_row();
                }
            });
        if goal.rows.is_empty() {
            ui.add_space(16.0);
            ui.label(RichText::new("No rows yet — use “+ Day”, “+ 7 days” or “+ Week range” above.").weak());
        }
    });
    if let Some(ri) = delete_row {
        goal.rows.remove(ri);
    }
}

/// Adds `count` consecutive rows starting at `start`, each spanning `extra_days` + 1 days.
fn add_rows(app: &mut PlannerApp, gi: usize, start: chrono::NaiveDate, count: i64, extra_days: i64) {
    for i in 0..count {
        let id = app.store.next_id();
        let s = start + Duration::days(i * (extra_days + 1));
        app.store.goals[gi].add_row(id, s, s + Duration::days(extra_days));
    }
    app.store.goals[gi].sort_rows();
}
