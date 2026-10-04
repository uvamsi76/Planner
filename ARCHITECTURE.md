# Planner — Architecture

Native GNOME/Ubuntu desktop app in Rust using **GTK4 + libadwaita** (the
toolkit GNOME's own apps use). One ~600 KB binary, one JSON data file.

## Core idea

Long-term plans are tables of **date-ranged rows**. The Today page is a pure
*query* over those tables ("which rows cover date D?"). Nothing is copied or
generated per day.

```
 Goal "Career — DSA"                      Today page for 5 Oct
 ┌──────────────┬──────────────────┐      ┌───────────────────────────────┐
 │ 5–11 Oct     │ Week 1 — Re-solve│ ───▶ │ 📌 Week 1 — Re-solve… (focus) │
 │ Mon 5 Oct    │ Arrays & Hashing │ ───▶ │ ☐ Arrays & Hashing (7)        │
 │ Tue 6 Oct    │ Arrays + 2 Ptrs  │      └───────────────────────────────┘
 └──────────────┴──────────────────┘
```

- **Day row** (`start == end`) → a checkable todo on that day.
- **Range row** (`start < end`, e.g. a week) → a "📌 focus" callout on every
  day it covers.

## Why GTK4 + libadwaita (and not egui)

The first version used egui. It drew its own light title bar on Wayland,
ignored the system theme, and its text widgets felt non-native. libadwaita
gives:

- the real GNOME header bar, following dark/light mode and accent colour;
- native text entry, fonts, scrolling, animations;
- a tiny binary, because GTK is a shared system library (~600 KB vs ~15 MB).

## Module map

```
src/
├── main.rs     Entry: load Store, `planner today [date]` CLI, else start adw::Application
├── model.rs    Data types, JSON persistence + migration, date queries, sample data (no GUI)
├── ui.rs       Window shell: sidebar, navigation, debounced save, toasts, shared helpers
├── today.rs    Today page: header (◀ ▶ Today, +, calendar), todos by priority, hidden quick add
├── dayplan.rs  Today's "Day plan": 24 h clock of half-hour slots, task chips, schedule list
├── goal.rs     Goal page: icon, title, priority, Done·Date·Task·Notes table, add/delete rows
├── layout.rs   Custom layout managers: resizable table columns, two-column Today split
├── cloud.rs    Google token from GNOME Online Accounts (D-Bus) + Drive v3 calls; blocking
├── sync.rs     Drive sync on the GTK side: status icon, sign-in window, pull/push, conflicts
├── import.rs   Pasted table → plan rows: Markdown/TSV/HTML readers, date parser, column roles (no GUI, unit-tested)
├── import_dialog.rs  "Import a plan" dialog: clipboard, live preview, import with undo
└── style.css   Notion-like styling on top of libadwaita (embedded via include_str!)
tests/fixtures/
├── notion_plan.md                   Real Notion clipboard text (Markdown, multi-line cells)
└── notion_chromium_clipboard.html   Real Notion clipboard HTML (malformed: see Import)
data/
├── dev.vamsi.Planner.svg      App icon
└── dev.vamsi.Planner.desktop  Launcher template (@BINDIR@ filled in by install.sh)
install.sh      Build + per-user install (binary, icon, launcher); no sudo
```

Dependency direction is one-way: `main → ui → {today, dayplan, goal, import_dialog} → {layout, model, import}`.
`model.rs` knows nothing about GTK, so the CLI and future tests reuse it.

## Data model (`model.rs`)

```rust
Store { goals: Vec<Goal>, next_id: u64,
        schedule: BTreeMap<NaiveDate, BTreeMap<u8 /* slot 0..48 */, u64 /* row id */>> }

Goal    { id, name, icon /* emoji */,
          priority: u8,                 // 1 = P1 (highest) … 4 = P4, 0 = none
          date_width: Option<i32>,      // remembered table layout
          task_ratio: Option<f64>,
          rows: Vec<PlanRow> }

PlanRow { id, start: NaiveDate, end: NaiveDate,
          task: String, notes: String,
          completed_on: Option<NaiveDate> }   // None = not done
```

- IDs come from one monotonic counter (`Store::next_id`) shared by goals and
  rows. UI callbacks capture `(goal_id, row_id)`, never indices, so they stay
  valid when rows are added, deleted or re-sorted.
- `start <= end` always holds: the UI edits a row as *start date + "lasts N days"*.
- Completion is per row: a range row checked once is done for the whole range.
- **Schedule:** each day maps half-hour slots (0 = 00:00, 47 = 23:30) to the
  row id doing them. Row ids are globally unique, so no goal id is needed.
  Rows that no longer exist are ignored when reading (`slots`), and deleting a
  row calls `unschedule_row`.

### Queries

| Function | Returns | Used by |
|---|---|---|
| `Store::agenda(date)` | `(goal_idx, range_rows, day_rows)` per goal with anything on `date` | Today page, CLI |
| `Store::overdue(date)` | unfinished day rows with `end < date` | "Carried over", CLI |
| `Store::progress(date)` | `(done, total)` over day rows on `date` | Today progress bar |
| `Goal::next_free_date(ranges, today)` | day after the last day row (or range row), else today | "+ New day / Next 7 days / Week focus" |
| `Goal::sort_rows()` | by `(start, end desc, id)`, so a week precedes its days | each time a goal page opens |
| `Store::goal_order()` | goal indices by priority (P1 first, none last), stable | sidebar, `agenda`, `overdue` |
| `Store::blocks(date)` | the day's slots merged into `Block { start, len, row }` runs | clock list, time pills |
| `Store::set_slot(date, slot, row?)` | assign / clear one slot (empty days are dropped) | clock painting |

## Persistence

- File: `$XDG_DATA_HOME/planner/data.json` (normally `~/.local/share/planner/data.json`).
- **First run:** no file → `Store::sample()` seeds the "Career — DSA" plan.
- **Saving:** every edit calls `Ui::save_soon()`, which debounces (400 ms) and
  then writes. The window's close handler saves immediately.
- **Atomic write:** write `data.json.tmp`, then rename over `data.json`.
- **Migration:** v1 (egui) rows stored text in `cells: [..]`. On load those are
  folded into `task` / `notes`, the original file is copied to
  `data.v1.json`, and the new format is written. Unknown old fields
  (`columns`, `color`) are ignored by serde; missing new ones use
  `#[serde(default)]`.
- Experiment safely with `XDG_DATA_HOME=/tmp/planner-test cargo run`.

## UI layer

### State and ownership

GTK is single-threaded and callback-driven. All state lives in one
`Rc<Ui>` that every signal handler clones:

```rust
Ui {
    store: RefCell<Store>,          // the data; handlers borrow_mut briefly
    view: Cell<View>,               // Today | Goal(id)
    date: Cell<NaiveDate>,          // day shown on the Today page
    today: Cell<NaiveDate>,         // real calendar day (midnight check every 30 s)
    hide_past, quick_goal, …        // small view preferences (not persisted)
    quick_open, quick, quick_toggle // Today's hidden add-a-todo bar + its header "+"
    brush: Cell<Brush>              // what the day-plan clock paints: None | Row(id) | Eraser
    + widget handles: window, split view, content page, sidebar lists, toasts
}
```

Rule: never hold a `store` borrow across a call that might borrow again
(`save_soon`, `show`, `rebuild_sidebar`). Handlers mutate inside a short
block, drop the borrow, then call those.

### Window shell (`ui.rs`)

```
adw::ApplicationWindow
 └─ adw::ToastOverlay                      ← "Deleted … · Undo", save errors
     └─ adw::NavigationSplitView            ← collapses to one pane below 640sp
         ├─ sidebar: HeaderBar(+ New goal) / ListBox "Today" / "Goals" / ListBox goals
         └─ content: NavigationPage ← ToolbarView(page.header, page.body)
```

- `Ui::show(view)` asks `today::page` or `goal::page` for a `Page { title,
  header, body }` and swaps it into the content pane. Navigating always
  rebuilds that page from the store, so it can't show stale data.
- The sidebar keeps handles to each goal's icon, name and count labels, so
  renames and ticked boxes update it in place, without rebuilding.
- `adw::Application` with id `dev.vamsi.Planner` makes the app single-instance:
  launching it again raises the existing window.

### Today page (`today.rs`)

- Header: ◀ ▶ (linked), "Today", **+**, and a calendar popover. Changing the
  date calls `refresh_today()`, which rebuilds only the page body, so the
  header and an open popover survive.
- **+** is a toggle that reveals the add-a-todo bar (`gtk::Revealer`: entry +
  goal dropdown) under the date and focuses it. Esc closes it.
  `ui.quick_open` keeps it open across rebuilds, so you can add several in a
  row.
- Body is a `layout::Split`: todos on the left, `dayplan::panel` on the right
  (360 px). Below 820 px of page width they stack, with the clock under the
  todos.
- Left column: eyebrow ("Tomorrow"), big date title, progress bar;
  "Carried over" (only on the real today) with "Move to today"; per-goal
  sections in **priority order** (the heading shows the P-badge and links to
  the goal page), with 📌 callouts for ranges and checkboxes for days. A
  todo with time on the clock shows a coloured pill ("09:00–10:30").
- Ticking a box updates the store, the row's CSS class (strikethrough) and the
  progress bar in place.

### Day plan (`dayplan.rs`)

- **Items:** everything schedulable that day (carried-over todos, focuses,
  todos) in priority order, each given a palette colour by position. The
  same colour is used on the clock, chips and pills.
- **Clock:** a `gtk::DrawingArea` drawn with cairo. It's a ring of 48
  half-hour slots with midnight at the top, running clockwise. Night hours
  are dimmer. It has hour ticks with labels every 3 h, a red "now" hand
  (redrawn every minute, today only), and a centre read-out: the hovered
  slot's time and task, or the day's total.
- **Painting:** a `GestureDrag` on the clock.
  - The selected brush decides what happens: a task chip paints that task,
    and dragging that starts on one of its own slots erases only that task's
    slots. The eraser clears any slot. With no brush, clicking a planned slot
    picks up its task.
  - Drag updates walk slot by slot the short way round, so fast drags don't
    skip slots.
  - The store updates live. On release the page is rebuilt (in an idle
    callback, since the clock is part of what's rebuilt) to refresh pills,
    chips and the list.
- **Chips:** unscheduled tasks first (in priority order), then scheduled ones
  with their total time, plus an Eraser. The **Schedule** list shows each
  block with a ✕ to remove it.

### Goal page (`goal.rs`)

- Notion-style header: emoji icon (`gtk::EmojiChooser` popover) and a large
  borderless title entry. Header bar: hide-past toggle, ⋮ menu → "Delete
  goal…" (`adw::AlertDialog` confirmation).
- Property row under the title: **Priority** (`gtk::DropDown`). Changing it
  re-sorts the sidebar and the Today page.
- Table: a header plus a `gtk::ListBox`. Each row is a `layout::TableLine`
  with six cells: `[+][check][date][Task][Notes][🗑]`. Every line on the
  page shares one `Rc<Widths>` (Date width + Task share of the text width),
  and a custom `gtk::LayoutManager` (`TableLayout`) places cells from it.
  So the header and all rows always line up. Each `ListBoxRow`'s widget name
  is its row id.
- **Fluid width:** the goal page's clamp allows up to 1800 px, and Task and
  Notes are proportional, so a maximized window gets a wider table.
- **Resizing:** the header (`layout::table_header`) is an overlay with two
  thin handles that light up on hover, placed by `get-child-position` at the
  Date|Task and Task|Notes borders.
  - The `GestureDrag` sits on the header itself, not on the moving handles,
    and starts only within 8 px of a border.
  - Dragging updates `Widths` and calls `Widths::changed()`, which
    `queue_resize`s every registered line, since text re-wraps.
  - On release the widths are saved to `goal.date_width` / `goal.task_ratio`.
- **"+" on week rows** (`insert_day_in_week`): adds a day row for the week's
  first date without a todo (or its last day if all are taken). The row is
  inserted into the `ListBox` right after the nearest preceding row on screen,
  in sorted order, then scrolled into view and opened for editing.
- **Cells** (`cell()`): a wrapping `gtk::Label` for display and a
  `gtk::TextView` for editing, stacked in a `gtk::Overlay`. While editing,
  the label stays underneath with opacity 0 and mirrors the text. It keeps
  sizing the row, since labels do proper height-for-width and text views
  don't, so the editor grows line by line.
  - **Enter** or **Esc** finishes. A capture-phase key controller handles them
    before the text view would insert a newline.
  - **Shift+Enter** inserts a line break.
  - Focus leaving (Tab, or clicking another widget) also finishes.
  - **Clicking anywhere else** finishes too. GTK doesn't move focus on clicks
    on empty space, so a capture-phase `GestureClick` on the window (in
    `ui.rs`) drops focus when a click lands outside the cell being edited.
    The click itself still goes through, so clicking another cell starts
    editing that one.
  - Text is saved on every keystroke (debounced), so finishing is purely
    visual.
- **Date cell:** popover with a `gtk::Calendar` (start) and a "Lasts N days"
  spin button. GTK's `day-selected` fires only on a real click, not when
  paging months, so browsing never moves a row.
- Row CSS classes `range` / `current` / `done` drive the look (tinted
  background, accent date, strikethrough) and are updated in place.
- Adding rows appends widgets, then `start_editing` waits a frame for layout,
  scrolls the row into view and opens its Task cell. Deleting removes
  the widget and shows an Undo toast, which reinserts the row and rebuilds the
  page.

### Styling (`style.css`)

Only spacing, typography and subtle `alpha(currentColor, …)` tints. All
colours come from libadwaita variables (`--accent-color`,
`--accent-bg-color`, `--warning-color`), so light/dark and the Ubuntu accent
colour apply automatically. Emoji are used as icons because they render in
colour everywhere.

## Import from Notion (`import.rs`, `import_dialog.rs`)

You copy a table in Notion and press **Ctrl+V** anywhere outside a text
field. (The sidebar import button and the goal page's ⋮ → "Import rows from
Notion…" do the same.) A dialog opens with the clipboard already read in, and
shows a live preview of what it understood.

**Entry points.** A window-level `gtk::ShortcutController` binds `<Control>v`.
It runs in the bubble phase, so text fields get Ctrl+V first and normal pasting
is unaffected. `Ui::open_import()` targets the goal being viewed, or "New goal"
from the Today page. A second Ctrl+V while the dialog is open is ignored
(`visible_dialog()`).

**Reading the clipboard.** Both `text/plain` and `text/html` are read, each
is parsed, and `import::best_source` keeps whichever yields more rows with
fewer skips. This matters because Notion in Chromium puts a *correct* Markdown
table in text/plain but a *broken* HTML table in text/html: each line of a
multi-line cell becomes its own `<tr>`, and the later columns shift. Both real
payloads are kept in `tests/fixtures/` and covered by a test. An HTML
candidate is converted to TSV (`to_tsv`) so it shows as editable text in the
dialog and re-parses identically.

**Parsing pipeline** (`import::parse` → `plan_from_table`):

1. `read_table` → `Table` (`Vec<Vec<String>>`):
   - Markdown: a row starts at a line beginning with `|` and ends at a line
     ending in `|` once it has as many pipes as the header. Raw newlines inside
     cells (which Notion emits) are therefore kept. `| --- |` rows are dropped,
     `\|` is unescaped, and `<br>` becomes a newline.
   - TSV: an RFC-4180-style reader with `"quoted"` cells that may contain
     tabs, newlines and `""`.
   - HTML (`html_table`): a tiny tag scanner over `<tr>/<td>/<th>`. `<br>`,
     `</p>`, `</div>` and `</li>` become newlines, and entities are decoded.
2. **Header detection:** the first row is a header if none of its cells parses
   as a date.
3. **Column roles:**
   - *date*: a header containing date/day/when, else the column with the most
     parseable cells;
   - *week*: a header containing "week", which becomes the prefix
     "Week 1 — …";
   - *task*: a header matching task/topic/todo/title/…, else the first
     remaining column;
   - every other non-empty column becomes a `"Header: value"` line in notes.
4. **Dates** (`parse_range`): "5 Oct", "Mon 5 Oct", "Oct 5th", "5–11 Oct",
   "5-11 Oct", "Oct 5–11", "26 Oct – 1 Nov", "28 Dec – 3 Jan",
   "October 5, 2026 → October 11, 2026" and ISO `2026-10-05`.
   - A missing month or year is borrowed from the other end of the range.
   - A missing year is picked to land nearest an *anchor*: today for the
     first row, then the previous row's start, so a plan crossing New Year
     keeps counting forward.
5. Blank rows are ignored silently. Rows with text but no readable date are
   listed in the preview as "Skipped · …" rather than guessed.

**Importing** (`do_import`): into a new goal (📋) or an existing one, either
adding rows or replacing them. When adding, rows with the same
`(start, end, task)` are skipped, so pasting the same table twice is harmless.
An Undo toast either removes the new goal or restores the old rows.

## Tests

`cargo test` runs the cloud tests (query escaping, old settings files still
load, changing folder, and reading Online Accounts without side effects), the
model tests (priority ordering; slots → blocks, ignoring
deleted rows; unscheduling) and the parser tests in `import.rs`: date formats, your real
Notion Markdown (12 week ranges + 7 days, multi-line cells, notes), TSV
quoting round-trip, HTML, choosing Markdown over Notion's broken HTML,
reporting rows without dates, and headerless tables.

## Google Drive sync (`cloud.rs`, `sync.rs`)

The local file stays the working copy, and Drive holds the shared copy.

**No sign-in of our own.** Google requires every API caller to be a
registered OAuth client. Instead of shipping one (or asking users to create
one), Planner borrows the Google account from Ubuntu Settings → Online
Accounts. GNOME Online Accounts ("GOA") is GNOME's registered client, and it
gives local apps access tokens over D-Bus:
- `cloud::google_accounts()` calls `GetManagedObjects` on
  `org.gnome.OnlineAccounts` and keeps accounts whose `ProviderType` is
  `google` and that implement `OAuth2Based`. It reads `Id`, `Identity`
  (email) and `AttentionNeeded`.
- Each Drive request gets a fresh token from `OAuth2Based.GetAccessToken`
  (GOA caches and refreshes it). A `NotAuthorized` error means GNOME needs
  you to sign in again; the window then shows "Open Online Accounts…",
  which runs `gnome-control-center online-accounts`.

**Folder confinement:** GOA's token covers the whole Drive, so the
"one folder" rule is enforced by Planner's own code:
- The folder is found by name at the top of My Drive, or created there.
- The data file is looked up only *inside* that folder by name, and a stored
  `file_id` is only trusted if its `parents` include that folder.
- Planner never lists or modifies anything else.
- Query values are escaped (`quoted`) so a folder name can't break the query.

**Settings file:** `~/.config/planner/google-drive.json` holds the GOA
account id and email, the folder name, the Drive `file_id`, the file
`version` last seen, a `dirty` flag (local changes not uploaded yet) and
`last_sync`. It contains no tokens or secrets.

**Drive layout:** `My Drive/<folder>/planner-data.json`, the same JSON as the
local file. The first upload creates the folder and file (metadata first, then
`PATCH …?uploadType=media`). Later uploads overwrite the content in place.
Changing the folder clears `file_id`/`version`, so the next sync finds or
creates the file there; if the new folder already has data, Planner asks
which copy to keep.

**Threading:** `cloud.rs` is blocking (`ureq` with rustls, 30 s timeout).
`sync.rs` runs it with `gio::spawn_blocking` inside
`glib::spawn_future_local`, so the UI never waits on the network. One sync
runs at a time (`busy`), and changes made meanwhile set `again`.

**When it syncs:**

| Trigger | What happens |
|---|---|
| Local save (`Ui::save_soon` → `sync::local_saved`) | mark `dirty`, upload 2 s later (debounced) |
| Startup, after sign-in, "Sync now" (`sync_now`) | no Drive file → upload. Drive unchanged → upload if `dirty`. Drive changed → download, unless we're `dirty` or this is a first sign-in with local data, then ask |
| Before every upload | if Drive's `version` moved since we last saw it → ask instead of overwriting |
| Window close with `dirty` | hide the window, finish the upload, then destroy it (`hold_close`) |

**Conflicts** (`ask_conflict`): "Use Google Drive's" / "Keep this
computer's" / "Decide later". The copy that isn't kept is written next to the
data file (`data.before-drive.json` or `data.drive-backup.json`), so nothing
is lost.

**Status icon** (`sync::status_button`, in each goal page's header): a
cairo-drawn database cylinder with a badge. Red ✕ = signed out, green ✓ =
synced, blue arc = syncing, orange ! = error (the tooltip says what). All
icons repaint on status changes, and the open sign-in window rebuilds
itself.

**Errors:** if the account disappears from Settings, Planner disconnects and
shows a toast. Anything else, including "sign in again in Settings" and a
token that lacks Drive access, shows the orange ! badge with the message,
leaves the connection in place, and retries on the next save or "Sync now".

## Install / desktop integration

`./install.sh` builds the release binary and installs, for the current user:

| What | Where |
|---|---|
| binary | `~/.local/bin/planner` |
| icon | `~/.local/share/icons/hicolor/scalable/apps/dev.vamsi.Planner.svg` |
| launcher | `~/.local/share/applications/dev.vamsi.Planner.desktop` |

The app ID, icon name and `.desktop` file name are all `dev.vamsi.Planner`.
That's what lets GNOME match the running window to its dock icon.

Release profile (`Cargo.toml`): `opt-level="s"`, LTO, 1 codegen unit,
stripped, `panic="abort"`.

## Known limitations / next steps

- No autostart yet. A `~/.config/autostart/dev.vamsi.Planner.desktop` would
  open the Today page at login.
- Range rows complete once, not per day. Per-day tracking would mean
  `completed_on` → `BTreeSet<NaiveDate>`.
- No recurrence ("every weekday"); it would be evaluated inside `agenda()`.
- Import skips rows without dates rather than attaching them to a neighbour.
  A Notion table where a week's days have no dates would need a "spread a
  week's topics over its days" option.
- Only row deletion has undo; goal deletion is confirmed but permanent.
- Sync is whole-file and last-writer-wins after the conflict prompt; there's
  no per-row merge.
- No UI tests; `model`, `import` and `cloud` have unit tests (`cargo test`).
