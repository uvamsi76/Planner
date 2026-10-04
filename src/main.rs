mod cloud;
mod dayplan;
mod goal;
mod import;
mod import_dialog;
mod layout;
mod model;
mod sync;
mod today;
mod ui;

use adw::prelude::*;
use gtk::glib;
use model::{Store, fmt_range};
use std::cell::RefCell;

/// Also the icon name and the .desktop file name, so the dock matches the window.
pub const APP_ID: &str = "dev.vamsi.Planner";

fn main() -> glib::ExitCode {
    let store = match Store::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("planner: cannot read {}: {e}", Store::path().display());
            return glib::ExitCode::FAILURE;
        }
    };

    // `planner today [YYYY-MM-DD]` prints the agenda instead of opening the window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("today") {
        let Ok(date) = args.get(1).map_or(Ok(model::today()), |s| s.parse()) else {
            eprintln!("planner: expected a date like 2026-10-05");
            return glib::ExitCode::FAILURE;
        };
        print_agenda(&store, date);
        return glib::ExitCode::SUCCESS;
    }

    let app = adw::Application::builder().application_id(APP_ID).build();
    let store = RefCell::new(Some(store));
    app.connect_activate(move |app| {
        // Single instance: launching again just raises the window.
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        if let Some(store) = store.take() {
            ui::build(app, store).window.present();
        }
    });
    // Our own args were handled above; don't let GTK parse them.
    app.run_with_args::<&str>(&[])
}

fn print_agenda(store: &Store, date: chrono::NaiveDate) {
    println!("{}", date.format("%A, %-d %B %Y"));
    for (gi, ri) in store.overdue(date) {
        let (g, r) = (&store.goals[gi], &store.goals[gi].rows[ri]);
        println!("  ! [ ] {} ({}, from {})", r.task, g.name, fmt_range(r.start, r.end));
    }
    let agenda = store.agenda(date);
    if agenda.is_empty() {
        println!("  Nothing planned.");
    }
    for (gi, ranges, days) in agenda {
        let g = &store.goals[gi];
        println!("\n{} {}", g.icon, g.name);
        for ri in ranges {
            let r = &g.rows[ri];
            println!("  * {} ({})", r.task, fmt_range(r.start, r.end));
        }
        for ri in days {
            let r = &g.rows[ri];
            println!("  [{}] {}", if r.done() { "x" } else { " " }, r.task);
        }
    }
}
