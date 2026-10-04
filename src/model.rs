//! Data model and persistence.
//!
//! A `Goal` (Career, Health, ...) owns a plan table. Each `PlanRow` covers a
//! date range: a single day (`start == end`) is a daily todo, a longer range
//! (e.g. a week) is a focus/milestone that shows up on every day it covers.

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlanRow {
    pub id: u64,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// One cell per goal column; `cells[0]` is the task title.
    pub cells: Vec<String>,
    #[serde(default)]
    pub completed_on: Option<NaiveDate>,
}

impl PlanRow {
    pub fn is_day(&self) -> bool {
        self.start == self.end
    }

    pub fn covers(&self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    pub fn title(&self) -> &str {
        self.cells.first().map(String::as_str).unwrap_or("")
    }

    pub fn done(&self) -> bool {
        self.completed_on.is_some()
    }

    pub fn set_done(&mut self, done: bool, on: NaiveDate) {
        self.completed_on = done.then_some(on);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Goal {
    pub id: u64,
    pub name: String,
    pub color: [u8; 3],
    pub columns: Vec<String>,
    pub rows: Vec<PlanRow>,
}

impl Goal {
    pub fn add_row(&mut self, id: u64, start: NaiveDate, end: NaiveDate) -> &mut PlanRow {
        self.rows.push(PlanRow {
            id,
            start,
            end,
            cells: vec![String::new(); self.columns.len()],
            completed_on: None,
        });
        self.rows.last_mut().unwrap()
    }

    pub fn add_column(&mut self, name: String) {
        self.columns.push(name);
        for row in &mut self.rows {
            row.cells.push(String::new());
        }
    }

    pub fn remove_column(&mut self, idx: usize) {
        if self.columns.len() <= 1 || idx >= self.columns.len() {
            return;
        }
        self.columns.remove(idx);
        for row in &mut self.rows {
            if idx < row.cells.len() {
                row.cells.remove(idx);
            }
        }
    }

    /// Chronological, with a range listed before the days it contains.
    pub fn sort_rows(&mut self) {
        self.rows
            .sort_by_key(|r| (r.start, std::cmp::Reverse(r.end), r.id));
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
        match fs::read_to_string(Self::path()) {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let store = Store::sample();
                store.save_json(&serde_json::to_string_pretty(&store)?)?;
                Ok(store)
            }
            Err(e) => Err(e),
        }
    }

    /// Atomic write: temp file + rename, so a crash never truncates the data.
    pub fn save_json(&self, json: &str) -> io::Result<()> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(tmp, path)
    }

    pub fn new_goal(&mut self, name: &str) -> u64 {
        const PALETTE: [[u8; 3]; 6] = [
            [99, 132, 255],
            [46, 184, 114],
            [240, 140, 50],
            [200, 90, 200],
            [230, 80, 90],
            [40, 170, 190],
        ];
        let id = self.next_id();
        self.goals.push(Goal {
            id,
            name: name.to_string(),
            color: PALETTE[self.goals.len() % PALETTE.len()],
            columns: vec!["Task".into(), "Notes".into()],
            rows: Vec::new(),
        });
        id
    }

    /// Everything planned for `date`, grouped by goal: (goal index, ranges, days).
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

    /// First-run example: the DSA plan from the Notion table.
    fn sample() -> Store {
        let mut s = Store::default();
        let gid = s.new_goal("Career — DSA");
        let d = |m: u32, day: u32| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let weeks = [
            (d(10, 5), d(10, 11), "Week 1 — Re-solve your 47 solved problems",
             "Hashmap counting, prefix and suffix products, two pointers on sorted input",
             "Product of Array Except Self, Longest Consecutive Sequence, 3Sum, Trapping Rain Water"),
            (d(10, 12), d(10, 18), "Week 2 — Stack (7), Binary Search (7)",
             "Monotonic stack, bracket matching, lower and upper bound template, binary search on the answer, rotated arrays",
             "Daily Temperatures, Largest Rectangle in Histogram, Koko Eating Bananas, Median of Two Sorted Arrays"),
            (d(10, 19), d(10, 25), "Week 3 — Sliding Window (6), Linked List (11)",
             "Fixed and variable windows with counts, monotonic deque, fast and slow pointers, in-place reversal, dummy node",
             "Minimum Window Substring, Sliding Window Maximum, LRU Cache, Merge K Sorted Lists, Reverse Nodes in K-Group"),
            (d(10, 26), d(11, 1), "Week 4 — Trees (15)",
             "Recursive DFS that returns values upward, BFS by level, BST invariants",
             "Validate BST, Construct Tree from Preorder and Inorder, Binary Tree Maximum Path Sum, Serialize and Deserialize"),
            (d(11, 2), d(11, 8), "Week 5 — Tries (3), Heap (7), Intervals (6)",
             "Trie insert and search, top-k with a heap, two heaps, sort by start then merge or sweep",
             "Word Search II, Find Median from Data Stream, Task Scheduler, Meeting Rooms II"),
            (d(11, 9), d(11, 15), "Week 6 — Backtracking (9), Greedy (8)",
             "Choose, explore, un-choose; removing duplicates by sorting; Kadane; furthest-reach greedy",
             "Combination Sum II, Palindrome Partitioning, N-Queens, Jump Game II, Gas Station"),
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
        let mut ids: Vec<u64> = (0..weeks.len() + days.len()).map(|_| s.next_id()).collect();
        let goal = s.goals.iter_mut().find(|g| g.id == gid).unwrap();
        goal.columns = vec!["Topics".into(), "Patterns to own".into(), "Landmark problems".into()];
        for (start, end, topic, patterns, landmarks) in weeks {
            let row = goal.add_row(ids.pop().unwrap(), start, end);
            row.cells = vec![topic.into(), patterns.into(), landmarks.into()];
        }
        for (day, topic) in days {
            let row = goal.add_row(ids.pop().unwrap(), d(10, day), d(10, day));
            row.cells[0] = topic.into();
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
