//! Data model and persistence. No GUI code lives here.
//!
//! A `Goal` (Career, Health, ...) owns a simple table of `PlanRow`s. Each row
//! covers a date range: a single day (`start == end`) is a daily todo, a longer
//! range (e.g. a week) is a focus that shows up on every day it covers.

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::PathBuf};

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
    /// 1 (highest) … 4 (lowest); 0 = no priority.
    #[serde(default)]
    pub priority: u8,
    /// Table layout remembered per goal: Date column width (px) and the
    /// Task share of the Task + Notes width.
    #[serde(default)]
    pub date_width: Option<i32>,
    #[serde(default)]
    pub task_ratio: Option<f64>,
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

/// Half-hour slots in a day.
pub const SLOTS: u8 = 48;

/// A run of consecutive half-hour slots given to one row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub start: u8,
    pub len: u8,
    pub row: u64,
}

impl Block {
    pub fn label(&self) -> String {
        format!("{}–{}", slot_time(self.start), slot_time(self.start + self.len))
    }
}

/// "09:30" for slot 19; slot 48 is "24:00".
pub fn slot_time(slot: u8) -> String {
    format!("{:02}:{:02}", slot / 2, if slot % 2 == 1 { 30 } else { 0 })
}

/// Sleep hours as half-hour slots, `start` up to (not including) `end`,
/// wrapping past midnight (22:00–06:00 is start 44, end 12). Shared by both apps.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Sleep {
    pub start: u8,
    pub end: u8,
}

impl Default for Sleep {
    fn default() -> Self {
        Sleep { start: 44, end: 12 }
    }
}

impl Sleep {
    pub fn contains(&self, slot: u8) -> bool {
        if self.start <= self.end {
            (self.start..self.end).contains(&slot)
        } else {
            slot >= self.start || slot < self.end
        }
    }

    pub fn label(&self) -> String {
        format!("{}–{}", slot_time(self.start), slot_time(self.end % SLOTS))
    }
}

/// Sort key for a goal priority: P1 first, "none" last.
pub fn priority_rank(p: u8) -> u8 {
    if p == 0 { 5 } else { p }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Store {
    pub goals: Vec<Goal>,
    next_id: u64,
    /// Day plans: date → half-hour slot (0..48) → row id doing it.
    #[serde(default)]
    pub schedule: BTreeMap<NaiveDate, BTreeMap<u8, u64>>,
    /// Sleep hours, shaded on the day-plan clock.
    #[serde(default)]
    pub sleep: Sleep,
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

    /// Parse data from elsewhere (e.g. Google Drive), upgrading old formats.
    pub fn from_json(json: &str) -> Result<Store, String> {
        let mut store: Store = serde_json::from_str(json).map_err(|e| format!("Not Planner data: {e}"))?;
        store.migrate();
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
        self.goals.push(Goal {
            id,
            name: name.into(),
            icon: default_icon(),
            priority: 0,
            date_width: None,
            task_ratio: None,
            rows: Vec::new(),
        });
        id
    }

    /// Everything planned for `date`, grouped by goal: (goal index, range rows, day rows).
    pub fn agenda(&self, date: NaiveDate) -> Vec<(usize, Vec<usize>, Vec<usize>)> {
        self.goal_order()
            .into_iter()
            .map(|gi| (gi, &self.goals[gi]))
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
        for gi in self.goal_order() {
            for (ri, r) in self.goals[gi].rows.iter().enumerate() {
                if r.is_day() && r.end < date && !r.done() {
                    out.push((gi, ri));
                }
            }
        }
        out
    }

    /// Goal indices by priority (P1 first, no priority last), otherwise in
    /// creation order.
    pub fn goal_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.goals.len()).collect();
        order.sort_by_key(|&i| priority_rank(self.goals[i].priority));
        order
    }

    pub fn row(&self, rid: u64) -> Option<(&Goal, &PlanRow)> {
        self.goals.iter().find_map(|g| g.rows.iter().find(|r| r.id == rid).map(|r| (g, r)))
    }

    /// Assign (or with `None`, clear) one half-hour slot on `date`.
    pub fn set_slot(&mut self, date: NaiveDate, slot: u8, row: Option<u64>) {
        let day = self.schedule.entry(date).or_default();
        match row {
            Some(rid) => {
                day.insert(slot, rid);
            }
            None => {
                day.remove(&slot);
            }
        }
        if day.is_empty() {
            self.schedule.remove(&date);
        }
    }

    /// Slot → row for `date`, ignoring rows that no longer exist.
    pub fn slots(&self, date: NaiveDate) -> BTreeMap<u8, u64> {
        self.schedule
            .get(&date)
            .map(|day| day.iter().filter(|(_, rid)| self.row(**rid).is_some()).map(|(s, r)| (*s, *r)).collect())
            .unwrap_or_default()
    }

    /// The day's slots merged into runs, in time order.
    pub fn blocks(&self, date: NaiveDate) -> Vec<Block> {
        let mut out: Vec<Block> = Vec::new();
        for (slot, rid) in self.slots(date) {
            match out.last_mut() {
                Some(b) if b.row == rid && b.start + b.len == slot => b.len += 1,
                _ => out.push(Block { start: slot, len: 1, row: rid }),
            }
        }
        out
    }

    /// Forget a deleted row's time slots on every day.
    pub fn unschedule_row(&mut self, rid: u64) {
        for day in self.schedule.values_mut() {
            day.retain(|_, r| *r != rid);
        }
        self.schedule.retain(|_, day| !day.is_empty());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn sleep_wraps_past_midnight() {
        let night = Sleep::default();
        assert!(night.contains(44) && night.contains(47) && night.contains(0) && night.contains(11));
        assert!(!night.contains(12) && !night.contains(43));
        assert_eq!(night.label(), "22:00–06:00");
        let nap = Sleep { start: 26, end: 28 };
        assert!(nap.contains(26) && nap.contains(27) && !nap.contains(28));
        assert!(!Sleep { start: 10, end: 10 }.contains(10), "equal start/end = no sleep");
        // Older files without the field get the default.
        let s: Store = serde_json::from_str(r#"{"goals":[],"next_id":0}"#).unwrap();
        assert_eq!(s.sleep, Sleep::default());
    }

    #[test]
    fn goals_ordered_by_priority() {
        let mut s = Store::default();
        let a = s.new_goal("none");
        let b = s.new_goal("p3");
        let c = s.new_goal("p1");
        s.goal_mut(b).unwrap().priority = 3;
        s.goal_mut(c).unwrap().priority = 1;
        let names: Vec<_> = s.goal_order().iter().map(|&i| s.goals[i].name.clone()).collect();
        assert_eq!(names, ["p1", "p3", "none"]);
        let _ = a;
    }

    #[test]
    fn slots_merge_into_blocks_and_ignore_deleted_rows() {
        let mut s = Store::default();
        let g = s.new_goal("g");
        let (r1, r2) = (s.next_id(), s.next_id());
        s.goal_mut(g).unwrap().add_row(r1, d(5), d(5));
        s.goal_mut(g).unwrap().add_row(r2, d(5), d(5));
        for slot in [18, 19, 20] {
            s.set_slot(d(5), slot, Some(r1));
        }
        s.set_slot(d(5), 21, Some(r2));
        s.set_slot(d(5), 30, Some(r1));
        s.set_slot(d(5), 40, Some(999)); // row that doesn't exist
        assert_eq!(
            s.blocks(d(5)),
            vec![
                Block { start: 18, len: 3, row: r1 },
                Block { start: 21, len: 1, row: r2 },
                Block { start: 30, len: 1, row: r1 },
            ]
        );
        assert_eq!(s.blocks(d(5))[0].label(), "09:00–10:30");
        s.set_slot(d(5), 21, None);
        s.unschedule_row(r1);
        s.unschedule_row(999);
        assert!(s.schedule.is_empty());
    }
}
