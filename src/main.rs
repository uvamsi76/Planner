mod app;
mod model;
mod plan;
mod today;

use eframe::egui;
use model::{Store, fmt_range};

fn main() -> eframe::Result {
    let store = match Store::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("planner: cannot read {}: {e}", Store::path().display());
            std::process::exit(1);
        }
    };

    // `planner today [YYYY-MM-DD]` prints the agenda instead of opening the window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("today") {
        let date = match args.get(1) {
            Some(s) => s.parse().unwrap_or_else(|_| {
                eprintln!("planner: expected a date like 2026-10-05");
                std::process::exit(2);
            }),
            None => model::today(),
        };
        print_agenda(&store, date);
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Planner")
            .with_app_id("planner")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([700.0, 450.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Planner",
        options,
        Box::new(|cc| Ok(Box::new(app::PlannerApp::new(cc, store)))),
    )
}

fn print_agenda(store: &Store, date: chrono::NaiveDate) {
    println!("{}", date.format("%A, %-d %B %Y"));
    for (gi, ri) in store.overdue(date) {
        let (g, r) = (&store.goals[gi], &store.goals[gi].rows[ri]);
        println!("  ! [ ] {} ({}, from {})", r.title(), g.name, fmt_range(r.start, r.end));
    }
    let agenda = store.agenda(date);
    if agenda.is_empty() {
        println!("  Nothing planned.");
    }
    for (gi, ranges, days) in agenda {
        let g = &store.goals[gi];
        println!("\n{}", g.name);
        for ri in ranges {
            let r = &g.rows[ri];
            println!("  * {} ({})", r.title(), fmt_range(r.start, r.end));
        }
        for ri in days {
            let r = &g.rows[ri];
            println!("  [{}] {}", if r.done() { "x" } else { " " }, r.title());
        }
    }
}
