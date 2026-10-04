# Planner for Android

The Android version of Planner, written in **Kotlin + Jetpack Compose** (Material 3).
It has the same features and visuals as the desktop app, laid out for phones (and
tablets):

| Desktop | Android |
|---|---|
| Sidebar (Today, goals by priority, P-badges, counts) | Navigation drawer; a permanent sidebar on tablets (≥ 840 dp) |
| Today page: todos + day-plan clock side by side | One scrolling column with the clock below the todos; side by side from 720 dp |
| "+" next to Today reveals quick add | Same "+" in the top bar |
| Goal page: emoji icon, big title, Priority, Done·Date·Task·Notes table | Same. The table scrolls sideways on narrow screens and stretches to fill wide ones |
| Drag column borders to resize | Drag the handles in the table header (remembered per goal on the device) |
| Click a cell to edit; Enter / click elsewhere finishes; Shift+Enter = new line | Tap to edit; keyboard **Done**, a hardware Enter, or tapping elsewhere finishes; Shift+Enter = new line |
| "+" on week rows adds a day of that week | Same |
| Ctrl+V a Notion table → import | **Share** a table from Notion to Planner, or drawer → *Import from Notion* (reads the clipboard) |
| 24 h clock, 48 half-hour slots, paint by dragging | Same, by touch |

The data file uses the **same JSON as the desktop app**, so a file can move between
them unchanged, which is the basis for future sync. Google Drive sign-in is not in
the Android app yet.

## Build

Needs JDK 17+ and the Android SDK (with `sdk.dir` in `local.properties`, or
`ANDROID_HOME`). Android Studio provides both: open this folder in it.

```bash
./gradlew assembleDebug        # debug APK
./gradlew assembleRelease      # optimized release APK (R8 + resource shrinking)
./gradlew bundleRelease        # Play Store App Bundle (.aab)
./gradlew testDebugUnitTest    # unit tests (model + Notion import, desktop fixtures)
```

**Binaries go to `app_android/dist/`**, separate from the desktop build's `target/`:

```
dist/planner-1.0.0-debug.apk
dist/planner-1.0.0-release.apk
dist/planner-1.0.0-release.aab
```

Install the debug build on a phone with USB debugging on:
`adb install -r dist/planner-1.0.0-debug.apk`. It installs as `dev.vamsi.planner.debug`,
so it can sit next to the Play version.

## Publishing to Google Play

1. **Create an upload key** (once; keep it safe, it's not in git):
   ```bash
   keytool -genkeypair -v -keystore ~/planner-upload.jks -alias upload \
     -keyalg RSA -keysize 4096 -validity 10000
   ```
2. **Point the build at it:** create `app_android/keystore.properties` (git-ignored):
   ```properties
   storeFile=/home/you/planner-upload.jks
   storePassword=…
   keyAlias=upload
   keyPassword=…
   ```
3. **Build:** `./gradlew bundleRelease` → `dist/planner-1.0.0-release.aab` (signed).
4. In the **Play Console** (one-time $25 developer account), create the app, turn on
   **Play App Signing** (Google holds the app signing key; yours is only the upload
   key), and upload the `.aab` to an internal testing track first.
5. **Store listing and policy forms:**
   - Data safety: the app collects no data and has no network permission. Data stays
     on the device, plus Android's own backup to the user's Google account
     (`res/xml/data_extraction_rules.xml`).
   - Content rating questionnaire.
   - Privacy policy URL: required even if nothing is collected.
   - New personal developer accounts must run a closed test with testers for 14 days
     before going to production.
6. **For each release:** bump `versionCode` (must increase) and `versionName` in
   `app/build.gradle.kts`.

Fixed choices worth knowing:
- `applicationId = "dev.vamsi.planner"` can never change after the first upload.
- `targetSdk = 36` (Android 16) meets Play's current requirement. `compileSdk = 37`
  because the current Compose libraries need it.

## Layout

```
app_android/
├── app/build.gradle.kts         SDK levels, R8, signing, copy-to-dist tasks
├── app/src/main/
│   ├── AndroidManifest.xml      no permissions; share-target intent filter
│   ├── java/dev/vamsi/planner/
│   │   ├── MainActivity.kt      edge-to-edge, share intents, day rollover, save on stop
│   │   ├── data/Model.kt        Store/Goal/PlanRow (desktop JSON), agenda, overdue, slots
│   │   ├── data/NotionImport.kt Markdown/TSV/HTML table parser (port of import.rs)
│   │   ├── data/Repository.kt   files/data.json (atomic writes) + per-device column widths
│   │   └── ui/                  Compose screens
│   │       ├── PlannerViewModel.kt  state, debounced save, undo events
│   │       ├── PlannerApp.kt        drawer / permanent sidebar, snackbar undo, back handling
│   │       ├── TodayScreen.kt       Today page, quick add, carried over, time pills
│   │       ├── DayPlan.kt           24 h clock (Canvas + touch painting), chips, schedule
│   │       ├── GoalScreen.kt        goal page, resizable/fluid table, cell editing
│   │       ├── ImportDialog.kt      full-screen import with live preview
│   │       └── Components.kt        badges, dots, emoji picker, date/span dialogs
│   └── res/                     vector icons, adaptive + themed launcher icon, backup rules
└── app/src/test/                JVM tests + the desktop's real Notion clipboard fixtures
```

**State:** one immutable `Store` in `PlannerViewModel`. Every change replaces it,
Compose recomposes whatever read it, and a save to `files/data.json` runs 400 ms
later on a background thread. The data is also written when the app goes to the
background.

**Optimizations:**
- R8 full-mode shrinking and resource shrinking; the release APK is about 1.7 MB.
- Vector icons instead of an icon library.
- No network or analytics dependencies.
- Lazy lists keyed by row id.
- The clock is a single `Canvas`.
