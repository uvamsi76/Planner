//! Google Drive sync on the GTK side: the database status icon, the sign-in
//! window, and keeping the local file and the Drive copy in step.
//!
//! Network calls (`cloud.rs`) run on worker threads via `gio::spawn_blocking`;
//! results are handled back on the GTK thread.
//!
//! Rules:
//! - Every local save marks the account dirty and uploads ~2 s later.
//! - On startup / sign-in / "Sync now": if Drive's copy changed since we last
//!   saw it, download it; if we also have unsent changes (or this is the first
//!   sign-in on a device that already has data), ask which copy to keep.
//! - Before an upload, if Drive's copy changed underneath us, ask too.
//! - Closing the window with unsent changes hides it, finishes the upload,
//!   then quits.

use crate::cloud::{self, Account, Meta};
use crate::model::Store;
use crate::ui::{Ui, label};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::{Cell, RefCell},
    f64::consts::PI,
    rc::Rc,
    time::Duration,
};

#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    SignedOut,
    Syncing,
    Synced,
    Error(String),
}

pub struct State {
    pub account: RefCell<Account>,
    status: RefCell<Status>,
    buttons: RefCell<Vec<glib::WeakRef<gtk::Button>>>,
    busy: Cell<bool>,
    again: Cell<bool>,
    upload_pending: Cell<bool>,
    closing: Cell<bool>,
    /// Rebuilds the open Google Drive window, if any.
    dialog: RefCell<Option<Rc<dyn Fn()>>>,
}

impl State {
    pub fn new() -> Self {
        let account = Account::load();
        let status = if account.signed_in() { Status::Synced } else { Status::SignedOut };
        State {
            account: RefCell::new(account),
            status: RefCell::new(status),
            buttons: RefCell::default(),
            busy: Cell::new(false),
            again: Cell::new(false),
            upload_pending: Cell::new(false),
            closing: Cell::new(false),
            dialog: RefCell::new(None),
        }
    }
}

fn status(ui: &Ui) -> Status {
    ui.sync.status.borrow().clone()
}

fn set_status(ui: &Rc<Ui>, s: Status) {
    ui.sync.status.replace(s.clone());
    let tip = tooltip(ui, &s);
    ui.sync.buttons.borrow_mut().retain(|w| match w.upgrade() {
        Some(b) => {
            b.set_tooltip_text(Some(&tip));
            if let Some(area) = b.child() {
                area.queue_draw();
            }
            true
        }
        None => false,
    });
    refresh_dialog(ui);
}

fn refresh_dialog(ui: &Ui) {
    let rebuild = ui.sync.dialog.borrow().clone();
    if let Some(rebuild) = rebuild {
        rebuild();
    }
}

fn tooltip(ui: &Ui, s: &Status) -> String {
    match s {
        Status::SignedOut => "Not syncing — connect Google Drive".into(),
        Status::Syncing => "Syncing with Google Drive…".into(),
        Status::Synced => match &ui.sync.account.borrow().email {
            Some(e) => format!("Saved to Google Drive ({e})"),
            None => "Saved to Google Drive".into(),
        },
        Status::Error(e) => format!("Google Drive: {e}"),
    }
}

fn save_account(ui: &Rc<Ui>) {
    if let Err(e) = ui.sync.account.borrow().save() {
        ui.toast(&format!("Couldn't save Google Drive settings: {e}"));
    }
}

fn now_label() -> String {
    chrono::Local::now().to_rfc3339()
}

// ---------------------------------------------------------------- status icon

/// The database icon for header bars: ✕ badge when signed out, ✓ when synced.
pub fn status_button(ui: &Rc<Ui>) -> gtk::Button {
    let area = gtk::DrawingArea::builder().content_width(24).content_height(20).build();
    area.set_draw_func({
        let ui = ui.clone();
        move |a, cr, w, h| draw_icon(a, cr, w, h, &status(&ui))
    });
    let button = gtk::Button::builder().child(&area).css_classes(["flat", "sync-button"]).build();
    button.set_tooltip_text(Some(&tooltip(ui, &status(ui))));
    button.connect_clicked({
        let ui = ui.clone();
        move |_| open_dialog(&ui)
    });
    ui.sync.buttons.borrow_mut().push(button.downgrade());
    button
}

fn draw_icon(area: &gtk::DrawingArea, cr: &gtk::cairo::Context, w: i32, h: i32, s: &Status) {
    let fg = area.color();
    cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.9);
    cr.set_line_width(1.5);
    // Database cylinder.
    let (x, top, cw, ch, ry) = (2.5, 2.5, 13.0, 14.0, 2.6);
    let cx = x + cw / 2.0;
    let ellipse = |cr: &gtk::cairo::Context, y: f64, full: bool| {
        cr.save().ok();
        cr.translate(cx, y);
        cr.scale(cw / 2.0, ry);
        if full { cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI) } else { cr.arc(0.0, 0.0, 1.0, 0.0, PI) }
        cr.restore().ok();
    };
    ellipse(cr, top + ry, true);
    let _ = cr.stroke();
    ellipse(cr, top + ch / 2.0, false);
    let _ = cr.stroke();
    ellipse(cr, top + ch - ry, false);
    let _ = cr.stroke();
    cr.move_to(x, top + ry);
    cr.line_to(x, top + ch - ry);
    cr.move_to(x + cw, top + ry);
    cr.line_to(x + cw, top + ch - ry);
    let _ = cr.stroke();

    // Badge, bottom right.
    let (bx, by, r) = (w as f64 - 6.0, h as f64 - 6.0, 5.5);
    let (red, green, blue) = match s {
        Status::SignedOut => (0.88, 0.29, 0.30),
        Status::Synced => (0.20, 0.68, 0.40),
        Status::Syncing => (0.24, 0.52, 0.95),
        Status::Error(_) => (0.94, 0.58, 0.16),
    };
    cr.set_source_rgb(red, green, blue);
    cr.arc(bx, by, r, 0.0, 2.0 * PI);
    let _ = cr.fill();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.set_line_width(1.5);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    match s {
        Status::SignedOut => {
            let d = 2.2;
            cr.move_to(bx - d, by - d);
            cr.line_to(bx + d, by + d);
            cr.move_to(bx + d, by - d);
            cr.line_to(bx - d, by + d);
        }
        Status::Synced => {
            cr.move_to(bx - 2.6, by + 0.2);
            cr.line_to(bx - 0.6, by + 2.2);
            cr.line_to(bx + 2.8, by - 2.0);
        }
        Status::Syncing => {
            cr.arc(bx, by, 2.6, 0.3, 1.6 * PI);
        }
        Status::Error(_) => {
            cr.move_to(bx, by - 2.8);
            cr.line_to(bx, by + 0.6);
            cr.move_to(bx, by + 2.6);
            cr.line_to(bx, by + 2.7);
        }
    }
    let _ = cr.stroke();
}

// ---------------------------------------------------------------- lifecycle

/// At launch: pull or push if signed in.
pub fn startup(ui: &Rc<Ui>) {
    if ui.sync.account.borrow().signed_in() {
        sync_now(ui);
    }
}

/// Called after every local save.
pub fn local_saved(ui: &Rc<Ui>) {
    if !ui.sync.account.borrow().signed_in() {
        return;
    }
    if !ui.sync.account.borrow().dirty {
        ui.sync.account.borrow_mut().dirty = true;
        save_account(ui);
    }
    if ui.sync.upload_pending.replace(true) {
        return;
    }
    let ui = ui.clone();
    glib::timeout_add_local_once(Duration::from_secs(2), move || {
        ui.sync.upload_pending.set(false);
        upload(&ui, false);
    });
}

/// From the window's close handler. Returns true if closing must wait for an
/// upload (the window is hidden meanwhile and destroyed when it's done).
pub fn hold_close(ui: &Rc<Ui>) -> bool {
    if ui.sync.closing.get() || !ui.sync.account.borrow().signed_in() || !ui.sync.account.borrow().dirty {
        return false;
    }
    ui.sync.closing.set(true);
    ui.window.set_visible(false);
    upload(ui, false);
    true
}

fn finish_close(ui: &Rc<Ui>) {
    if ui.sync.closing.get() {
        ui.window.destroy();
    }
}

enum Outcome {
    UpToDate(Meta),
    Pushed(Meta),
    Pulled(Meta, String),
    /// Drive changed and so did we (or this is a first sign-in with local data).
    Conflict(Meta, String),
}

/// Bring local and Drive in step (see module docs).
pub fn sync_now(ui: &Rc<Ui>) {
    if ui.sync.busy.replace(true) {
        ui.sync.again.set(true);
        return;
    }
    set_status(ui, Status::Syncing);
    let acct = ui.sync.account.borrow().clone();
    let data = ui.store.borrow().to_json();
    let local_has_data = !ui.store.borrow().goals.is_empty();
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || -> cloud::Result<Outcome> {
            let Some(meta) = cloud::find_file(&acct)? else {
                return Ok(Outcome::Pushed(cloud::upload(&acct, &data)?));
            };
            let remote_changed = meta.version != acct.version;
            if !remote_changed {
                return Ok(if acct.dirty { Outcome::Pushed(cloud::upload(&acct, &data)?) } else { Outcome::UpToDate(meta) });
            }
            let remote = cloud::download(&acct, &meta.id)?;
            let first_time = acct.version.is_none();
            if (acct.dirty || (first_time && local_has_data)) && remote != data {
                Ok(Outcome::Conflict(meta, remote))
            } else {
                Ok(Outcome::Pulled(meta, remote))
            }
        })
        .await
        .unwrap_or_else(|_| Err("Sync crashed".into()));
        handle(&ui, result);
    });
}

/// Upload the current data. `force` skips the "did Drive change?" check
/// (used after the user chose to keep this device's copy).
fn upload(ui: &Rc<Ui>, force: bool) {
    if !ui.sync.account.borrow().signed_in() {
        return;
    }
    if ui.sync.busy.replace(true) {
        ui.sync.again.set(true);
        return;
    }
    set_status(ui, Status::Syncing);
    let acct = ui.sync.account.borrow().clone();
    let data = ui.store.borrow().to_json();
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || -> cloud::Result<Outcome> {
            if !force
                && acct.version.is_some()
                && let Some(meta) = cloud::find_file(&acct)?
                && meta.version != acct.version
            {
                let remote = cloud::download(&acct, &meta.id)?;
                return Ok(Outcome::Conflict(meta, remote));
            }
            Ok(Outcome::Pushed(cloud::upload(&acct, &data)?))
        })
        .await
        .unwrap_or_else(|_| Err("Upload crashed".into()));
        handle(&ui, result);
    });
}

fn handle(ui: &Rc<Ui>, result: cloud::Result<Outcome>) {
    ui.sync.busy.set(false);
    match result {
        Ok(Outcome::UpToDate(meta)) | Ok(Outcome::Pushed(meta)) => {
            mark_synced(ui, &meta);
            finish_close(ui);
        }
        Ok(Outcome::Pulled(meta, remote)) => {
            apply_remote(ui, &remote);
            mark_synced(ui, &meta);
        }
        Ok(Outcome::Conflict(meta, remote)) => {
            if ui.sync.closing.get() {
                // Can't ask now; keep the change marked dirty for next launch.
                ui.window.destroy();
                return;
            }
            set_status(ui, Status::Error("Choose which copy to keep".into()));
            ask_conflict(ui, meta, remote);
            return;
        }
        Err(e) => {
            if cloud::is_account_gone(&e) {
                ui.sync.account.borrow_mut().sign_out();
                save_account(ui);
                set_status(ui, Status::SignedOut);
                ui.toast("The Google account was removed from Settings, so syncing stopped.");
            } else {
                // Includes "sign in again in Settings": stay connected and retry later.
                set_status(ui, Status::Error(e));
            }
            if ui.sync.closing.get() {
                ui.window.destroy();
            }
            return;
        }
    }
    if ui.sync.again.take() {
        upload(ui, false);
    }
}

fn mark_synced(ui: &Rc<Ui>, meta: &Meta) {
    {
        let mut acct = ui.sync.account.borrow_mut();
        acct.file_id = Some(meta.id.clone());
        acct.version = meta.version.clone();
        acct.dirty = false;
        acct.last_sync = Some(now_label());
    }
    save_account(ui);
    set_status(ui, Status::Synced);
}

/// Replace local data with Drive's copy (keeping a backup of the local file).
fn apply_remote(ui: &Rc<Ui>, remote: &str) {
    let store = match Store::from_json(remote) {
        Ok(s) => s,
        Err(e) => {
            set_status(ui, Status::Error(e));
            return;
        }
    };
    let backup = Store::path().with_extension("before-drive.json");
    let _ = std::fs::write(&backup, ui.store.borrow().to_json());
    *ui.store.borrow_mut() = store;
    if let Err(e) = ui.store.borrow().save() {
        ui.toast(&format!("Could not save: {e}"));
    }
    ui.rebuild_sidebar();
    ui.show(ui.view.get());
}

fn ask_conflict(ui: &Rc<Ui>, meta: Meta, remote: String) {
    let when = meta
        .modified
        .as_deref()
        .and_then(|m| chrono::DateTime::parse_from_rfc3339(m).ok())
        .map(|t| t.with_timezone(&chrono::Local).format("%a %-d %b, %H:%M").to_string())
        .unwrap_or_else(|| "recently".into());
    let goals = Store::from_json(&remote).map(|s| s.goals.len()).unwrap_or(0);
    let dialog = adw::AlertDialog::new(
        Some("Which plans should Planner keep?"),
        Some(&format!(
            "Google Drive has a different copy (changed {when}, {goals} goal{}) than this computer. \
             The copy you don't keep is saved as a backup file next to your data.",
            if goals == 1 { "" } else { "s" }
        )),
    );
    dialog.add_responses(&[("later", "Decide later"), ("local", "Keep this computer’s"), ("drive", "Use Google Drive’s")]);
    dialog.set_response_appearance("drive", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("drive"));
    dialog.set_close_response("later");
    dialog.connect_response(None, {
        let ui = ui.clone();
        move |_, response| match response {
            "drive" => {
                apply_remote(&ui, &remote);
                mark_synced(&ui, &meta);
            }
            "local" => {
                let backup = Store::path().with_extension("drive-backup.json");
                let _ = std::fs::write(backup, &remote);
                ui.sync.account.borrow_mut().version = meta.version.clone();
                upload(&ui, true);
            }
            _ => set_status(&ui, Status::Error("Not synced yet — choose which copy to keep".into())),
        }
    });
    dialog.present(Some(&ui.window));
}

// ---------------------------------------------------------------- window

pub fn open_dialog(ui: &Rc<Ui>) {
    if ui.window.visible_dialog().is_some() {
        return;
    }
    let dialog = adw::Dialog::builder().title("Google Drive").content_width(460).build();
    let holder = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&holder));
    dialog.set_child(Some(&toolbar));

    let rebuild: Rc<dyn Fn()> = {
        let (ui, holder) = (ui.clone(), holder.clone());
        Rc::new(move || {
            while let Some(c) = holder.first_child() {
                holder.remove(&c);
            }
            holder.append(&dialog_content(&ui));
        })
    };
    rebuild();
    ui.sync.dialog.replace(Some(rebuild));
    dialog.connect_closed({
        let ui = ui.clone();
        move |_| {
            ui.sync.dialog.replace(None);
        }
    });
    dialog.present(Some(&ui.window));
}

fn dialog_content(ui: &Rc<Ui>) -> gtk::Widget {
    let page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(16).css_classes(["sync-page"]).build();
    let acct = ui.sync.account.borrow().clone();
    let icon = status_button(ui);
    icon.set_halign(gtk::Align::Center);
    icon.set_can_target(false);
    icon.add_css_class("sync-hero");
    page.append(&icon);
    let centered = |text: &str, classes: &[&str]| {
        let l = label(text, classes);
        l.set_xalign(0.5);
        l.set_justify(gtk::Justification::Center);
        l
    };

    if acct.signed_in() {
        let who = acct.email.clone().unwrap_or_else(|| "your Google account".into());
        page.append(&centered(&format!("Connected as {who}"), &["title-4"]));
        page.append(&centered(
            &format!("Your goals, plans and day schedules are saved to My Drive › {} › {}.", acct.folder, cloud::FILE_NAME),
            &["dim-label"],
        ));
        let error = match status(ui) {
            Status::Error(e) => Some(e),
            _ => None,
        };
        let state = match (&error, status(ui)) {
            (Some(e), _) => e.clone(),
            (None, Status::Syncing) => "Syncing…".to_string(),
            _ => match acct.last_sync.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
                Some(t) => format!("Last synced {}", t.format("%a %-d %b, %H:%M")),
                None => "Synced".into(),
            },
        };
        page.append(&centered(&state, &["caption", if error.is_some() { "warning" } else { "dim-label" }]));
        if error.as_deref().is_some_and(cloud::needs_sign_in) {
            page.append(&online_accounts_button());
        }

        // Change the folder (the data file there is picked up, or created).
        let group = adw::PreferencesGroup::new();
        let folder_row = adw::EntryRow::builder().title("Folder in My Drive").text(&acct.folder).show_apply_button(true).build();
        folder_row.connect_apply({
            let ui = ui.clone();
            move |row| {
                let name = row.text().trim().to_string();
                if name.is_empty() || name == ui.sync.account.borrow().folder {
                    return;
                }
                ui.sync.account.borrow_mut().set_folder(&name);
                save_account(&ui);
                sync_now(&ui);
            }
        });
        group.add(&folder_row);
        page.append(&group);

        let buttons = gtk::Box::builder().spacing(12).halign(gtk::Align::Center).build();
        let sync = gtk::Button::builder().label("Sync now").css_classes(["pill"]).sensitive(status(ui) != Status::Syncing).build();
        sync.connect_clicked({
            let ui = ui.clone();
            move |_| sync_now(&ui)
        });
        let out = gtk::Button::builder().label("Disconnect").css_classes(["pill", "destructive-action"]).build();
        out.connect_clicked({
            let ui = ui.clone();
            move |_| {
                ui.sync.account.borrow_mut().sign_out();
                save_account(&ui);
                set_status(&ui, Status::SignedOut);
                ui.toast("Disconnected. Your data stays on this computer and in Drive.");
            }
        });
        buttons.append(&sync);
        buttons.append(&out);
        page.append(&buttons);
        return page.upcast();
    }

    page.append(&centered("Store your plans in Google Drive", &["title-4"]));
    page.append(&centered(
        "Planner uses the Google account from Ubuntu Settings, so there's nothing extra to set up. \
         Your data is kept in one folder in My Drive and still saved on this computer for offline use.",
        &["dim-label"],
    ));

    let accounts = cloud::google_accounts();
    if accounts.is_empty() {
        page.append(&centered("No Google account in Settings → Online Accounts yet.", &["caption", "warning"]));
        page.append(&online_accounts_button());
        page.append(&refresh_button(ui));
        return page.upcast();
    }

    let group = adw::PreferencesGroup::new();
    let emails: Vec<&str> = accounts.iter().map(|a| a.email.as_str()).collect();
    let account_row = adw::ComboRow::builder().title("Google account").model(&gtk::StringList::new(&emails)).build();
    let folder_row = adw::EntryRow::builder().title("Folder in My Drive").text(&acct.folder).build();
    group.add(&account_row);
    group.add(&folder_row);
    page.append(&group);
    page.append(&centered(
        "Planner only reads and writes planner-data.json in this folder. It's created if it doesn't exist.",
        &["dim-label", "caption"],
    ));

    let attention = centered(cloud::NEEDS_SIGN_IN, &["caption", "warning"]);
    let fix = online_accounts_button();
    let connect = gtk::Button::builder().label("Use this account").css_classes(["pill", "suggested-action"]).halign(gtk::Align::Center).build();
    let update = {
        let (accounts, account_row, folder_row) = (accounts.clone(), account_row.clone(), folder_row.clone());
        let (attention, fix, connect) = (attention.clone(), fix.clone(), connect.clone());
        move || {
            let needs = accounts.get(account_row.selected() as usize).is_some_and(|a| a.attention_needed);
            attention.set_visible(needs);
            fix.set_visible(needs);
            connect.set_sensitive(!needs && !folder_row.text().trim().is_empty());
        }
    };
    update();
    account_row.connect_selected_notify({
        let update = update.clone();
        move |_| update()
    });
    folder_row.connect_changed(move |_| update());
    connect.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let Some(chosen) = accounts.get(account_row.selected() as usize) else { return };
            {
                let mut acct = ui.sync.account.borrow_mut();
                acct.sign_out();
                acct.goa_id = Some(chosen.id.clone());
                acct.email = Some(chosen.email.clone());
                acct.set_folder(&folder_row.text());
            }
            save_account(&ui);
            sync_now(&ui);
        }
    });
    page.append(&attention);
    page.append(&fix);
    page.append(&connect);
    if let Status::Error(e) = status(ui) {
        page.append(&centered(&e, &["caption", "warning"]));
    }
    page.append(&refresh_button(ui));
    page.upcast()
}

fn online_accounts_button() -> gtk::Button {
    let b = gtk::Button::builder().label("Open Online Accounts…").css_classes(["pill"]).halign(gtk::Align::Center).build();
    b.connect_clicked(|_| cloud::open_online_accounts());
    b
}

/// Re-read Settings after adding or fixing an account there.
fn refresh_button(ui: &Rc<Ui>) -> gtk::Button {
    let b = gtk::Button::builder().label("Refresh").css_classes(["flat"]).halign(gtk::Align::Center).build();
    b.connect_clicked({
        let ui = ui.clone();
        move |_| refresh_dialog(&ui)
    });
    b
}
