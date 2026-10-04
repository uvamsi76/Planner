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
- **Today**: just the day's checkbox todos, grouped by goal (by priority),
  plus unfinished past todos under "Carried over". Long notes are cut to two
  lines. Week focuses (multi-day rows) are context, not tasks: they sit
  collapsed under **📌 Week focus** at the end of the page. The **+** next to
  "Today" opens a quick-add box (Esc hides it). ◀ ▶ or the calendar browse
  other days.
- **Day plan** (right side of Today): a 24-hour clock in half-hour slots.
  Click a task chip, then click or drag around the clock to give it time.
  Drag over its own slots to clear them, or use the Eraser. Tasks without
  time are listed first as unscheduled, by priority. Scheduled todos show
  their time on the left. **🌙 Sleep** under the clock sets your bedtime and
  wake-up; those hours are shaded darker. The setting is stored in the data
  file, so the phone app uses it too.
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

## Google Drive sync (desktop + Android)

Both apps keep the same file, **My Drive › Planner › planner-data.json**. Click
the **database icon** in the Today page's header: it shows ✕ when signed out,
✓ when synced, and ! with the reason when something fails. Changes upload a
couple of seconds after you make them. Each app loads the Drive copy when it
starts. If both sides changed, this device's data wins and is uploaded, and
Drive's previous copy is saved locally as `data.drive-backup.json`. Planner asks only
for `drive.file`, so it can see only the files it creates.

**One-time Google Cloud setup (done by you, the developer).** After this, users
just tap *Sign in with Google*.

1. <https://console.cloud.google.com/> → create a project → **APIs & Services →
   Library** → enable **Google Drive API**.
2. **Google Auth Platform → Branding**: app name, support email. **Audience**:
   External, and add your account as a **test user**. **Data access**: add the
   scope `.../auth/drive.file`.
3. **Clients → Create client**, three times in the same project:
   - **Desktop app**: download the JSON, save it as `google-client.json` in this
     folder (git-ignored), then run `./install.sh`.
   - **Android**, package `dev.vamsi.planner.debug`, SHA-1
     `4E:36:DD:84:01:1A:4A:19:62:B8:E2:B7:CE:6A:84:DA:AB:BF:07:9A` (debug builds).
   - **Android**, package `dev.vamsi.planner`, SHA-1
     `8D:92:F8:72:7E:13:78:D8:45:07:5A:19:DD:69:99:58:73:57:82:AA` (your upload key).
     Once the app is on Play, add another Android client with the **App signing
     key** SHA-1 from Play Console → Setup → App signing.
4. While the app is in *Testing*, only test users can sign in, and Google ends
   desktop sign-ins after 7 days. Publish the app (Audience → Publish) to
   lift that.

If Android sign-in fails, the Drive window shows Google's reason. For an
unregistered build, it lists the exact package name and SHA-1 to register.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the code is organized.

## Android app

`app_android/` holds the Android version (Kotlin + Jetpack Compose), with the same
features laid out for phones. It uses the same data format. Its binaries are
built into `app_android/dist/`. See [app_android/README.md](app_android/README.md)
for building and publishing to Google Play.
