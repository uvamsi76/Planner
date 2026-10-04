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
  click the title to rename, set a **Priority** (P1–P4). The sidebar and the
  Today page list goals by priority. The table has **Done · Date · Task ·
  Notes**.
  - Drag the faint lines between the column headers to resize columns
    (remembered per goal). The table widens with the window.
  - Click a cell to edit it. **Enter** (or clicking anywhere else) finishes;
    **Shift+Enter** starts a new line.
  - The **+** at the far left of a week row adds a day of that week right
    below it.
  - Click a date to pick the start day and how many days it lasts. One day is
    a daily todo; 7 days is a week focus.
  - **+ New day**, **+ Next 7 days** and **+ Week focus** add rows after the
    last one. Hover a row to delete it (with Undo).
- **Today**: today's todos grouped by goal (by priority), 📌 week focuses,
  and unfinished past todos under "Carried over". The **+** next to "Today"
  opens a quick-add box (Esc hides it). ◀ ▶ or the calendar browse other
  days.
- **Day plan** (right side of Today): a 24-hour clock in half-hour slots.
  Click a task chip, then click or drag around the clock to give it time.
  Drag over its own slots to clear them, or use the Eraser. Tasks without
  time are listed first as unscheduled, by priority. Scheduled todos show
  their time on the left.
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

## Google Drive sync

Planner uses the Google account from **Ubuntu Settings → Online Accounts**,
so there's no Google Cloud setup and no client ID.

1. In Settings → Online Accounts, add your Google account (or sign in again
   if it says it needs attention). Keep **Files** turned on.
2. In Planner, open a goal page and click the **database icon** in the header
   bar. It shows ✕ when not connected and ✓ when synced.
3. Pick the account and a folder name in My Drive (default **Planner**),
   then press **Use this account**.

Your data is then kept in **My Drive › <folder> › planner-data.json**, and
the local file stays as an offline copy. Changes upload a couple of seconds
after you make them. On startup Planner loads the Drive copy, and if both
sides changed it asks which one to keep. You can change the folder or
disconnect from the same window.

GNOME's Google sign-in covers your whole Drive. Planner limits itself to the
chosen folder: it only looks up and writes `planner-data.json` inside it.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the code is organized.

## Android app

`app_android/` holds the Android version (Kotlin + Jetpack Compose), with the same
features laid out for phones. It uses the same data format. Its binaries are
built into `app_android/dist/`. See [app_android/README.md](app_android/README.md)
for building and publishing to Google Play.
