# Planner

A Linux desktop todo app for long-term planning, written in Rust (egui).

Plan each goal (Career, Health, …) as a table of date-ranged rows. The home page
shows everything those plans say to do on a given day.

## Run

```bash
cargo run --release
```

Print a day's agenda in the terminal instead of opening the window:

```bash
cargo run --release -- today            # today
cargo run --release -- today 2026-10-13 # any date
```

Data is saved automatically to `~/.local/share/planner/data.json`. On first run
it is seeded with a sample "Career — DSA" plan.

## Using it

- **Goal page** (sidebar): one row per day for daily todos, or one row spanning
  several days (e.g. a week) for a focus/milestone. Columns are customizable
  under "Table columns"; the first column is the task title.
- **Today page**: today's todos grouped by goal, week focuses shown as 📌,
  unfinished past todos under "Carried over", plus a quick-add box.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the code is organized.
