//! Custom layouts:
//! - `TableLine`: one row of the goal table. Every line on a page shares one
//!   `Widths`, so header and rows always line up, and resizing a column
//!   relayouts them all. Columns are proportional, so the table uses
//!   whatever width the window gives it.
//! - `table_header`: the header line with draggable column borders.
//! - `Split`: two columns side by side when wide, stacked when narrow.

use gtk::{glib, prelude::*, subclass::prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const INSERT_W: i32 = 28;
const CHECK_W: i32 = 30;
const DELETE_W: i32 = 36;
const GAP: i32 = 6;
const MIN_DATE: i32 = 64;
const MIN_TEXT: i32 = 90;
pub const DEFAULT_DATE: i32 = 120;
pub const DEFAULT_RATIO: f64 = 0.5;

/// Column sizes shared by every line of one table.
pub struct Widths {
    pub date: Cell<i32>,
    /// Task's share of the space left for Task + Notes.
    pub ratio: Cell<f64>,
    lines: RefCell<Vec<glib::WeakRef<gtk::Widget>>>,
}

impl Widths {
    pub fn new(date: i32, ratio: f64) -> Rc<Self> {
        Rc::new(Widths { date: Cell::new(date), ratio: Cell::new(ratio), lines: RefCell::default() })
    }

    /// (x, width) of [insert, check, date, task, notes, delete] for a line `width` wide.
    pub fn columns(&self, width: i32) -> [(i32, i32); 6] {
        let fixed = INSERT_W + CHECK_W + DELETE_W + GAP * 5;
        let date = self.date.get().clamp(MIN_DATE, (width - fixed - 2 * MIN_TEXT).max(MIN_DATE));
        let text = (width - fixed - date).max(2 * MIN_TEXT);
        let task = ((text as f64 * self.ratio.get()).round() as i32).clamp(MIN_TEXT, text - MIN_TEXT);
        let sizes = [INSERT_W, CHECK_W, date, task, text - task, DELETE_W];
        let mut x = 0;
        sizes.map(|w| {
            let col = (x, w);
            x += w + GAP;
            col
        })
    }

    fn min_width() -> i32 {
        INSERT_W + CHECK_W + DELETE_W + GAP * 5 + MIN_DATE + 2 * MIN_TEXT
    }

    fn register(&self, w: &impl IsA<gtk::Widget>) {
        self.lines.borrow_mut().push(w.upcast_ref::<gtk::Widget>().downgrade());
    }

    /// Re-measure every line after a width change (row heights depend on wrapping).
    pub fn changed(&self) {
        self.lines.borrow_mut().retain(|w| match w.upgrade() {
            Some(w) => {
                w.queue_resize();
                true
            }
            None => false,
        });
    }
}

// ---------------------------------------------------------------- TableLine

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TableLayout {
        pub widths: RefCell<Option<Rc<Widths>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TableLayout {
        const NAME: &'static str = "PlannerTableLayout";
        type Type = super::TableLayout;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for TableLayout {}

    impl LayoutManagerImpl for TableLayout {
        fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, widget: &gtk::Widget, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                let min = Widths::min_width();
                return (min, min, -1, -1);
            }
            let widths = self.widths.borrow().clone().expect("widths");
            let cols = widths.columns(if for_size < 0 { 900 } else { for_size });
            let (mut min, mut nat) = (0, 0);
            for (child, (_, w)) in children(widget).zip(cols) {
                if child.should_layout() {
                    let (cmin, cnat, _, _) = child.measure(gtk::Orientation::Vertical, w);
                    min = min.max(cmin);
                    nat = nat.max(cnat);
                }
            }
            (min, nat, -1, -1)
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, _baseline: i32) {
            let widths = self.widths.borrow().clone().expect("widths");
            for (child, (x, w)) in children(widget).zip(widths.columns(width)) {
                if child.should_layout() {
                    child.size_allocate(&gtk::Allocation::new(x, 0, w, height), -1);
                }
            }
        }
    }

    #[derive(Default)]
    pub struct TableLine;

    #[glib::object_subclass]
    impl ObjectSubclass for TableLine {
        const NAME: &'static str = "PlannerTableLine";
        type Type = super::TableLine;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for TableLine {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for TableLine {}
}

fn children(w: &gtk::Widget) -> impl Iterator<Item = gtk::Widget> {
    std::iter::successors(w.first_child(), |c| c.next_sibling())
}

glib::wrapper! {
    pub struct TableLayout(ObjectSubclass<imp::TableLayout>) @extends gtk::LayoutManager;
}

glib::wrapper! {
    pub struct TableLine(ObjectSubclass<imp::TableLine>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TableLine {
    /// A line with exactly six cells: insert, check, date, task, notes, delete.
    pub fn new(widths: &Rc<Widths>, cells: [&gtk::Widget; 6]) -> Self {
        let line: Self = glib::Object::builder().property("hexpand", true).build();
        let layout: TableLayout = glib::Object::new();
        layout.imp().widths.replace(Some(widths.clone()));
        line.set_layout_manager(Some(layout));
        for c in cells {
            c.set_parent(&line);
        }
        widths.register(&line);
        line
    }
}

// ---------------------------------------------------------------- Header

#[derive(Clone, Copy)]
enum Border {
    DateTask,
    TaskNotes,
}

/// Header line with draggable borders after Date and after Task.
/// `on_done(date_width, task_ratio)` runs when a drag ends.
pub fn table_header(widths: &Rc<Widths>, cells: [&gtk::Widget; 6], on_done: impl Fn(i32, f64) + 'static) -> gtk::Widget {
    let line = TableLine::new(widths, cells);
    let overlay = gtk::Overlay::builder().child(&line).css_classes(["table-header"]).build();

    let handles = [Border::DateTask, Border::TaskNotes].map(|b| {
        let h = gtk::Box::builder().css_classes(["col-resize"]).build();
        h.set_cursor_from_name(Some("col-resize"));
        h.set_tooltip_text(Some("Drag to resize"));
        overlay.add_overlay(&h);
        (b, h)
    });
    let border_x = {
        let widths = widths.clone();
        move |b: Border, width: i32| {
            let cols = widths.columns(width);
            match b {
                Border::DateTask => cols[3].0 - GAP / 2,
                Border::TaskNotes => cols[4].0 - GAP / 2,
            }
        }
    };
    overlay.connect_get_child_position({
        let (handles, border_x) = (handles.clone(), border_x.clone());
        move |o, child| {
            let (b, _) = handles.iter().find(|(_, h)| h.upcast_ref::<gtk::Widget>() == child)?;
            Some(gtk::gdk::Rectangle::new(border_x(*b, o.width()) - 5, 0, 10, o.height()))
        }
    });

    // The drag lives on the (static) header, not on the moving handle.
    let drag = gtk::GestureDrag::new();
    let active: Rc<Cell<Option<(Border, i32, i32, i32)>>> = Rc::default();
    drag.connect_drag_begin({
        let (widths, active, overlay) = (widths.clone(), active.clone(), overlay.clone());
        move |g, x, _| {
            let width = overlay.width();
            let near = |b| (x - border_x(b, width) as f64).abs() <= 8.0;
            let border = [Border::DateTask, Border::TaskNotes].into_iter().find(|b| near(*b));
            match border {
                Some(b) => {
                    let cols = widths.columns(width);
                    active.set(Some((b, cols[2].1, cols[3].1, cols[3].1 + cols[4].1)));
                    g.set_state(gtk::EventSequenceState::Claimed);
                }
                None => {
                    g.set_state(gtk::EventSequenceState::Denied);
                }
            }
        }
    });
    drag.connect_drag_update({
        let (widths, active, overlay) = (widths.clone(), active.clone(), overlay.clone());
        move |_, dx, _| {
            let Some((b, date, task, text)) = active.get() else { return };
            match b {
                Border::DateTask => widths.date.set(date + dx as i32),
                Border::TaskNotes => {
                    let task = (task as f64 + dx).clamp(MIN_TEXT as f64, (text - MIN_TEXT) as f64);
                    widths.ratio.set(task / text as f64);
                }
            }
            // Clamp the stored date width to what's actually shown.
            widths.date.set(widths.columns(overlay.width())[2].1);
            widths.changed();
            overlay.queue_allocate();
        }
    });
    drag.connect_drag_end({
        let (widths, active) = (widths.clone(), active.clone());
        move |_, _, _| {
            if active.take().is_some() {
                on_done(widths.date.get(), widths.ratio.get());
            }
        }
    });
    overlay.add_controller(drag);
    overlay.upcast()
}

// ---------------------------------------------------------------- Split

mod split_imp {
    use super::*;

    pub struct SplitLayout {
        pub side: Cell<i32>,
        pub threshold: Cell<i32>,
        pub gap: Cell<i32>,
    }

    impl Default for SplitLayout {
        fn default() -> Self {
            SplitLayout { side: Cell::new(380), threshold: Cell::new(940), gap: Cell::new(40) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SplitLayout {
        const NAME: &'static str = "PlannerSplitLayout";
        type Type = super::SplitLayout;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for SplitLayout {}

    impl SplitLayout {
        /// Widths of (main, side) and whether they sit side by side.
        fn arrange(&self, width: i32) -> (i32, i32, bool) {
            if width >= self.threshold.get() {
                (width - self.side.get() - self.gap.get(), self.side.get(), true)
            } else {
                (width, width, false)
            }
        }
    }

    impl LayoutManagerImpl for SplitLayout {
        fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, widget: &gtk::Widget, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let kids: Vec<_> = children(widget).filter(|c| c.should_layout()).collect();
            if orientation == gtk::Orientation::Horizontal {
                let min = kids.iter().map(|c| c.measure(orientation, -1).0).max().unwrap_or(0);
                let nat = kids.iter().map(|c| c.measure(orientation, -1).1).sum::<i32>() + self.gap.get();
                return (min, nat.max(min), -1, -1);
            }
            let (main_w, side_w, wide) = self.arrange(if for_size < 0 { self.threshold.get() } else { for_size });
            let hs: Vec<(i32, i32)> = kids
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let (min, nat, _, _) = c.measure(orientation, if i == 0 { main_w } else { side_w });
                    (min, nat)
                })
                .collect();
            if wide {
                let min = hs.iter().map(|h| h.0).max().unwrap_or(0);
                let nat = hs.iter().map(|h| h.1).max().unwrap_or(0);
                (min, nat, -1, -1)
            } else {
                let gap = self.gap.get() * (hs.len() as i32 - 1).max(0);
                (hs.iter().map(|h| h.0).sum::<i32>() + gap, hs.iter().map(|h| h.1).sum::<i32>() + gap, -1, -1)
            }
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, _baseline: i32) {
            let (main_w, side_w, wide) = self.arrange(width);
            let kids: Vec<_> = children(widget).filter(|c| c.should_layout()).collect();
            let mut y = 0;
            for (i, c) in kids.iter().enumerate() {
                if wide {
                    let x = if i == 0 { 0 } else { main_w + self.gap.get() };
                    let w = if i == 0 { main_w } else { side_w };
                    c.size_allocate(&gtk::Allocation::new(x, 0, w, height), -1);
                } else {
                    let h = c.measure(gtk::Orientation::Vertical, width).1;
                    c.size_allocate(&gtk::Allocation::new(0, y, width, h), -1);
                    y += h + self.gap.get();
                }
            }
        }
    }

    #[derive(Default)]
    pub struct Split;

    #[glib::object_subclass]
    impl ObjectSubclass for Split {
        const NAME: &'static str = "PlannerSplit";
        type Type = super::Split;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Split {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Split {}
}

glib::wrapper! {
    pub struct SplitLayout(ObjectSubclass<split_imp::SplitLayout>) @extends gtk::LayoutManager;
}

glib::wrapper! {
    pub struct Split(ObjectSubclass<split_imp::Split>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Split {
    /// `main` takes the remaining width; `side` is `side_width` wide. Below
    /// `threshold` px they stack (main on top).
    pub fn new(main: &impl IsA<gtk::Widget>, side: &impl IsA<gtk::Widget>, side_width: i32, threshold: i32) -> Self {
        let split: Self = glib::Object::builder().property("hexpand", true).build();
        let layout: SplitLayout = glib::Object::new();
        layout.imp().side.set(side_width);
        layout.imp().threshold.set(threshold);
        split.set_layout_manager(Some(layout));
        main.set_parent(&split);
        side.set_parent(&split);
        split
    }
}
