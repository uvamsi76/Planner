//! Window shell: sidebar, page navigation, saving, toasts. Pages live in
//! `today.rs` and `goal.rs`; they read and write `Ui::store` directly.

use crate::model::{self, Store};
use adw::prelude::*;
use chrono::{Datelike, NaiveDate};
use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Today,
    Goal(u64),
}

/// What a page builder hands back to the shell.
pub struct Page {
    pub title: String,
    pub header: adw::HeaderBar,
    pub body: gtk::Widget,
}

struct SideRow {
    row: gtk::ListBoxRow,
    icon: gtk::Label,
    name: gtk::Label,
    count: gtk::Label,
}

/// The hidden "add a todo" bar on the Today page and its header toggle.
pub struct QuickAdd {
    pub revealer: gtk::Revealer,
    pub entry: gtk::Entry,
}

pub struct Ui {
    pub store: RefCell<Store>,
    pub window: adw::ApplicationWindow,
    split: adw::NavigationSplitView,
    content: adw::NavigationPage,
    toasts: adw::ToastOverlay,
    today_list: gtk::ListBox,
    today_count: gtk::Label,
    goal_list: gtk::ListBox,
    side_rows: RefCell<HashMap<u64, SideRow>>,
    /// Container on the Today page whose child is rebuilt on date changes.
    today_holder: RefCell<Option<gtk::Box>>,
    pub view: Cell<View>,
    /// Day shown on the Today page.
    pub date: Cell<NaiveDate>,
    /// Real calendar day; checked periodically to follow midnight.
    pub today: Cell<NaiveDate>,
    pub hide_past: Cell<bool>,
    pub quick_goal: Cell<u32>,
    /// Whether the Today page's add-a-todo bar is open.
    pub quick_open: Cell<bool>,
    pub quick: RefCell<Option<QuickAdd>>,
    pub quick_toggle: RefCell<Option<gtk::ToggleButton>>,
    /// What clicking/dragging on the day-plan clock paints.
    pub brush: Cell<crate::dayplan::Brush>,
    /// Google Drive account and sync status.
    pub sync: crate::sync::State,
    save_pending: Cell<bool>,
}

pub fn build(app: &adw::Application, store: Store) -> Rc<Ui> {
    load_css();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Planner")
        .icon_name(crate::APP_ID)
        .default_width(1360)
        .default_height(860)
        .build();

    // Sidebar: "Today", then the goals.
    let today_list = gtk::ListBox::new();
    today_list.add_css_class("navigation-sidebar");
    let today_count = count_label();
    today_list.append(&sidebar_row("📅", &gtk::Label::new(Some("Today")), &today_count));

    let goal_list = gtk::ListBox::new();
    goal_list.add_css_class("navigation-sidebar");

    let side_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    side_box.append(&today_list);
    side_box.append(
        &gtk::Label::builder().label("Goals").xalign(0.0).css_classes(["sidebar-heading"]).build(),
    );
    side_box.append(&goal_list);

    let new_goal = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New goal")
        .build();
    let side_header = adw::HeaderBar::new();
    let import = gtk::Button::builder()
        .icon_name("edit-paste-symbolic")
        .tooltip_text("Import a plan from Notion (Ctrl+V)")
        .build();
    side_header.pack_end(&new_goal);
    side_header.pack_end(&import);
    let side_view = adw::ToolbarView::new();
    side_view.add_top_bar(&side_header);
    side_view.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&side_box)
            .build(),
    ));
    let sidebar = adw::NavigationPage::builder().title("Planner").child(&side_view).build();

    let content = adw::NavigationPage::builder().title("Today").build();
    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar)
        .content(&content)
        .min_sidebar_width(220.0)
        .max_sidebar_width(280.0)
        .build();
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&split));
    window.set_content(Some(&toasts));

    // Narrow windows collapse to one pane with back navigation.
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 640sp").unwrap());
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);

    let today = model::today();
    let ui = Rc::new(Ui {
        store: RefCell::new(store),
        window,
        split,
        content,
        toasts,
        today_list,
        today_count,
        goal_list,
        side_rows: RefCell::new(HashMap::new()),
        today_holder: RefCell::new(None),
        view: Cell::new(View::Today),
        date: Cell::new(today),
        today: Cell::new(today),
        hide_past: Cell::new(false),
        quick_goal: Cell::new(0),
        quick_open: Cell::new(false),
        quick: RefCell::new(None),
        quick_toggle: RefCell::new(None),
        brush: Cell::new(crate::dayplan::Brush::None),
        sync: crate::sync::State::new(),
        save_pending: Cell::new(false),
    });

    ui.today_list.connect_row_activated({
        let ui = ui.clone();
        move |_, _| {
            ui.date.set(ui.today.get());
            ui.show(View::Today);
        }
    });
    ui.goal_list.connect_row_activated({
        let ui = ui.clone();
        move |_, row| {
            if let Ok(id) = row.widget_name().parse() {
                ui.show(View::Goal(id));
            }
        }
    });
    new_goal.connect_clicked({
        let ui = ui.clone();
        move |_| {
            let id = ui.store.borrow_mut().new_goal("");
            ui.rebuild_sidebar();
            ui.show(View::Goal(id));
            ui.save_soon();
        }
    });
    import.connect_clicked({
        let ui = ui.clone();
        move |_| ui.open_import()
    });
    // Ctrl+V outside a text field opens the import dialog with the clipboard.
    // Text fields handle Ctrl+V first (bubble phase), so normal pasting still works.
    let shortcuts = gtk::ShortcutController::new();
    shortcuts.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("<Control>v"),
        Some(gtk::CallbackAction::new({
            let ui = ui.clone();
            move |_, _| {
                ui.open_import();
                glib::Propagation::Stop
            }
        })),
    ));
    ui.window.add_controller(shortcuts);

    // Clicking anywhere outside the table cell being edited finishes the edit
    // (like pressing Enter). GTK doesn't move focus on clicks on empty space,
    // so watch clicks in the capture phase and drop focus ourselves. The
    // click itself still goes through (e.g. to start editing another cell).
    let outside = gtk::GestureClick::builder()
        .button(0)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    outside.connect_pressed({
        let window = ui.window.clone();
        move |_, _, x, y| {
            let Some(focus) = gtk::prelude::GtkWindowExt::focus(&window) else { return };
            let Some(editing) = ancestor_with_class(&focus, "cell") else { return };
            let picked = window.pick(x, y, gtk::PickFlags::DEFAULT);
            if !picked.is_some_and(|w| w == editing || w.is_ancestor(&editing)) {
                gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
            }
        }
    });
    ui.window.add_controller(outside);

    ui.window.connect_close_request({
        let ui = ui.clone();
        move |_| {
            ui.save_now();
            // With unsent changes, the window hides and closes once uploaded.
            if crate::sync::hold_close(&ui) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });

    // Follow the calendar if the app stays open past midnight.
    glib::timeout_add_seconds_local(30, {
        let ui = ui.clone();
        move || {
            let now = model::today();
            if now != ui.today.get() {
                let old = ui.today.replace(now);
                if ui.date.get() == old {
                    ui.date.set(now);
                }
                ui.show(ui.view.get());
                ui.update_counts();
            }
            glib::ControlFlow::Continue
        }
    });

    ui.rebuild_sidebar();
    ui.show(View::Today);
    crate::sync::startup(&ui);
    ui
}

impl Ui {
    /// Navigate to `view`, rebuilding its page from the store.
    pub fn show(self: &Rc<Self>, view: View) {
        let page = match view {
            View::Today => crate::today::page(self),
            View::Goal(id) => match crate::goal::page(self, id) {
                Some(page) => page,
                None => return self.show(View::Today),
            },
        };
        self.view.set(view);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&page.header);
        toolbar.set_content(Some(&page.body));
        self.content.set_title(&page.title);
        self.content.set_child(Some(&toolbar));
        self.split.set_show_content(true);

        match view {
            View::Today => {
                self.goal_list.unselect_all();
                self.today_list.select_row(self.today_list.row_at_index(0).as_ref());
            }
            View::Goal(id) => {
                self.today_list.unselect_all();
                let rows = self.side_rows.borrow();
                self.goal_list.select_row(rows.get(&id).map(|r| &r.row));
            }
        }
    }

    pub fn set_today_holder(&self, holder: gtk::Box) {
        self.today_holder.replace(Some(holder));
    }

    /// Rebuild only the Today page body (keeps the header and any open popover).
    pub fn refresh_today(self: &Rc<Self>) {
        let Some(holder) = self.today_holder.borrow().clone() else { return };
        while let Some(child) = holder.first_child() {
            holder.remove(&child);
        }
        holder.append(&crate::today::body(self));
    }

    pub fn set_title(&self, title: &str) {
        self.content.set_title(title);
    }

    pub fn rebuild_sidebar(self: &Rc<Self>) {
        self.goal_list.remove_all();
        let mut rows = self.side_rows.borrow_mut();
        rows.clear();
        let store = self.store.borrow();
        for g in store.goal_order().into_iter().map(|i| &store.goals[i]) {
            let icon = gtk::Label::new(Some(&g.icon));
            let name = gtk::Label::new(Some(display_name(&g.name)));
            let count = count_label();
            let row = sidebar_row_widgets(&icon, &name, &count);
            if g.priority > 0 {
                // Badge goes before the count.
                let badge = priority_badge(g.priority);
                let line = row.child().unwrap();
                badge.insert_before(&line, Some(&count));
            }
            row.set_widget_name(&g.id.to_string());
            self.goal_list.append(&row);
            rows.insert(g.id, SideRow { row, icon, name, count });
        }
        drop(store);
        drop(rows);
        if let View::Goal(id) = self.view.get() {
            let rows = self.side_rows.borrow();
            self.goal_list.select_row(rows.get(&id).map(|r| &r.row));
        }
        self.update_counts();
    }

    pub fn set_sidebar_goal(&self, id: u64, icon: &str, name: &str) {
        if let Some(r) = self.side_rows.borrow().get(&id) {
            r.icon.set_text(icon);
            r.name.set_text(display_name(name));
        }
    }

    /// Remaining (unchecked) todos for today, per goal and in total.
    pub fn update_counts(&self) {
        let today = self.today.get();
        let store = self.store.borrow();
        let mut total = 0;
        let rows = self.side_rows.borrow();
        for g in &store.goals {
            let n = g.rows.iter().filter(|r| r.is_day() && r.covers(today) && !r.done()).count();
            total += n;
            if let Some(r) = rows.get(&g.id) {
                set_count(&r.count, n);
            }
        }
        set_count(&self.today_count, total);
    }

    /// Debounced save, so typing doesn't write the file on every keystroke.
    pub fn save_soon(self: &Rc<Self>) {
        self.update_counts();
        if self.save_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(400), move || {
            ui.save_pending.set(false);
            if ui.save_now() {
                crate::sync::local_saved(&ui);
            }
        });
    }

    /// Write the local file now. Returns false (after a toast) on failure.
    pub fn save_now(&self) -> bool {
        match self.store.borrow().save() {
            Ok(()) => true,
            Err(e) => {
                self.toast(&format!("Could not save: {e}"));
                false
            }
        }
    }

    /// Import into the goal being viewed, or into a new goal from elsewhere.
    pub fn open_import(self: &Rc<Self>) {
        let target = match self.view.get() {
            View::Goal(id) => Some(id),
            View::Today => None,
        };
        crate::import_dialog::open(self, target);
    }

    pub fn toast(&self, title: &str) {
        self.toasts.add_toast(adw::Toast::new(title));
    }

    pub fn toast_undo(&self, title: &str, undo: impl Fn() + 'static) {
        let toast = adw::Toast::builder().title(title).button_label("Undo").timeout(6).build();
        toast.connect_button_clicked(move |_| undo());
        self.toasts.add_toast(toast);
    }
}

fn ancestor_with_class(w: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    let mut cur = Some(w.clone());
    while let Some(c) = cur {
        if c.has_css_class(class) {
            return Some(c);
        }
        cur = c.parent();
    }
    None
}

pub fn display_name(name: &str) -> &str {
    if name.trim().is_empty() { "Untitled" } else { name }
}

/// Page content: centred, at most `max_width` wide, scrollable.
pub fn scroller(child: &impl IsA<gtk::Widget>, max_width: i32) -> gtk::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(max_width)
        .tightening_threshold(max_width * 3 / 4)
        .child(child)
        .build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build()
        .upcast()
}

pub fn to_glib(d: NaiveDate) -> glib::DateTime {
    glib::DateTime::from_local(d.year(), d.month() as i32, d.day() as i32, 12, 0, 0.0).unwrap()
}

pub fn from_glib(dt: &glib::DateTime) -> NaiveDate {
    NaiveDate::from_ymd_opt(dt.year(), dt.month() as u32, dt.day_of_month() as u32).unwrap()
}

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .css_classes(classes)
        .build()
}

/// "P1"… badge, coloured by level (see style.css).
pub fn priority_badge(priority: u8) -> gtk::Label {
    gtk::Label::builder()
        .label(format!("P{priority}"))
        .valign(gtk::Align::Center)
        .css_classes(["prio", &format!("prio-{priority}")])
        .tooltip_text("Priority")
        .build()
}

fn count_label() -> gtk::Label {
    gtk::Label::builder().css_classes(["sidebar-count"]).build()
}

fn set_count(label: &gtk::Label, n: usize) {
    label.set_text(&n.to_string());
    label.set_visible(n > 0);
}

fn sidebar_row(icon: &str, name: &gtk::Label, count: &gtk::Label) -> gtk::ListBoxRow {
    sidebar_row_widgets(&gtk::Label::new(Some(icon)), name, count)
}

fn sidebar_row_widgets(icon: &gtk::Label, name: &gtk::Label, count: &gtk::Label) -> gtk::ListBoxRow {
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    b.append(icon);
    b.append(name);
    b.append(count);
    gtk::ListBoxRow::builder().child(&b).build()
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
