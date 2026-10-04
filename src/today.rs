//! Home page: everything the long-term plans say to do on one day.

use crate::model::fmt_range;
use crate::ui::{Page, Ui, View, from_glib, label, scroller, to_glib};
use adw::prelude::*;
use chrono::{Duration, NaiveDate};
use gtk::glib;
use std::rc::Rc;

pub fn page(ui: &Rc<Ui>) -> Page {
    let header = adw::HeaderBar::new();

    let prev = gtk::Button::builder().icon_name("go-previous-symbolic").tooltip_text("Previous day").build();
    let next = gtk::Button::builder().icon_name("go-next-symbolic").tooltip_text("Next day").build();
    let arrows = gtk::Box::builder().css_classes(["linked"]).build();
    arrows.append(&prev);
    arrows.append(&next);
    let today_btn = gtk::Button::with_label("Today");
    header.pack_start(&arrows);
    header.pack_start(&today_btn);

    let calendar = gtk::Calendar::new();
    calendar.set_date(&to_glib(ui.date.get()));
    let popover = gtk::Popover::builder().child(&calendar).build();
    let cal_btn = gtk::MenuButton::builder()
        .icon_name("x-office-calendar-symbolic")
        .tooltip_text("Pick a date")
        .popover(&popover)
        .build();
    header.pack_end(&cal_btn);

    let go = {
        let ui = ui.clone();
        let calendar = calendar.clone();
        Rc::new(move |date: NaiveDate| {
            ui.date.set(date);
            calendar.set_date(&to_glib(date));
            ui.refresh_today();
        })
    };
    prev.connect_clicked({
        let (ui, go) = (ui.clone(), go.clone());
        move |_| go(ui.date.get() - Duration::days(1))
    });
    next.connect_clicked({
        let (ui, go) = (ui.clone(), go.clone());
        move |_| go(ui.date.get() + Duration::days(1))
    });
    today_btn.connect_clicked({
        let (ui, go) = (ui.clone(), go.clone());
        move |_| go(ui.today.get())
    });
    // Month arrows don't emit day-selected, so this fires only on a real pick.
    calendar.connect_day_selected({
        let ui = ui.clone();
        move |c| {
            let picked = from_glib(&c.date());
            if picked != ui.date.get() {
                ui.date.set(picked);
                ui.refresh_today();
            }
            popover.popdown();
        }
    });

    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    ui.set_today_holder(holder.clone());
    holder.append(&body(ui));
    Page { title: "Today".into(), header, body: scroller(&holder) }
}

/// The scrollable content for `ui.date`.
pub fn body(ui: &Rc<Ui>) -> gtk::Box {
    let date = ui.date.get();
    let today = ui.today.get();
    let page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["page"]).build();

    let rel = match (date - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        n if n > 0 => format!("In {n} days"),
        n => format!("{} days ago", -n),
    };
    page.append(&label(&rel, &["page-eyebrow"]));
    page.append(&label(&date.format("%A, %-d %B").to_string(), &["page-title"]));

    // Progress over this day's todos; updated in place when boxes are ticked.
    let progress = gtk::LevelBar::builder().valign(gtk::Align::Center).width_request(180).build();
    // Plain accent bar: drop the default low/high/full colour thresholds.
    for name in [gtk::LEVEL_BAR_OFFSET_LOW, gtk::LEVEL_BAR_OFFSET_HIGH, gtk::LEVEL_BAR_OFFSET_FULL] {
        progress.remove_offset_value(Some(name));
    }
    let progress_text = gtk::Label::builder().valign(gtk::Align::Center).css_classes(["dim-label", "caption"]).build();
    let progress_row = gtk::Box::builder().spacing(12).css_classes(["progress-row"]).build();
    progress_row.append(&progress);
    progress_row.append(&progress_text);
    page.append(&progress_row);
    let update_progress: Rc<dyn Fn()> = {
        let ui = ui.clone();
        Rc::new(move || {
            let (done, total) = ui.store.borrow().progress(date);
            progress_row.set_visible(total > 0);
            progress.set_value(if total > 0 { done as f64 / total as f64 } else { 0.0 });
            progress_text.set_text(&format!("{done} of {total} done"));
        })
    };
    update_progress();

    let store = ui.store.borrow();

    // Unfinished todos from earlier days, only on the real today.
    let overdue = if date == today { store.overdue(date) } else { Vec::new() };
    if !overdue.is_empty() {
        page.append(&label(&format!("Carried over · {}", overdue.len()), &["section-title", "overdue-title"]));
        for &(gi, ri) in &overdue {
            let (g, r) = (&store.goals[gi], &store.goals[gi].rows[ri]);
            let row = todo_row(ui, g.id, r.id, &r.task, "", r.done(), date, update_progress.clone());
            let meta = label(&format!("{} {} · {}", g.icon, g.name, fmt_range(r.start, r.end)), &["dim-label", "caption"]);
            meta.set_margin_start(32);
            row.append(&meta);
            let move_btn = gtk::Button::builder()
                .label("Move to today")
                .css_classes(["flat", "small-button"])
                .halign(gtk::Align::Start)
                .margin_start(26)
                .build();
            move_btn.connect_clicked({
                let (ui, gid, rid) = (ui.clone(), g.id, r.id);
                move |_| {
                    if let Some(row) = ui.store.borrow_mut().goal_mut(gid).and_then(|g| g.row_mut(rid)) {
                        row.start = today;
                        row.end = today;
                    }
                    ui.save_soon();
                    ui.refresh_today();
                }
            });
            row.append(&move_btn);
            page.append(&row);
        }
    }

    let agenda = store.agenda(date);
    for (gi, ranges, days) in &agenda {
        let g = &store.goals[*gi];
        let heading = gtk::Button::builder().css_classes(["flat", "section-link"]).halign(gtk::Align::Start).build();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hbox.append(&gtk::Label::new(Some(&g.icon)));
        hbox.append(&gtk::Label::new(Some(crate::ui::display_name(&g.name))));
        heading.set_child(Some(&hbox));
        heading.set_tooltip_text(Some("Open plan"));
        heading.connect_clicked({
            let (ui, gid) = (ui.clone(), g.id);
            move |_| ui.show(View::Goal(gid))
        });
        page.append(&heading);

        for &ri in ranges {
            let r = &g.rows[ri];
            let callout = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .css_classes(["callout"])
                .build();
            let title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let pin = label("📌", &[]);
            pin.set_valign(gtk::Align::Start);
            title.append(&pin);
            let t = label(if r.task.is_empty() { "Untitled focus" } else { &r.task }, &["callout-title"]);
            t.set_hexpand(true);
            title.append(&t);
            title.append(
                &gtk::Label::builder()
                    .label(fmt_range(r.start, r.end))
                    .valign(gtk::Align::Start)
                    .css_classes(["dim-label", "caption"])
                    .build(),
            );
            callout.append(&title);
            if !r.notes.trim().is_empty() {
                callout.append(&label(&r.notes, &["callout-notes"]));
            }
            page.append(&callout);
        }
        for &ri in days {
            let r = &g.rows[ri];
            page.append(&todo_row(ui, g.id, r.id, &r.task, &r.notes, r.done(), date, update_progress.clone()));
        }
    }

    if agenda.is_empty() && overdue.is_empty() {
        let empty = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).css_classes(["empty-state"]).build();
        empty.append(&label("Nothing planned for this day.", &["empty-title"]));
        empty.append(&label("Add rows to a goal’s plan, paste a Notion table with Ctrl+V, or jot something down below.", &["dim-label"]));
        page.append(&empty);
    }
    drop(store);

    page.append(&quick_add(ui, date));
    page
}

#[allow(clippy::too_many_arguments)]
fn todo_row(
    ui: &Rc<Ui>,
    gid: u64,
    rid: u64,
    task: &str,
    notes: &str,
    done: bool,
    date: NaiveDate,
    on_change: Rc<dyn Fn()>,
) -> gtk::Box {
    let row = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["todo"]).build();
    let text = gtk::Label::builder()
        .label(if task.is_empty() { "Untitled" } else { task })
        .xalign(0.0)
        .wrap(true)
        .css_classes(["todo-text"])
        .build();
    let check = gtk::CheckButton::builder().child(&text).active(done).build();
    if done {
        row.add_css_class("done");
    }
    check.connect_toggled({
        let (ui, row) = (ui.clone(), row.clone());
        move |c| {
            let active = c.is_active();
            if let Some(r) = ui.store.borrow_mut().goal_mut(gid).and_then(|g| g.row_mut(rid)) {
                r.set_done(active, date);
            }
            if active { row.add_css_class("done") } else { row.remove_css_class("done") }
            on_change();
            ui.save_soon();
        }
    });
    row.append(&check);
    if !notes.trim().is_empty() {
        let n = label(notes, &["todo-notes"]);
        n.set_margin_start(32);
        row.append(&n);
    }
    row
}

fn quick_add(ui: &Rc<Ui>, date: NaiveDate) -> gtk::Box {
    let bar = gtk::Box::builder().spacing(8).css_classes(["quick-add"]).build();
    let store = ui.store.borrow();
    if store.goals.is_empty() {
        let b = gtk::Button::builder().label("Create your first goal").css_classes(["pill", "suggested-action"]).build();
        b.connect_clicked({
            let ui = ui.clone();
            move |_| {
                let id = ui.store.borrow_mut().new_goal("");
                ui.rebuild_sidebar();
                ui.show(View::Goal(id));
                ui.save_soon();
            }
        });
        bar.append(&b);
        return bar;
    }

    let entry = gtk::Entry::builder()
        .placeholder_text(format!("Add a todo for {}…", date.format("%a %-d %b")))
        .hexpand(true)
        .build();
    let names: Vec<String> = store.goals.iter().map(|g| format!("{} {}", g.icon, crate::ui::display_name(&g.name))).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let goal = gtk::DropDown::from_strings(&names);
    goal.set_selected(ui.quick_goal.get().min(names.len() as u32 - 1));
    let add = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("Add").build();
    drop(store);

    let submit = {
        let (ui, entry, goal) = (ui.clone(), entry.clone(), goal.clone());
        move || {
            let text = entry.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            let gi = goal.selected() as usize;
            ui.quick_goal.set(goal.selected());
            {
                let mut store = ui.store.borrow_mut();
                let id = store.next_id();
                let Some(g) = store.goals.get_mut(gi) else { return };
                g.add_row(id, date, date).task = text;
                g.sort_rows();
            }
            ui.save_soon();
            ui.refresh_today();
            refocus_quick_add(&ui);
        }
    };
    entry.connect_activate({
        let submit = submit.clone();
        move |_| submit()
    });
    add.connect_clicked(move |_| submit());

    bar.append(&entry);
    bar.append(&goal);
    bar.append(&add);
    bar
}

/// After a rebuild, put the cursor back in the (new) quick-add entry.
fn refocus_quick_add(ui: &Rc<Ui>) {
    let ui = ui.clone();
    glib::idle_add_local_once(move || {
        let Some(holder) = ui.today_holder_child() else { return };
        if let Some(entry) = find_entry(&holder.upcast()) {
            entry.grab_focus();
        }
    });
}

fn find_entry(w: &gtk::Widget) -> Option<gtk::Entry> {
    if w.has_css_class("quick-add") {
        return w.first_child().and_then(|c| c.downcast().ok());
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(e) = find_entry(&c) {
            return Some(e);
        }
        child = c.next_sibling();
    }
    None
}
