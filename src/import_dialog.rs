//! "Import a plan" dialog: paste a table, preview what was understood, then
//! import it into a new or existing goal. Parsing lives in `import.rs`.

use crate::import::{self, ImportedRow, Parsed};
use crate::model::fmt_range;
use crate::ui::{Ui, View, display_name, label};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, rc::Rc};

const PREVIEW_LIMIT: usize = 200;

/// Open the dialog. `target` preselects an existing goal; otherwise "New goal".
pub fn open(ui: &Rc<Ui>, target: Option<u64>) {
    if ui.window.visible_dialog().is_some() {
        return;
    }
    let today = ui.today.get();
    let dialog = adw::Dialog::builder()
        .title("Import a plan")
        .content_width(780)
        .content_height(760)
        .build();

    let import_btn = gtk::Button::builder()
        .label("Import")
        .css_classes(["suggested-action"])
        .sensitive(false)
        .build();
    let header = adw::HeaderBar::new();
    header.pack_end(&import_btn);

    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .css_classes(["import-body"])
        .build();

    // 1. Paste area.
    body.append(&label(
        "Copy a table in Notion (select it, then Ctrl+C) and paste it here. Markdown and \
         spreadsheet tables work too. It needs a date column like “5 Oct”, “12–18 Oct” or \
         “26 Oct – 1 Nov”; a Week column and any extra columns are picked up as well.",
        &["dim-label"],
    ));
    let text = gtk::TextView::builder()
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(10)
        .bottom_margin(10)
        .left_margin(12)
        .right_margin(12)
        .css_classes(["import-text"])
        .build();
    let text_scroll = gtk::ScrolledWindow::builder()
        .height_request(170)
        .child(&text)
        .css_classes(["card"])
        .build();
    let paste = gtk::Button::builder()
        .child(&adw::ButtonContent::builder().icon_name("edit-paste-symbolic").label("Paste from clipboard").build())
        .halign(gtk::Align::Start)
        .build();
    let clear = gtk::Button::builder().label("Clear").css_classes(["flat"]).build();
    let paste_row = gtk::Box::builder().spacing(8).build();
    paste_row.append(&paste);
    paste_row.append(&clear);
    let paste_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
    paste_box.append(&text_scroll);
    paste_box.append(&paste_row);
    body.append(&paste_box);

    // 2. Destination.
    let (names, target_index) = {
        let store = ui.store.borrow();
        let mut names = vec!["New goal".to_string()];
        names.extend(store.goals.iter().map(|g| format!("{} {}", g.icon, display_name(&g.name))));
        let idx = target.and_then(|id| store.goals.iter().position(|g| g.id == id)).map_or(0, |i| i + 1);
        (names, idx as u32)
    };
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let target_row = adw::ComboRow::builder()
        .title("Import into")
        .model(&gtk::StringList::new(&name_refs))
        .selected(target_index)
        .build();
    let name_row = adw::EntryRow::builder().title("New goal name").text("Imported plan").build();
    let replace_row = adw::SwitchRow::builder()
        .title("Replace existing rows")
        .subtitle("Off: rows are added and exact duplicates are skipped")
        .build();
    let sync_target = {
        let (name_row, replace_row) = (name_row.clone(), replace_row.clone());
        move |row: &adw::ComboRow| {
            let new = row.selected() == 0;
            name_row.set_visible(new);
            replace_row.set_visible(!new);
        }
    };
    sync_target(&target_row);
    target_row.connect_selected_notify(sync_target);
    let group = adw::PreferencesGroup::new();
    group.add(&target_row);
    group.add(&name_row);
    group.add(&replace_row);
    body.append(&group);

    // 3. Preview of what was understood.
    let summary = label("", &["heading"]);
    let columns = label("", &["dim-label", "caption"]);
    let skipped = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).build();
    let preview = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let preview_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
    preview_box.append(&summary);
    preview_box.append(&columns);
    preview_box.append(&skipped);
    preview_box.append(&preview);
    body.append(&preview_box);

    let parsed: Rc<RefCell<Parsed>> = Rc::default();
    let refresh = {
        let (parsed, import_btn) = (parsed.clone(), import_btn.clone());
        move |buffer: &gtk::TextBuffer| {
            let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            let p = import::parse(&source, today);
            show_preview(&p, &source, &summary, &columns, &skipped, &preview);
            import_btn.set_sensitive(!p.rows.is_empty());
            parsed.replace(p);
        }
    };
    refresh(&text.buffer());
    text.buffer().connect_changed(refresh);

    let clipboard = ui.window.clipboard();
    paste.connect_clicked({
        let (clipboard, buffer) = (clipboard.clone(), text.buffer());
        move |_| read_clipboard(clipboard.clone(), buffer.clone(), today, false)
    });
    clear.connect_clicked({
        let buffer = text.buffer();
        move |_| buffer.set_text("")
    });
    // Opening the dialog picks up a copied table straight away.
    read_clipboard(clipboard, text.buffer(), today, true);

    import_btn.connect_clicked({
        let (ui, dialog) = (ui.clone(), dialog.clone());
        move |_| {
            let name = name_row.text().trim().to_string();
            do_import(&ui, &parsed.borrow().rows, target_row.selected(), name, replace_row.is_active());
            dialog.close();
        }
    });

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&body)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(&ui.window));
    text.grab_focus();
}

fn show_preview(
    p: &Parsed,
    source: &str,
    summary: &gtk::Label,
    columns: &gtk::Label,
    skipped: &gtk::Box,
    preview: &gtk::ListBox,
) {
    preview.remove_all();
    while let Some(c) = skipped.first_child() {
        skipped.remove(&c);
    }

    let ranges = p.rows.iter().filter(|r| !r.is_day()).count();
    let days = p.rows.len() - ranges;
    summary.set_text(&if p.rows.is_empty() {
        if source.trim().is_empty() { "Nothing pasted yet".into() } else { "No plan rows found".into() }
    } else {
        format!("{ranges} focus range{} · {days} daily todo{}", plural(ranges), plural(days))
    });
    let mut cols = Vec::new();
    if let Some(d) = p.date_column.as_deref().filter(|d| !d.is_empty()) {
        cols.push(format!("Dates from “{d}”"));
    }
    if let Some(t) = p.task_column.as_deref().filter(|t| !t.is_empty()) {
        cols.push(format!("tasks from “{t}”"));
    }
    columns.set_text(&cols.join(" · "));
    columns.set_visible(!cols.is_empty() && !p.rows.is_empty());

    if !source.trim().is_empty() && p.rows.is_empty() && p.skipped.is_empty() {
        skipped.append(&label("That doesn’t look like a table. Paste a Notion table, a Markdown table, or tab-separated text.", &["warning"]));
    }
    for reason in &p.skipped {
        skipped.append(&label(&format!("Skipped · {reason}"), &["warning", "caption"]));
    }

    for r in p.rows.iter().take(PREVIEW_LIMIT) {
        preview.append(&preview_row(r));
    }
    if p.rows.len() > PREVIEW_LIMIT {
        preview.append(&label(&format!("…and {} more", p.rows.len() - PREVIEW_LIMIT), &["dim-label", "preview-more"]));
    }
    preview.set_visible(!p.rows.is_empty());
}

fn preview_row(r: &ImportedRow) -> gtk::Box {
    let row = gtk::Box::builder().spacing(14).css_classes(["preview-row"]).build();
    let date = gtk::Label::builder()
        .label(fmt_range(r.start, r.end))
        .xalign(0.0)
        .width_chars(14)
        .valign(gtk::Align::Start)
        .css_classes(if r.is_day() { vec!["dim-label"] } else { vec!["accent"] })
        .build();
    let text = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).hexpand(true).build();
    let task = label(if r.task.is_empty() { "Untitled" } else { &r.task }, &[]);
    if !r.is_day() {
        task.add_css_class("heading");
    }
    text.append(&task);
    if !r.notes.is_empty() {
        text.append(&label(&r.notes, &["dim-label", "caption"]));
    }
    row.append(&date);
    row.append(&text);
    row
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Read both plain text and HTML from the clipboard and keep whichever parses
/// best (see `import::best_source`). With `only_if_table`, leave the box alone
/// unless something parses, so opening the dialog doesn't dump an unrelated
/// clipboard into it.
fn read_clipboard(clipboard: gtk::gdk::Clipboard, buffer: gtk::TextBuffer, today: chrono::NaiveDate, only_if_table: bool) {
    glib::spawn_future_local(async move {
        let mut candidates = Vec::new();
        if let Ok(Some(text)) = clipboard.read_text_future().await {
            candidates.push(text.to_string());
        }
        if clipboard.formats().contain_mime_type("text/html")
            && let Some(html) = read_html(&clipboard).await
        {
            let table = import::html_table(&html);
            if table.iter().any(|r| r.len() >= 2) {
                candidates.push(import::to_tsv(&table));
            }
        }
        match import::best_source(&candidates, today) {
            Some(text) => buffer.set_text(&text),
            // Nothing table-like: show the raw text only if the user asked to paste.
            None if !only_if_table => {
                if let Some(text) = candidates.first() {
                    buffer.set_text(text);
                }
            }
            None => {}
        }
    });
}

async fn read_html(clipboard: &gtk::gdk::Clipboard) -> Option<String> {
    let (stream, _) = clipboard.read_future(&["text/html"], glib::Priority::DEFAULT).await.ok()?;
    let out = gio::MemoryOutputStream::new_resizable();
    out.splice_future(
        &stream,
        gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
        glib::Priority::DEFAULT,
    )
    .await
    .ok()?;
    let bytes = out.steal_as_bytes();
    // Some apps (Firefox) put UTF-16 HTML on the clipboard.
    Some(match bytes.strip_prefix(&[0xFF, 0xFE]) {
        Some(utf16) => String::from_utf16_lossy(
            &utf16.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
        ),
        None => String::from_utf8_lossy(&bytes).into_owned(),
    })
}

/// `target_index` 0 = new goal, otherwise goal index + 1.
fn do_import(ui: &Rc<Ui>, rows: &[ImportedRow], target_index: u32, name: String, replace: bool) {
    let (gid, previous, added, dupes) = {
        let mut store = ui.store.borrow_mut();
        let (gid, previous) = if target_index == 0 {
            let id = store.new_goal(if name.is_empty() { "Imported plan" } else { &name });
            store.goal_mut(id).unwrap().icon = "📋".into();
            (id, None)
        } else {
            let Some(g) = store.goals.get(target_index as usize - 1) else { return };
            (g.id, Some(g.rows.clone()))
        };
        if replace {
            store.goal_mut(gid).unwrap().rows.clear();
        }
        let (mut added, mut dupes) = (0, 0);
        for r in rows {
            let id = store.next_id();
            let g = store.goal_mut(gid).unwrap();
            if g.rows.iter().any(|x| x.start == r.start && x.end == r.end && x.task == r.task) {
                dupes += 1;
                continue;
            }
            let row = g.add_row(id, r.start, r.end);
            row.task = r.task.clone();
            row.notes = r.notes.clone();
            added += 1;
        }
        store.goal_mut(gid).unwrap().sort_rows();
        (gid, previous, added, dupes)
    };
    ui.save_soon();
    ui.rebuild_sidebar();
    ui.show(View::Goal(gid));

    let mut msg = format!("Imported {added} row{}", plural(added));
    if dupes > 0 {
        msg += &format!(" · {dupes} duplicate{} skipped", plural(dupes));
    }
    let ui2 = ui.clone();
    ui.toast_undo(&msg, move || {
        {
            let mut store = ui2.store.borrow_mut();
            match &previous {
                None => store.goals.retain(|g| g.id != gid),
                Some(rows) => {
                    if let Some(g) = store.goal_mut(gid) {
                        g.rows = rows.clone();
                    }
                }
            }
        }
        ui2.save_soon();
        ui2.rebuild_sidebar();
        ui2.show(if previous.is_none() { View::Today } else { View::Goal(gid) });
    });
}
