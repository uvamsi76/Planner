# Planner

A small, native Ubuntu/GNOME app for long-term planning, written in Rust with
GTK4 + libadwaita.

Plan each goal (Career, Health, …) as a simple table of dated rows. Every
morning the **Today** page shows exactly what those plans say to do.

## Install

Needs the GTK4/libadwaita headers once:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

Then build and install for your user (no sudo):

```bash
./install.sh
```

"Planner" now appears in the app grid; right-click it in the dock to pin it.
Re-run `./install.sh` after pulling changes.

For development: `cargo run`.

## Use

- **Goal pages** (sidebar, **+** to create): click the emoji to change it,
  click the title to rename. The table has **Done · Date · Task · Notes**.
  - Click a cell to edit it. **Enter** (or clicking anywhere else) finishes;
    **Shift+Enter** starts a new line.
  - The **+** at the far left of a week row adds a day of that week right
    below it.
  - Click a date to pick the start day and how many days it lasts. One day is
    a daily todo; 7 days is a week focus.
  - **+ New day**, **+ Next 7 days** and **+ Week focus** add rows after the
    last one. Hover a row to delete it (with Undo).
- **Today**: today's todos grouped by goal, 📌 week focuses, unfinished past
  todos under "Carried over", and a quick-add box. ◀ ▶ or the calendar
  browse other days.
- **Import from Notion**: select your plan table in Notion, press Ctrl+C,
  then press **Ctrl+V** anywhere in Planner. The import window shows what it
  understood: week ranges, daily rows, and anything it skipped. Pick a new
  or existing goal and click **Import**. Columns are detected automatically:
  a date column ("5 Oct", "12–18 Oct", "26 Oct – 1 Nov", …), an optional Week
  column, the task/topic column, and extra columns as notes. Markdown and
  spreadsheet tables work too.
- **Terminal**: `planner today` or `planner today 2026-10-13` prints a day's
  agenda.

Data lives in `~/.local/share/planner/data.json` and saves automatically.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the code is organized.
