# Planner — Architecture

Snapshot of the design as of the initial skeleton (Oct 2026). Single-binary
native Linux desktop app written in Rust with [egui/eframe](https://github.com/emilk/egui)
(immediate-mode GUI), persisting to one JSON file.

## Core idea

Long-term plans are tables of **date-ranged rows**. The home page is a pure
*query* over those tables: "which rows cover date D?". Nothing is copied or
generated per day — the daily todo list is derived on every frame.

```
 Goal "Career — DSA"                      Home page for 5 Oct
 ┌──────────────┬──────────────────┐      ┌───────────────────────────────┐
 │ 5 Oct–11 Oct │ Week 1 — Re-solve│ ───▶ │ 📌 Week 1 — Re-solve… (focus) │
 │ 5 Oct        │ Arrays & Hashing │ ───▶ │ [ ] Arrays & Hashing (7)      │
 │ 6 Oct        │ Arrays + 2 Ptrs  │      └───────────────────────────────┘
 └──────────────┴──────────────────┘
```

- **Day row** (`start == end`) → a checkable todo on that day.
- **Range row** (`start < end`, e.g. a week) → a "focus" banner on every day it
  covers, showing its extra columns (Patterns, Landmark problems, …).

## Module map

```
src/
├── main.rs    Entry point: loads Store, dispatches CLI (`planner today [date]`) or launches the GUI
├── model.rs   Data types, persistence, date queries, first-run sample data   (no GUI code)
├── app.rs     PlannerApp state, eframe::App impl, sidebar, autosave, shared widgets
├── today.rs   Home page: agenda for one date, carried-over items, quick add
└── plan.rs    Goal page: editable Notion-style table, columns editor, row generators
```

Dependency direction is one-way: `main → app → {today, plan} → model`.
`model.rs` knows nothing about egui, so it can be reused by the CLI, tests, or a
future TUI/web front end.

## Data model (`model.rs`)

```rust
Store { goals: Vec<Goal>, next_id: u64 }          // whole database

Goal {
    id, name, color: [u8; 3],
    columns: Vec<String>,     // user-defined; columns[0] is the title column
    rows: Vec<PlanRow>,
}

PlanRow {
    id, start: NaiveDate, end: NaiveDate,
    cells: Vec<String>,                 // parallel to Goal::columns
    completed_on: Option<NaiveDate>,    // None = not done
}
```

Invariants:
- `row.cells.len() == goal.columns.len()` — maintained by `Goal::add_row`,
  `add_column`, `remove_column`. Always go through these, never push directly.
- A goal always has ≥ 1 column (`remove_column` refuses to drop the last one).
- `start <= end` — enforced in the plan table UI after each date edit.
- IDs come from the single monotonic counter `Store::next_id()` and are unique
  across goals and rows. They are used as stable egui widget IDs, so editing
  dates/reordering rows never mixes up widget state.

Completion is **per row**, not per day: a range row checked once is done for
the whole range.

### Queries

| Function | Returns | Used by |
|---|---|---|
| `Store::agenda(date)` | `Vec<(goal_idx, range_row_idxs, day_row_idxs)>` for goals with anything covering `date` | Home page, CLI |
| `Store::overdue(date)` | unfinished **day** rows with `end < date` | "Carried over" section, CLI |
| `Goal::next_free_date(ranges, today)` | day after the last day-row (or range-row), else today | "+ Day", "+ 7 days", "+ Week range" |
| `Goal::sort_rows()` | sorts by `(start, end desc, id)` so a week row precedes the days it contains | after inserts, "Sort by date" |

Queries return **indices**, not references, so the UI can then take a `&mut`
to the specific row (to toggle a checkbox) without fighting the borrow checker.

## Persistence

- File: `$XDG_DATA_HOME/planner/data.json` (normally `~/.local/share/planner/data.json`),
  resolved via the `dirs` crate. Pretty-printed JSON via `serde`.
- **First run:** if the file doesn't exist, `Store::sample()` seeds the
  "Career — DSA" plan and writes it.
- **Autosave:** at the end of every frame `PlannerApp::autosave` serializes the
  store and writes only if the JSON differs from the last saved string. This
  avoids tracking a dirty flag across dozens of widgets. Cost is negligible at
  this data size; revisit (dirty flag / debounce) if plans grow to thousands of rows.
- **Atomic write:** write to `data.json.tmp`, then `rename` over the real file,
  so a crash mid-write never leaves a truncated database.
- Save errors are shown in red at the bottom of the sidebar; the app keeps running.
- Forward compatibility: new optional fields should use `#[serde(default)]`
  (as `completed_on` does) so older files still load.

Tip: point `XDG_DATA_HOME` at a temp dir to experiment without touching real data:
`XDG_DATA_HOME=/tmp/planner-test cargo run`.

## UI layer

### Frame loop (`app.rs`)

egui is immediate mode: `App::ui` runs every frame and redraws everything from
`PlannerApp` state. There is no separate view-model or event bus — widgets
mutate the `Store` directly.

```
App::ui(ui)
 ├─ midnight check: if the calendar day changed, update `today`
 │    (and move the Home page to it if it was showing "today")
 ├─ request_repaint_after(60 s)      → keeps the midnight check alive while idle
 ├─ Panel::left("nav")  → sidebar(): Today link, goal list (+ today's count), "+ New goal"
 ├─ CentralPanel        → match view { Today → today::show, Goal(id) → plan::show }
 └─ autosave()
```

`PlannerApp` holds the `Store` plus transient UI state: current `View`
(`Today | Goal(id)`), the date being viewed, quick-add text/goal, "hide past
rows" toggle, pending goal-delete confirmation, and new-column text. Only
`Store` is persisted.

### Home page (`today.rs`)

1. Header — ◀ / Today / ▶, date picker, relative label ("Tomorrow", "In 9 days"),
   progress bar over **day rows** for that date.
2. Carried over (only when viewing the real today) — overdue day rows with a
   checkbox and "Move to today" (rewrites the row's dates).
3. One card per goal from `agenda(date)`: range rows as 📌 banners with their
   extra columns, then day rows as checkboxes with their extra columns.
4. Quick add — creates a one-day row on the viewed date in the chosen goal.

### Goal page (`plan.rs`)

- Header: colour picker, editable name, two-step "Delete goal" confirmation.
- Toolbar: `+ Day`, `+ 7 days`, `+ Week range` (all via `add_rows`), hide past
  rows, sort.
- "Table columns" collapsible: rename / remove / add columns.
- Table: `egui::Grid` inside a two-way `ScrollArea`. Columns: marker (dot = day,
  bar = range; goal-coloured if it covers today), done checkbox, start/end date
  pickers, one multiline `TextEdit` per cell, delete button. Row deletion is
  deferred until after the grid loop to avoid mutating the vec while iterating.

### Shared helpers (`app.rs`)

- `goal_color`, `dot` — the default egui font lacks some glyphs (●, →, ▬
  render as boxes), so shapes are painted instead of typed. Glyphs from egui's
  bundled emoji font (◀ ▶ 📌 🗑) are fine.
- `date_picker` — adapter: `egui_extras::DatePickerButton` (v0.36) works on
  `jiff::civil::Date`, while the model uses `chrono::NaiveDate`. The adapter
  converts both ways and only writes back when the user picks a new date.

## CLI (`main.rs`)

`planner today [YYYY-MM-DD]` prints overdue items, range focuses and day todos
in plain text and exits without opening a window. It reuses `agenda` /
`overdue`, which is the main payoff of keeping `model.rs` GUI-free (e.g. call it
from `.bashrc` or a login script).

## Dependencies

| Crate | Why |
|---|---|
| `eframe` / `egui` 0.36 | Window, rendering (glow/OpenGL), widgets. Note 0.36 API: `App::ui(&mut self, ui, frame)`, panels take `&mut Ui` (`egui::Panel::left(..).show(ui, ..)`). |
| `egui_extras` (`datepicker`) | Calendar date picker |
| `jiff` | Date type required by the date picker (adapter only) |
| `chrono` (`serde`) | Model's date type, local "today", formatting |
| `serde`, `serde_json` | Persistence |
| `dirs` | XDG data directory |

## Known limitations / extension points

- **No undo** — deleting a row or column is immediate. A natural fix is a
  snapshot stack of `Store` (cheap: it's already serialized each frame).
- **Range rows complete once**, not per day. Per-day tracking would mean
  `completed_on: Option<NaiveDate>` → `BTreeSet<NaiveDate>`.
- **No recurrence** ("every weekday") — would be a new row kind evaluated in
  `agenda()`; the query-based design makes this a local change.
- **No import** — pasting a Notion/TSV table would parse into `Goal::add_row`.
- **Whole-file JSON** — fine for personal scale; swap `Store::load/save_json`
  for SQLite if data or concurrent access grows. Nothing outside `model.rs`
  touches storage.
- **No tests yet** — `model.rs` (agenda, overdue, column invariants,
  next_free_date) is pure and the obvious first target.
- **Autostart** is not set up; a `~/.config/autostart/planner.desktop` entry
  would open the home page at login.
