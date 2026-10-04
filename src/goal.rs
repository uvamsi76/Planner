//! A goal's plan: page icon + title, then a plain table of
//! Done · Date · Task · Notes rows.

use crate::layout::{self, TableLine, Widths};
use crate::model::{PlanRow, fmt_range};
use crate::ui::{Page, Ui, View, display_name, from_glib, label, scroller, to_glib};
use adw::prelude::*;
use chrono::{Duration, NaiveDate};
use gtk::glib;
use std::rc::Rc;

/// Column widths shared by the header and every row of the table.
type Columns = Rc<Widths>;

const PRIORITIES: [&str; 5] = ["No priority", "P1 · Highest", "P2 · High", "P3 · Medium", "P4 · Low"];

pub fn page(ui: &Rc<Ui>, gid: u64) -> Option<Page> {
    let today = ui.today.get();
    let (name, icon, priority, cols, rows) = {
        let mut store = ui.store.borrow_mut();
        let g = store.goal_mut(gid)?;
        g.sort_rows();
        let cols = Widths::new(
            g.date_width.unwrap_or(layout::DEFAULT_DATE),
            g.task_ratio.unwrap_or(layout::DEFAULT_RATIO),
        );
        (g.name.clone(), g.icon.clone(), g.priority, cols, g.rows.clone())
    };

    // Header bar: hide-past toggle and a menu with "Delete goal".
    let header = adw::HeaderBar::new();
    let hide_past = gtk::ToggleButton::builder()
        .icon_name("view-conceal-symbolic")
        .tooltip_text("Hide past rows")
        .active(ui.hide_past.get())
        .build();
    hide_past.connect_toggled({
        let ui = ui.clone();
        move |b| {
            ui.hide_past.set(b.is_active());
            ui.show(View::Goal(gid));
        }
    });
    let delete_goal = gtk::Button::builder().label("Delete goal…").css_classes(["flat", "destructive-action"]).build();
    let import_rows = gtk::Button::builder().label("Import rows from Notion…").css_classes(["flat"]).build();
    let menu_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    menu_box.append(&import_rows);
    menu_box.append(&delete_goal);
    let menu = gtk::Popover::builder().child(&menu_box).build();
    import_rows.connect_clicked({
        let (ui, menu) = (ui.clone(), menu.clone());
        move |_| {
            menu.popdown();
            crate::import_dialog::open(&ui, Some(gid));
        }
    });
    header.pack_end(&gtk::MenuButton::builder().icon_name("view-more-symbolic").popover(&menu).build());
    // Google Drive status: database icon with ✕ (signed out) or ✓ (synced).
    header.pack_start(&crate::sync::status_button(ui));
    header.pack_end(&hide_past);
    delete_goal.connect_clicked({
        let ui = ui.clone();
        move |_| {
            menu.popdown();
            confirm_delete_goal(&ui, gid);
        }
    });

    let page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["page"]).build();

    // Notion-style page icon (emoji picker) and big editable title.
    let icon_label = gtk::Label::new(Some(&icon));
    let chooser = gtk::EmojiChooser::new();
    let icon_btn = gtk::MenuButton::builder()
        .child(&icon_label)
        .popover(&chooser)
        .halign(gtk::Align::Start)
        .css_classes(["flat", "page-icon"])
        .tooltip_text("Change icon")
        .build();
    let title = gtk::Entry::builder()
        .text(&name)
        .placeholder_text("Untitled")
        .css_classes(["page-title-entry"])
        .build();
    chooser.connect_emoji_picked({
        let (ui, icon_label, title) = (ui.clone(), icon_label.clone(), title.clone());
        move |_, emoji| {
            if let Some(g) = ui.store.borrow_mut().goal_mut(gid) {
                g.icon = emoji.to_string();
            }
            icon_label.set_text(emoji);
            ui.set_sidebar_goal(gid, emoji, &title.text());
            ui.save_soon();
        }
    });
    title.connect_changed({
        let (ui, icon_label) = (ui.clone(), icon_label.clone());
        move |e| {
            let text = e.text().to_string();
            if let Some(g) = ui.store.borrow_mut().goal_mut(gid) {
                g.name = text.clone();
            }
            ui.set_sidebar_goal(gid, &icon_label.text(), &text);
            ui.set_title(display_name(&text));
            ui.save_soon();
        }
    });
    page.append(&icon_btn);
    page.append(&title);

    // Notion-style property row: Priority.
    let prio = gtk::DropDown::from_strings(&PRIORITIES);
    prio.set_selected(priority.min(4) as u32);
    prio.add_css_class("flat");
    prio.connect_selected_notify({
        let ui = ui.clone();
        move |d| {
            if let Some(g) = ui.store.borrow_mut().goal_mut(gid) {
                g.priority = d.selected() as u8;
            }
            ui.rebuild_sidebar();
            ui.save_soon();
        }
    });
    let props = gtk::Box::builder().spacing(12).css_classes(["page-props"]).build();
    props.append(&label("Priority", &["dim-label", "prop-name"]));
    props.append(&prio);
    page.append(&props);

    page.append(&label(
        "One row per day for daily todos. Give a row a longer span (like a week) for a focus that shows on each of those days.",
        &["dim-label", "page-hint"],
    ));

    // The table.
    page.append(&table_header(ui, gid, &cols));
    let table = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["plan-table"])
        .build();
    table.set_placeholder(Some(&label("No rows yet. Add some below, or copy a table in Notion and press Ctrl+V.", &["dim-label", "table-empty"])));
    for r in &rows {
        if ui.hide_past.get() && r.end < today {
            continue;
        }
        table.append(&row_widget(ui, gid, r, &cols, &table));
    }
    page.append(&table);

    // Notion-style "+ New" buttons under the table.
    let footer = gtk::Box::builder().spacing(4).css_classes(["table-footer"]).build();
    for (text, count, span, ranges) in [
        ("New day", 1, 1, false),
        ("Next 7 days", 7, 1, false),
        ("Week focus", 1, 7, true),
    ] {
        let b = gtk::Button::builder().css_classes(["flat", "add-row"]).build();
        let content = adw::ButtonContent::builder().icon_name("list-add-symbolic").label(text).build();
        b.set_child(Some(&content));
        b.connect_clicked({
            let (ui, cols, table) = (ui.clone(), cols.clone(), table.clone());
            move |_| add_rows(&ui, gid, &cols, &table, count, span, ranges)
        });
        footer.append(&b);
    }
    page.append(&footer);

    // A brand-new goal starts with the cursor in the title.
    if name.is_empty() {
        let title = title.clone();
        glib::idle_add_local_once(move || {
            title.grab_focus();
        });
    }

    Some(Page { title: display_name(&name).to_string(), header, body: scroller(&page, 1800) })
}

fn table_header(ui: &Rc<Ui>, gid: u64, cols: &Columns) -> gtk::Widget {
    let blank = || gtk::Label::new(None).upcast::<gtk::Widget>();
    // Indented to line up with the text inside the date button and cells.
    let (date, task, notes) = (label("Date", &[]), label("Task", &[]), label("Notes", &[]));
    date.set_margin_start(10);
    task.set_margin_start(8);
    notes.set_margin_start(8);
    let ui = ui.clone();
    layout::table_header(
        cols,
        [&blank(), &blank(), date.upcast_ref(), task.upcast_ref(), notes.upcast_ref(), &blank()],
        move |date_width, task_ratio| {
            if let Some(g) = ui.store.borrow_mut().goal_mut(gid) {
                g.date_width = Some(date_width);
                g.task_ratio = Some(task_ratio);
            }
            ui.save_soon();
        },
    )
}

fn row_widget(ui: &Rc<Ui>, gid: u64, r: &PlanRow, cols: &Columns, table: &gtk::ListBox) -> gtk::ListBoxRow {
    let rid = r.id;
    let today = ui.today.get();
    let row = gtk::ListBoxRow::builder().activatable(false).selectable(false).build();
    row.set_widget_name(&rid.to_string());
    let set_classes = {
        let row = row.clone();
        move |r: &PlanRow| {
            for (class, on) in [("range", !r.is_day()), ("current", r.covers(today)), ("done", r.done())] {
                if on { row.add_css_class(class) } else { row.remove_css_class(class) }
            }
        }
    };
    set_classes(r);

    // "+" on week (range) rows: insert a day of that week below it.
    let insert: gtk::Widget = if r.is_day() {
        gtk::Box::new(gtk::Orientation::Horizontal, 0).upcast()
    } else {
        let plus = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add a day of this week below")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "circular", "row-insert"])
            .build();
        plus.connect_clicked({
            let (ui, cols, table) = (ui.clone(), cols.clone(), table.clone());
            move |_| insert_day_in_week(&ui, gid, rid, &cols, &table)
        });
        plus.upcast()
    };

    // Done
    let check = gtk::CheckButton::builder().active(r.done()).valign(gtk::Align::Center).build();
    check.connect_toggled({
        let (ui, set_classes) = (ui.clone(), set_classes.clone());
        move |c| {
            if let Some(r) = ui.store.borrow_mut().goal_mut(gid).and_then(|g| g.row_mut(rid)) {
                r.set_done(c.is_active(), today);
                set_classes(r);
            }
            ui.save_soon();
        }
    });

    // Date: calendar for the start, spin button for how many days it spans.
    // A child (not a label) so GTK doesn't add a dropdown arrow.
    let date_text = gtk::Label::builder()
        .label(fmt_range(r.start, r.end))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let date_btn = gtk::MenuButton::builder()
        .child(&date_text)
        .css_classes(["flat", "date-button"])
        .tooltip_text("Change date or span")
        .build();
    let calendar = gtk::Calendar::new();
    calendar.set_date(&to_glib(r.start));
    let span = gtk::SpinButton::with_range(1.0, 366.0, 1.0);
    span.set_value(r.days() as f64);
    let span_row = gtk::Box::builder().spacing(8).build();
    span_row.append(&label("Lasts", &[]));
    span_row.append(&span);
    span_row.append(&label("day(s)", &["dim-label"]));
    let pop_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).css_classes(["date-popover"]).build();
    pop_box.append(&calendar);
    pop_box.append(&span_row);
    let popover = gtk::Popover::builder().child(&pop_box).build();
    date_btn.set_popover(Some(&popover));

    let apply_dates = {
        let (ui, set_classes) = (ui.clone(), set_classes.clone());
        Rc::new(move |start: Option<NaiveDate>, days: Option<i64>| {
            if let Some(r) = ui.store.borrow_mut().goal_mut(gid).and_then(|g| g.row_mut(rid)) {
                let days = days.unwrap_or(r.days());
                r.start = start.unwrap_or(r.start);
                r.end = r.start + Duration::days(days - 1);
                date_text.set_label(&fmt_range(r.start, r.end));
                set_classes(r);
            }
            ui.save_soon();
        })
    };
    calendar.connect_day_selected({
        let apply = apply_dates.clone();
        move |c| apply(Some(from_glib(&c.date())), None)
    });
    span.connect_value_changed(move |s| apply_dates(None, Some(s.value() as i64)));

    // Task and Notes; their widths come from the shared (resizable) columns.
    let [task, notes] = [(&r.task, "task", true), (&r.notes, "notes", false)].map(|(value, class, is_task)| {
        let ui = ui.clone();
        cell(value, if is_task { "Untitled" } else { "" }, class, move |v| {
            if let Some(r) = ui.store.borrow_mut().goal_mut(gid).and_then(|g| g.row_mut(rid)) {
                if is_task { r.task = v.to_string() } else { r.notes = v.to_string() }
            }
            ui.save_soon();
        })
    });

    // Delete (appears on hover), with undo.
    let del = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Delete row")
        .valign(gtk::Align::Center)
        .css_classes(["flat", "row-delete"])
        .build();
    del.connect_clicked({
        let (ui, row, table) = (ui.clone(), row.clone(), table.clone());
        move |_| {
            let removed = {
                let mut store = ui.store.borrow_mut();
                let Some(g) = store.goal_mut(gid) else { return };
                let Some(i) = g.rows.iter().position(|r| r.id == rid) else { return };
                let removed = (i, g.rows.remove(i));
                store.unschedule_row(rid);
                removed
            };
            table.remove(&row);
            ui.save_soon();
            let label = if removed.1.task.is_empty() { "Row deleted".to_string() } else { format!("Deleted “{}”", removed.1.task) };
            let ui2 = ui.clone();
            ui.toast_undo(&label, move || {
                if let Some(g) = ui2.store.borrow_mut().goal_mut(gid) {
                    let i = removed.0.min(g.rows.len());
                    g.rows.insert(i, removed.1.clone());
                }
                ui2.save_soon();
                if ui2.view.get() == View::Goal(gid) {
                    ui2.show(View::Goal(gid));
                }
            });
        }
    });

    let line = TableLine::new(cols, [&insert, check.upcast_ref(), date_btn.upcast_ref(), task.upcast_ref(), notes.upcast_ref(), del.upcast_ref()]);
    row.set_child(Some(&line));
    row
}

/// Add a day row inside week row `wid` (its first day without a todo, or its
/// last day if all are taken), put it in date order, and start editing it.
fn insert_day_in_week(ui: &Rc<Ui>, gid: u64, wid: u64, cols: &Columns, table: &gtk::ListBox) {
    let (new_row, preceding) = {
        let mut store = ui.store.borrow_mut();
        let id = store.next_id();
        let Some(g) = store.goal_mut(gid) else { return };
        let Some(week) = g.rows.iter().find(|r| r.id == wid).cloned() else { return };
        let taken: Vec<NaiveDate> = g.rows.iter().filter(|r| r.is_day()).map(|r| r.start).collect();
        let date = week
            .start
            .iter_days()
            .take_while(|d| *d <= week.end)
            .find(|d| !taken.contains(d))
            .unwrap_or(week.end);
        let new_row = g.add_row(id, date, date).clone();
        g.sort_rows();
        let pos = g.rows.iter().position(|r| r.id == id).unwrap_or(0);
        let preceding: Vec<String> = g.rows[..pos].iter().rev().map(|r| r.id.to_string()).collect();
        (new_row, preceding)
    };
    ui.save_soon();

    // Insert right after the nearest preceding row that is on screen.
    let mut index = 0;
    'find: for id in &preceding {
        let mut i = 0;
        while let Some(w) = table.row_at_index(i) {
            if w.widget_name() == *id {
                index = i + 1;
                break 'find;
            }
            i += 1;
        }
    }
    let widget = row_widget(ui, gid, &new_row, cols, table);
    table.insert(&widget, index);
    start_editing(widget.upcast());
}

/// Append `count` consecutive rows, each `span` days long, after the last
/// existing day row (or range row), and put the cursor in the first one.
fn add_rows(ui: &Rc<Ui>, gid: u64, cols: &Columns, table: &gtk::ListBox, count: i64, span: i64, ranges: bool) {
    let today = ui.today.get();
    let new_rows: Vec<PlanRow> = {
        let mut store = ui.store.borrow_mut();
        let Some(start) = store.goal(gid).map(|g| g.next_free_date(ranges, today)) else { return };
        (0..count)
            .map(|i| {
                let id = store.next_id();
                let s = start + Duration::days(i * span);
                store.goal_mut(gid).unwrap().add_row(id, s, s + Duration::days(span - 1)).clone()
            })
            .collect()
    };
    let mut first = None;
    for r in &new_rows {
        let w = row_widget(ui, gid, r, cols, table);
        table.append(&w);
        first.get_or_insert(w);
    }
    ui.save_soon();
    if let Some(w) = first {
        start_editing(w.upcast());
    }
}

/// A table cell. Shows a wrapping label; click to edit in place.
/// Enter (or clicking elsewhere, Tab, Esc) finishes; Shift+Enter adds a line.
///
/// While editing, the label stays underneath with opacity 0 and mirrors the
/// text, so it keeps sizing the row (labels do proper height-for-width,
/// text views don't) and the editor grows line by line as you type.
fn cell(value: &str, placeholder: &str, class: &str, on_change: impl Fn(&str) + 'static) -> gtk::Box {
    let cell = gtk::Box::builder().css_classes(["cell", class]).build();
    cell.set_cursor_from_name(Some("text"));
    let label = gtk::Label::builder()
        .xalign(0.0)
        .yalign(0.0)
        .hexpand(true)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();
    let editor = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .accepts_tab(false)
        .top_margin(8)
        .bottom_margin(8)
        .visible(false)
        .css_classes(["cell-editor"])
        .build();
    editor.buffer().set_text(value);
    let overlay = gtk::Overlay::builder().child(&label).hexpand(true).build();
    overlay.add_overlay(&editor);

    let placeholder = placeholder.to_string();
    let finish = {
        let (label, editor) = (label.clone(), editor.clone());
        move || {
            let text = buffer_text(&editor.buffer());
            if text.is_empty() {
                label.set_text(&placeholder);
                label.add_css_class("placeholder");
            } else {
                label.set_text(&text);
                label.remove_css_class("placeholder");
            }
            editor.set_visible(false);
            label.set_opacity(1.0);
        }
    };
    finish();

    editor.buffer().connect_changed({
        let label = label.clone();
        move |b| {
            let text = buffer_text(b);
            on_change(&text);
            // A trailing newline needs a visible character to count as a line.
            label.set_text(&if text.ends_with('\n') { format!("{text}\u{200b}") } else { text });
        }
    });

    // Capture phase: decide about Enter before the text view inserts a newline.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let editor = editor.clone();
        move |_, key, _, mods| {
            use gtk::gdk::Key;
            let enter = matches!(key, Key::Return | Key::KP_Enter | Key::ISO_Enter);
            if (enter && !mods.contains(gtk::gdk::ModifierType::SHIFT_MASK)) || key == Key::Escape {
                if let Some(root) = editor.root() {
                    root.set_focus(None::<&gtk::Widget>);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    editor.add_controller(keys);
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| finish());
    editor.add_controller(focus);

    let click = gtk::GestureClick::new();
    click.connect_released({
        let cell = cell.clone();
        move |_, _, _, _| edit_cell(&cell)
    });
    cell.add_controller(click);

    cell.append(&overlay);
    cell
}

fn buffer_text(b: &gtk::TextBuffer) -> String {
    b.text(&b.start_iter(), &b.end_iter(), false).to_string()
}

/// Switch a `cell()` into editing mode, cursor at the end.
fn edit_cell(cell: &gtk::Box) {
    let Some(overlay) = cell.first_child().and_downcast::<gtk::Overlay>() else { return };
    let (Some(label), Some(editor)) = (
        overlay.child().and_downcast::<gtk::Label>(),
        overlay.last_child().and_downcast::<gtk::TextView>(),
    ) else {
        return;
    };
    if editor.is_visible() {
        return;
    }
    let buffer = editor.buffer();
    label.set_text(&buffer_text(&buffer));
    label.remove_css_class("placeholder");
    label.set_opacity(0.0);
    editor.set_visible(true);
    buffer.place_cursor(&buffer.end_iter());
    editor.grab_focus();
}

/// Once a freshly added row has been laid out: scroll it into view and start
/// editing its Task cell.
fn start_editing(row: gtk::Widget) {
    // Wait for the row's first allocation; before that its position is unknown.
    row.add_tick_callback(|row, _| {
        if row.height() == 0 {
            return glib::ControlFlow::Continue;
        }
        scroll_into_view(row);
        if let Some(cell) = find_task_cell(row) {
            edit_cell(&cell);
        }
        glib::ControlFlow::Break
    });
}

fn scroll_into_view(w: &gtk::Widget) {
    let Some(scroller) = w.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>() else {
        return;
    };
    let Some(content) = scroller.child() else { return };
    let Some(bounds) = w.compute_bounds(&content) else { return };
    let adj = scroller.vadjustment();
    let (top, bottom) = (bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
    let margin = 48.0;
    if top < adj.value() {
        adj.set_value(top - margin);
    } else if bottom > adj.value() + adj.page_size() {
        adj.set_value(bottom - adj.page_size() + margin);
    }
}

fn find_task_cell(w: &gtk::Widget) -> Option<gtk::Box> {
    if w.has_css_class("cell") && w.has_css_class("task") {
        return w.clone().downcast().ok();
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(e) = find_task_cell(&c) {
            return Some(e);
        }
        child = c.next_sibling();
    }
    None
}

fn confirm_delete_goal(ui: &Rc<Ui>, gid: u64) {
    let (name, n) = {
        let store = ui.store.borrow();
        let Some(g) = store.goal(gid) else { return };
        (display_name(&g.name).to_string(), g.rows.len())
    };
    let dialog = adw::AlertDialog::new(
        Some(&format!("Delete “{name}”?")),
        Some(&format!("Its {n} row(s) will be deleted too. This can’t be undone.")),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(Some("delete"), {
        let ui = ui.clone();
        move |_, _| {
            ui.store.borrow_mut().goals.retain(|g| g.id != gid);
            ui.save_soon();
            ui.rebuild_sidebar();
            ui.show(View::Today);
        }
    });
    dialog.present(Some(&ui.window));
}
