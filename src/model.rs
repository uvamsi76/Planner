//! Data model and persistence. No GUI code lives here.
//!
//! A `Goal` (Career, Health, ...) owns a simple table of `PlanRow`s. Each row
//! covers a date range: a single day (`start == end`) is a daily todo, a longer
//! range (e.g. a week) is a focus that shows up on every day it covers.

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlanRow {
    pub id: u64,
    pub start: NaiveDate,
    pub end: NaiveDate,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub completed_on: Option<NaiveDate>,
    /// v1 format (custom columns); only read, migrated into task/notes on load.
    #[serde(default, skip_serializing)]
    cells: Vec<String>,
}

impl PlanRow {
    pub fn is_day(&self) -> bool {
        self.start == self.end
    }

    pub fn covers(&self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    pub fn done(&self) -> bool {
        self.completed_on.is_some()
    }

    pub fn set_done(&mut self, done: bool, on: NaiveDate) {
        self.completed_on = done.then_some(on);
    }

    /// Length in days, inclusive (a single-day row is 1).
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Goal {
    pub id: u64,
    pub name: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    pub rows: Vec<PlanRow>,
}

fn default_icon() -> String {
    "🎯".into()
}

impl Goal {
    pub fn add_row(&mut self, id: u64, start: NaiveDate, end: NaiveDate) -> &mut PlanRow {
        self.rows.push(PlanRow {
            id,
            start,
            end,
            task: String::new(),
            notes: String::new(),
            completed_on: None,
            cells: Vec::new(),
        });
        self.rows.last_mut().unwrap()
    }

    pub fn row_mut(&mut self, id: u64) -> Option<&mut PlanRow> {
        self.rows.iter_mut().find(|r| r.id == id)
    }

    /// Chronological, with a range listed before the days it contains.
    pub fn sort_rows(&mut self) {
        self.rows.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end), r.id));
    }

    /// First free date after the last day-row (or range-row), or `today`.
    pub fn next_free_date(&self, ranges: bool, today: NaiveDate) -> NaiveDate {
        self.rows
            .iter()
            .filter(|r| r.is_day() != ranges)
            .map(|r| r.end + Duration::days(1))
            .max()
            .unwrap_or(today)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Store {
    pub goals: Vec<Goal>,
    next_id: u64,
}

impl Store {
    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("planner")
            .join("data.json")
    }

    /// Loads the store, creating it with sample data on first run.
    pub fn load() -> io::Result<Store> {
        let json = match fs::read_to_string(Self::path()) {
            Ok(json) => json,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let store = Store::sample();
                store.save()?;
                return Ok(store);
            }
            Err(e) => return Err(e),
        };
        let mut store: Store =
            serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if store.migrate() {
            // Keep the original next to the new file before rewriting it.
            fs::write(Self::path().with_extension("v1.json"), &json)?;
            store.save()?;
        }
        Ok(store)
    }

    /// v1 rows kept text in `cells`; fold them into task + notes. Returns true if anything changed.
    fn migrate(&mut self) -> bool {
        let mut changed = false;
        for row in self.goals.iter_mut().flat_map(|g| &mut g.rows) {
            if row.cells.is_empty() {
                continue;
            }
            let mut cells = std::mem::take(&mut row.cells).into_iter();
            row.task = cells.next().unwrap_or_default();
            row.notes = cells.filter(|c| !c.trim().is_empty()).collect::<Vec<_>>().join(" · ");
            changed = true;
        }
        changed
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("store serializes")
    }

    /// Atomic write: temp file + rename, so a crash never truncates the data.
    pub fn save(&self) -> io::Result<()> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, self.to_json())?;
        fs::rename(tmp, path)
    }

    pub fn goal(&self, id: u64) -> Option<&Goal> {
        self.goals.iter().find(|g| g.id == id)
    }

    pub fn goal_mut(&mut self, id: u64) -> Option<&mut Goal> {
        self.goals.iter_mut().find(|g| g.id == id)
    }

    pub fn new_goal(&mut self, name: &str) -> u64 {
        let id = self.next_id();
        self.goals.push(Goal { id, name: name.into(), icon: default_icon(), rows: Vec::new() });
        id
    }

    /// Everything planned for `date`, grouped by goal: (goal index, range rows, day rows).
    pub fn agenda(&self, date: NaiveDate) -> Vec<(usize, Vec<usize>, Vec<usize>)> {
        self.goals
            .iter()
            .enumerate()
            .filter_map(|(gi, g)| {
                let (mut ranges, mut days) = (Vec::new(), Vec::new());
                for (ri, r) in g.rows.iter().enumerate() {
                    if r.covers(date) {
                        if r.is_day() { days.push(ri) } else { ranges.push(ri) }
                    }
                }
                (!ranges.is_empty() || !days.is_empty()).then_some((gi, ranges, days))
            })
            .collect()
    }

    /// Unfinished single-day todos from before `date`: (goal index, row index).
    pub fn overdue(&self, date: NaiveDate) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (gi, g) in self.goals.iter().enumerate() {
            for (ri, r) in g.rows.iter().enumerate() {
                if r.is_day() && r.end < date && !r.done() {
                    out.push((gi, ri));
                }
            }
        }
        out
    }

    /// (done, total) over the single-day todos on `date`.
    pub fn progress(&self, date: NaiveDate) -> (usize, usize) {
        self.goals
            .iter()
            .flat_map(|g| &g.rows)
            .filter(|r| r.is_day() && r.covers(date))
            .fold((0, 0), |(d, t), r| (d + r.done() as usize, t + 1))
    }

    /// First-run example: the DSA plan from the Notion table.
    fn sample() -> Store {
        let mut s = Store::default();
        let gid = s.new_goal("Career — DSA");
        let d = |m: u32, day: u32| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let weeks = [
            (d(10, 5), d(10, 11), "Week 1 — Re-solve your 47 solved problems",
             "Patterns: hashmap counting, prefix/suffix products, two pointers · Landmarks: Product of Array Except Self, Longest Consecutive Sequence, 3Sum, Trapping Rain Water"),
            (d(10, 12), d(10, 18), "Week 2 — Stack (7), Binary Search (7)",
             "Patterns: monotonic stack, bracket matching, bound templates, search on the answer · Landmarks: Daily Temperatures, Largest Rectangle in Histogram, Koko Eating Bananas"),
            (d(10, 19), d(10, 25), "Week 3 — Sliding Window (6), Linked List (11)",
             "Patterns: fixed/variable windows, monotonic deque, fast & slow pointers · Landmarks: Minimum Window Substring, LRU Cache, Merge K Sorted Lists"),
            (d(10, 26), d(11, 1), "Week 4 — Trees (15)",
             "Patterns: DFS returning values upward, BFS by level, BST invariants · Landmarks: Validate BST, Max Path Sum, Serialize and Deserialize"),
            (d(11, 2), d(11, 8), "Week 5 — Tries (3), Heap (7), Intervals (6)",
             "Patterns: trie insert/search, top-k heap, two heaps, sort then sweep · Landmarks: Word Search II, Find Median from Data Stream, Meeting Rooms II"),
            (d(11, 9), d(11, 15), "Week 6 — Backtracking (9), Greedy (8)",
             "Patterns: choose/explore/un-choose, dedupe by sorting, Kadane · Landmarks: Combination Sum II, N-Queens, Jump Game II, Gas Station"),
        ];
        let days = [
            (5, "Arrays & Hashing (7)"),
            (6, "Arrays & Hashing (2) + Two Pointers (5)"),
            (7, "Stack (5) + Binary Search (2)"),
            (8, "Binary Search (3) + Sliding window (2) + Linked List (2)"),
            (9, "Linked List (2) + Trees (4) + Backtracking (1)"),
            (10, "Backtracking (1) + Heap / Priority Queue (4) + Graphs (2)"),
            (11, "Graphs (3) + DP (1) + Adv Graph (1)"),
        ];
        let ids: Vec<u64> = (0..weeks.len() + days.len()).map(|_| s.next_id()).collect();
        let mut ids = ids.into_iter();
        let goal = s.goal_mut(gid).unwrap();
        goal.icon = "💼".into();
        for (start, end, task, notes) in weeks {
            let row = goal.add_row(ids.next().unwrap(), start, end);
            row.task = task.into();
            row.notes = notes.into();
        }
        for (day, task) in days {
            goal.add_row(ids.next().unwrap(), d(10, day), d(10, day)).task = task.into();
        }
        goal.sort_rows();
        s
    }
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// "Mon 5 Oct" / "5–11 Oct" / "26 Oct – 1 Nov"
pub fn fmt_range(start: NaiveDate, end: NaiveDate) -> String {
    use chrono::Datelike;
    if start == end {
        start.format("%a %-d %b").to_string()
    } else if start.month() == end.month() {
        format!("{}–{}", start.format("%-d"), end.format("%-d %b"))
    } else {
        format!("{} – {}", start.format("%-d %b"), end.format("%-d %b"))
    }
}
