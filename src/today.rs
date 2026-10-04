//! Home page: everything the long-term plans say to do on one day.

use crate::app::{PlannerApp, date_picker, dot, goal_color};
use crate::model::{Goal, PlanRow, fmt_range};
use chrono::Duration;
use eframe::egui::{self, RichText};

pub fn show(app: &mut PlannerApp, ui: &mut egui::Ui) {
    header(app, ui);
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if app.date == app.today {
            overdue(app, ui);
        }

        let agenda = app.store.agenda(app.date);
        if agenda.is_empty() {
            ui.add_space(24.0);
            ui.label(RichText::new("Nothing planned for this day.").weak());
            ui.label(
                RichText::new("Add rows to a goal's plan, or use the quick add below.").weak(),
            );
        }
        for (gi, ranges, days) in agenda {
            goal_card(ui, &mut app.store.goals[gi], &ranges, &days, app.date);
            ui.add_space(10.0);
        }

        quick_add(app, ui);
    });
}

fn header(app: &mut PlannerApp, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("◀").on_hover_text("Previous day").clicked() {
            app.date -= Duration::days(1);
        }
        if ui.button("Today").clicked() {
            app.date = app.today;
        }
        if ui.button("▶").on_hover_text("Next day").clicked() {
            app.date += Duration::days(1);
        }
        date_picker(ui, &mut app.date, "today_date");
    });

    let title = app.date.format("%A, %-d %B %Y").to_string();
    let rel = match (app.date - app.today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        n if n > 0 => format!("In {n} days"),
        n => format!("{} days ago", -n),
    };
    ui.add_space(4.0);
    ui.label(RichText::new(rel).weak());
    ui.heading(RichText::new(title).size(26.0).strong());

    let (done, total) = app
        .store
        .goals
        .iter()
        .flat_map(|g| &g.rows)
        .filter(|r| r.is_day() && r.covers(app.date))
        .fold((0, 0), |(d, t), r| (d + r.done() as usize, t + 1));
    if total > 0 {
        ui.add(
            egui::ProgressBar::new(done as f32 / total as f32)
                .desired_width(320.0)
                .text(format!("{done} / {total} done")),
        );
    }
    ui.add_space(4.0);
}

fn goal_card(ui: &mut egui::Ui, goal: &mut Goal, ranges: &[usize], days: &[usize], date: chrono::NaiveDate) {
    let color = goal_color(goal.color);
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.6)))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                dot(ui, color, 12.0);
                ui.label(RichText::new(&goal.name).size(18.0).strong());
            });

            for &ri in ranges {
                let row = &goal.rows[ri];
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("📌 {}  ·  {}", row.title(), fmt_range(row.start, row.end)))
                        .color(color)
                        .strong(),
                );
                details(ui, &goal.columns, row);
            }

            if !days.is_empty() {
                ui.add_space(6.0);
            }
            for &ri in days {
                let row = &mut goal.rows[ri];
                todo_line(ui, row, date);
                details(ui, &goal.columns, row);
            }
        });
}

fn todo_line(ui: &mut egui::Ui, row: &mut PlanRow, date: chrono::NaiveDate) {
    let mut done = row.done();
    let mut text = RichText::new(row.title()).size(15.0);
    if done {
        text = text.strikethrough().weak();
    }
    if ui.checkbox(&mut done, text).changed() {
        row.set_done(done, date);
    }
}

/// The non-title columns, as "Column: value" lines under the item.
fn details(ui: &mut egui::Ui, columns: &[String], row: &PlanRow) {
    for (name, value) in columns.iter().zip(&row.cells).skip(1) {
        if value.trim().is_empty() {
            continue;
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_space(26.0);
            ui.label(RichText::new(format!("{name}:")).strong());
            ui.label(RichText::new(value).weak());
        });
    }
}

fn overdue(app: &mut PlannerApp, ui: &mut egui::Ui) {
    let items = app.store.overdue(app.date);
    if items.is_empty() {
        return;
    }
    let today = app.today;
    egui::CollapsingHeader::new(RichText::new(format!("⚠ Carried over ({})", items.len())).strong())
        .default_open(true)
        .show(ui, |ui| {
            for (gi, ri) in items {
                let goal = &mut app.store.goals[gi];
                let color = goal_color(goal.color);
                let name = goal.name.clone();
                let row = &mut goal.rows[ri];
                ui.horizontal(|ui| {
                    todo_line(ui, row, today);
                    ui.label(RichText::new(format!("{name} · {}", fmt_range(row.start, row.end))).small().color(color));
                    if ui.small_button("Move to today").clicked() {
                        row.start = today;
                        row.end = today;
                    }
                });
            }
        });
    ui.add_space(10.0);
}

fn quick_add(app: &mut PlannerApp, ui: &mut egui::Ui) {
    if app.store.goals.is_empty() {
        return;
    }
    ui.add_space(12.0);
    ui.separator();
    ui.label(RichText::new("Quick add for this day").strong());
    app.quick_goal = app.quick_goal.min(app.store.goals.len() - 1);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("quick_goal")
            .selected_text(&app.store.goals[app.quick_goal].name)
            .show_ui(ui, |ui| {
                for (i, g) in app.store.goals.iter().enumerate() {
                    ui.selectable_value(&mut app.quick_goal, i, &g.name);
                }
            });
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.quick_text)
                .hint_text("What needs doing?")
                .desired_width(360.0),
        );
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Add").clicked() || enter) && !app.quick_text.trim().is_empty() {
            let id = app.store.next_id();
            let goal = &mut app.store.goals[app.quick_goal];
            let row = goal.add_row(id, app.date, app.date);
            row.cells[0] = std::mem::take(&mut app.quick_text).trim().to_string();
            goal.sort_rows();
            edit.request_focus();
        }
    });
}
