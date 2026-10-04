//! Turn a pasted table into plan rows. Pure text processing, no GUI.
//!
//! Accepts what Notion (and most apps) put on the clipboard:
//! - Markdown pipe tables, including cells that span several lines,
//! - tab-separated text, with "quoted" multi-line cells,
//! - HTML tables (`html_table`), converted to the same `Table` shape.
//!
//! Column roles are guessed from the header (Date/Week/Topic…) and, failing
//! that, from the content (the column whose cells parse as dates).

use chrono::{Datelike, NaiveDate};

pub type Table = Vec<Vec<String>>;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRow {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub task: String,
    pub notes: String,
}

impl ImportedRow {
    pub fn is_day(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub rows: Vec<ImportedRow>,
    /// Human-readable reasons for rows that were left out.
    pub skipped: Vec<String>,
    pub date_column: Option<String>,
    pub task_column: Option<String>,
}

/// Parse pasted text (Markdown or TSV) into plan rows. `today` anchors years
/// for dates written without one ("5 Oct").
pub fn parse(text: &str, today: NaiveDate) -> Parsed {
    plan_from_table(&read_table(text), today)
}

pub fn read_table(text: &str) -> Table {
    let text = text.replace("\r\n", "\n");
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    if first.trim_start().starts_with('|') {
        markdown(&text)
    } else if text.contains('\t') {
        tsv(&text)
    } else {
        Vec::new()
    }
}

/// Pick the clipboard representation that parses best. Notion (via
/// Chromium) offers Markdown as text/plain and a broken HTML table where
/// multi-line cells are split into extra rows, so neither format can be
/// trusted blindly. Ties go to the earlier candidate.
pub fn best_source(candidates: &[String], today: NaiveDate) -> Option<String> {
    candidates
        .iter()
        .map(|c| {
            let p = parse(c, today);
            (p.rows.len() as i64 * 2 - p.skipped.len() as i64, c)
        })
        .filter(|(score, _)| *score > 0)
        .rev()
        .max_by_key(|(score, _)| *score)
        .map(|(_, c)| c.clone())
}

// ---------------------------------------------------------------- Markdown

fn markdown(text: &str) -> Table {
    let mut rows = Vec::new();
    let mut cur = String::new();
    let mut ncols = 0;
    for line in text.lines() {
        if cur.is_empty() {
            if !line.trim_start().starts_with('|') {
                continue;
            }
            cur.push_str(line.trim());
        } else {
            // A cell continued on the next line (Notion keeps raw newlines).
            cur.push('\n');
            cur.push_str(line.trim());
        }
        let pipes = count_pipes(&cur);
        let closed = cur.ends_with('|') && !cur.ends_with("\\|");
        if closed && pipes >= 2 && (ncols == 0 || pipes > ncols) {
            let cells = split_md(&cur);
            cur.clear();
            if cells.iter().all(|c| is_separator(c)) {
                continue;
            }
            if ncols == 0 {
                ncols = cells.len();
            }
            rows.push(cells);
        }
    }
    if !cur.trim().is_empty() {
        rows.push(split_md(&cur));
    }
    rows
}

fn count_pipes(s: &str) -> usize {
    let mut prev = ' ';
    s.chars()
        .filter(|&c| {
            let hit = c == '|' && prev != '\\';
            prev = c;
            hit
        })
        .count()
}

fn split_md(row: &str) -> Vec<String> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row.strip_suffix('|').unwrap_or(row);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(clean(&std::mem::take(&mut cell))),
            _ => cell.push(c),
        }
    }
    cells.push(clean(&cell));
    cells
}

fn is_separator(cell: &str) -> bool {
    cell.contains('-') && cell.chars().all(|c| matches!(c, '-' | ':' | ' '))
}

// ---------------------------------------------------------------- TSV

fn tsv(text: &str) -> Table {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut at_start = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => {
                in_quotes = true;
                at_start = false;
            }
            '\t' => {
                row.push(clean(&std::mem::take(&mut field)));
                at_start = true;
            }
            '\n' => {
                row.push(clean(&std::mem::take(&mut field)));
                rows.push(std::mem::take(&mut row));
                at_start = true;
            }
            _ => {
                field.push(c);
                at_start = false;
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(clean(&field));
        rows.push(row);
    }
    rows
}

/// Serialize back to TSV (quoting cells with tabs, newlines or quotes), so an
/// HTML clipboard can be shown as editable text and re-parsed identically.
pub fn to_tsv(table: &Table) -> String {
    table
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| {
                    if c.contains(['\t', '\n', '"']) {
                        format!("\"{}\"", c.replace('"', "\"\""))
                    } else {
                        c.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------- HTML

/// Extract the first table's rows from clipboard HTML. Line breaks inside
/// cells (<br>, paragraphs, list items) are kept as newlines.
pub fn html_table(html: &str) -> Table {
    let mut rows = Vec::new();
    let mut row: Option<Vec<String>> = None;
    let mut cell: Option<String> = None;
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if let Some(c) = cell.as_mut() {
                c.push_str(rest);
            }
            break;
        };
        if let Some(c) = cell.as_mut() {
            c.push_str(&rest[..lt]);
        }
        let Some(gt) = rest[lt..].find('>') else { break };
        let tag = rest[lt + 1..lt + gt].trim().to_ascii_lowercase();
        rest = &rest[lt + gt + 1..];

        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        match (name.as_str(), closing) {
            ("tr", false) => row = Some(Vec::new()),
            ("tr", true) => {
                if let Some(r) = row.take() {
                    rows.push(r);
                }
            }
            ("td" | "th", false) => cell = Some(String::new()),
            ("td" | "th", true) => {
                if let (Some(c), Some(r)) = (cell.take(), row.as_mut()) {
                    r.push(clean(&decode_entities(&c)));
                }
            }
            ("br", _) | ("p" | "div" | "li", true) => {
                if let Some(c) = cell.as_mut() {
                    c.push('\n');
                }
            }
            ("table", true) if !rows.is_empty() => break,
            _ => {}
        }
    }
    rows
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest[..rest.len().min(10)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Normalise a cell: <br> → newline, trim each line, drop blank lines.
fn clean(s: &str) -> String {
    let s = s
        .replace('\u{a0}', " ")
        .replace("<br />", "\n")
        .replace("<br/>", "\n")
        .replace("<br>", "\n");
    s.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

// ---------------------------------------------------------------- Dates

#[derive(Default, Clone, Copy, Debug)]
struct Part {
    day: Option<u32>,
    month: Option<u32>,
    year: Option<i32>,
}

const MONTHS: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october",
    "november", "december",
];
const WEEKDAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/// Parse "5 Oct", "Mon 5 Oct", "Oct 5, 2026", "5–11 Oct", "26 Oct – 1 Nov",
/// "October 5, 2026 → October 11, 2026", "2026-10-05". Years that are left
/// out are chosen to put the date nearest to `anchor`.
pub fn parse_range(s: &str, anchor: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let iso = iso_dates(s);
    let (start, end) = if !iso.is_empty() {
        (iso[0], *iso.last().unwrap())
    } else {
        let parts = split_range(s);
        match parts.as_slice() {
            [one] => {
                let d = resolve(parse_part(one)?, anchor)?;
                (d, d)
            }
            [a, b] => {
                let (mut a, mut b) = (parse_part(a)?, parse_part(b)?);
                a.month = a.month.or(b.month);
                b.month = b.month.or(a.month);
                match (a.year, b.year) {
                    (None, Some(_)) => {
                        // "26 Dec – 2 Jan 2027": start takes the end's year, minus one if that's after the end.
                        let end = resolve(b, anchor)?;
                        a.year = b.year;
                        let mut start = resolve(a, anchor)?;
                        if start > end {
                            a.year = a.year.map(|y| y - 1);
                            start = resolve(a, anchor)?;
                        }
                        (start, end)
                    }
                    _ => {
                        let start = resolve(a, anchor)?;
                        b.year = b.year.or(Some(start.year()));
                        let mut end = resolve(b, anchor)?;
                        if end < start {
                            b.year = b.year.map(|y| y + 1);
                            end = resolve(b, anchor)?;
                        }
                        (start, end)
                    }
                }
            }
            _ => return None,
        }
    };
    (start <= end && (end - start).num_days() <= 366).then_some((start, end))
}

fn iso_dates(s: &str) -> Vec<NaiveDate> {
    s.split(|c: char| c.is_whitespace() || "–—→,|".contains(c))
        .filter_map(|w| NaiveDate::parse_from_str(w.trim(), "%Y-%m-%d").ok())
        .collect()
}

fn split_range(s: &str) -> Vec<String> {
    let s = s
        .replace(['–', '—', '→'], "|")
        .replace(" to ", "|")
        .replace(" - ", "|");
    // "5-11 Oct": a hyphen between two numbers is a range too.
    let chars: Vec<char> = s.chars().collect();
    let s: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let between_digits = i > 0
                && i + 1 < chars.len()
                && chars[i - 1].is_ascii_digit()
                && chars[i + 1].is_ascii_digit();
            if c == '-' && between_digits { '|' } else { c }
        })
        .collect();
    s.split('|').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
}

fn parse_part(s: &str) -> Option<Part> {
    let mut p = Part::default();
    let mut any = false;
    for tok in tokens(s) {
        if tok.chars().all(|c| c.is_ascii_digit()) {
            let n: u32 = tok.parse().ok()?;
            if tok.len() == 4 {
                p.year = Some(n as i32);
            } else if (1..=31).contains(&n) && p.day.is_none() {
                p.day = Some(n);
            } else {
                return None;
            }
        } else {
            let t = tok.to_lowercase();
            if t.len() >= 3 && let Some(m) = MONTHS.iter().position(|m| m.starts_with(&t)) {
                p.month = Some(m as u32 + 1);
            } else if !(t.len() >= 2 && WEEKDAYS.iter().any(|w| w.starts_with(&t))
                || matches!(t.as_str(), "st" | "nd" | "rd" | "th" | "of"))
            {
                return None;
            }
        }
        any = true;
    }
    any.then_some(p)
}

/// Alphanumeric runs, with digits and letters split apart ("5th" → "5", "th").
fn tokens(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let mut cur = String::new();
        for c in word.chars() {
            if let Some(last) = cur.chars().last() && last.is_ascii_digit() != c.is_ascii_digit() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
        out.push(cur);
    }
    out
}

fn resolve(p: Part, anchor: NaiveDate) -> Option<NaiveDate> {
    let (day, month) = (p.day?, p.month?);
    match p.year {
        Some(y) => NaiveDate::from_ymd_opt(y, month, day),
        None => (anchor.year() - 1..=anchor.year() + 1)
            .filter_map(|y| NaiveDate::from_ymd_opt(y, month, day))
            .min_by_key(|d| (*d - anchor).num_days().abs()),
    }
}

// ---------------------------------------------------------------- Table → rows

const TASK_WORDS: [&str; 10] = ["task", "topic", "todo", "to-do", "title", "name", "goal", "plan", "focus", "what"];

pub fn plan_from_table(table: &Table, today: NaiveDate) -> Parsed {
    let mut out = Parsed::default();
    let ncols = table.iter().map(Vec::len).max().unwrap_or(0);
    if ncols == 0 {
        return out;
    }
    let cell = |r: &[String], c: usize| r.get(c).map(String::as_str).unwrap_or("").trim().to_string();

    // A first row with no dates in it is a header.
    let has_header = !table[0].iter().any(|c| parse_range(c, today).is_some());
    let headers: Vec<String> = (0..ncols)
        .map(|c| if has_header { cell(&table[0], c) } else { String::new() })
        .collect();
    let lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    let body = &table[usize::from(has_header)..];

    let date_hits = |c: usize| body.iter().filter(|r| parse_range(&cell(r, c), today).is_some()).count();
    let by_name = (0..ncols).find(|&c| ["date", "day", "when"].iter().any(|k| lower[c].contains(k)) && date_hits(c) > 0);
    let Some(date_col) = by_name.or_else(|| (0..ncols).filter(|&c| date_hits(c) > 0).max_by_key(|&c| date_hits(c)))
    else {
        out.skipped.push("No date column found — add one with dates like “5 Oct” or “12–18 Oct”.".into());
        return out;
    };
    let week_col = (0..ncols).find(|&c| c != date_col && lower[c].contains("week"));
    let others: Vec<usize> = (0..ncols).filter(|&c| c != date_col && Some(c) != week_col).collect();
    let task_col = others
        .iter()
        .copied()
        .find(|&c| TASK_WORDS.iter().any(|k| lower[c].contains(k)))
        .or(others.first().copied());
    let note_cols: Vec<usize> = others.iter().copied().filter(|&c| Some(c) != task_col).collect();
    out.date_column = Some(headers[date_col].clone());
    out.task_column = task_col.map(|c| headers[c].clone());

    let mut anchor = today;
    for r in body {
        if r.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let date_text = cell(r, date_col);
        let Some((start, end)) = parse_range(&date_text, anchor) else {
            out.skipped.push(if date_text.is_empty() {
                let what = r.iter().map(|c| c.trim()).find(|c| !c.is_empty()).unwrap_or("");
                format!("No date: “{}”", what.lines().next().unwrap_or(""))
            } else {
                format!("Couldn’t read the date “{date_text}”")
            });
            continue;
        };
        anchor = start;

        let mut task = task_col.map(|c| cell(r, c)).unwrap_or_default();
        if let Some(w) = week_col.map(|c| cell(r, c)).filter(|w| !w.is_empty()) {
            let label = if w.starts_with(|c: char| c.is_ascii_digit()) { format!("Week {w}") } else { w };
            task = if task.is_empty() { label } else { format!("{label} — {task}") };
        }
        let notes = note_cols
            .iter()
            .filter_map(|&c| {
                let v = cell(r, c);
                (!v.is_empty()).then(|| if headers[c].is_empty() { v } else { format!("{}: {v}", headers[c]) })
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.rows.push(ImportedRow { start, end, task, notes });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    const TODAY: (i32, u32, u32) = (2026, 10, 4);
    fn today() -> NaiveDate {
        d(TODAY.0, TODAY.1, TODAY.2)
    }

    #[test]
    fn dates() {
        let t = today();
        assert_eq!(parse_range("5 Oct", t), Some((d(2026, 10, 5), d(2026, 10, 5))));
        assert_eq!(parse_range("Mon 5 Oct", t), Some((d(2026, 10, 5), d(2026, 10, 5))));
        assert_eq!(parse_range("Oct 5th", t), Some((d(2026, 10, 5), d(2026, 10, 5))));
        assert_eq!(parse_range("5–11 Oct", t), Some((d(2026, 10, 5), d(2026, 10, 11))));
        assert_eq!(parse_range("5-11 Oct", t), Some((d(2026, 10, 5), d(2026, 10, 11))));
        assert_eq!(parse_range("Oct 5–11", t), Some((d(2026, 10, 5), d(2026, 10, 11))));
        assert_eq!(parse_range("26 Oct – 1 Nov", t), Some((d(2026, 10, 26), d(2026, 11, 1))));
        assert_eq!(parse_range("28 Dec – 3 Jan", t), Some((d(2026, 12, 28), d(2027, 1, 3))));
        assert_eq!(parse_range("28 Dec – 3 Jan 2027", t), Some((d(2026, 12, 28), d(2027, 1, 3))));
        assert_eq!(
            parse_range("October 5, 2026 → October 11, 2026", t),
            Some((d(2026, 10, 5), d(2026, 10, 11)))
        );
        assert_eq!(parse_range("2026-10-05", t), Some((d(2026, 10, 5), d(2026, 10, 5))));
        assert_eq!(parse_range("2026-10-05 → 2026-10-11", t), Some((d(2026, 10, 5), d(2026, 10, 11))));
        assert_eq!(parse_range("12 Jan", t), Some((d(2027, 1, 12), d(2027, 1, 12))));
        assert_eq!(parse_range("Dates", t), None);
        assert_eq!(parse_range("1", t), None);
        assert_eq!(parse_range("Stack (5)", t), None);
        assert_eq!(parse_range("31 Feb", t), None);
    }

    #[test]
    fn notion_markdown_with_multiline_cells() {
        let p = parse(include_str!("../tests/fixtures/notion_plan.md"), today());
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);
        assert_eq!(p.date_column.as_deref(), Some("Dates"));
        assert_eq!(p.task_column.as_deref(), Some("Topics"));
        assert_eq!(p.rows.iter().filter(|r| !r.is_day()).count(), 12);
        assert_eq!(p.rows.iter().filter(|r| r.is_day()).count(), 7);

        let w1 = &p.rows[0];
        assert_eq!((w1.start, w1.end), (d(2026, 10, 5), d(2026, 10, 11)));
        assert!(w1.task.starts_with("Week 1 — Re-solve your 47 solved problems:\nArrays & Hashing (9),"));
        assert!(w1.task.ends_with("Adv Graph (1)"));
        assert_eq!(
            w1.notes,
            "Patterns to own: Hashmap counting, prefix and suffix products, two pointers on sorted input\n\
             Landmark problems: Product of Array Except Self, Longest Consecutive Sequence, 3Sum, Trapping Rain Water"
        );

        let day = &p.rows[1];
        assert_eq!((day.start, day.end), (d(2026, 10, 5), d(2026, 10, 5)));
        assert_eq!(day.task, "Arrays & Hashing (7)");
        assert_eq!(day.notes, "");

        let w4 = p.rows.iter().find(|r| r.task.starts_with("Week 4")).unwrap();
        assert_eq!((w4.start, w4.end), (d(2026, 10, 26), d(2026, 11, 1)));
        let w12 = p.rows.last().unwrap();
        assert_eq!((w12.start, w12.end), (d(2026, 12, 21), d(2026, 12, 27)));
    }

    #[test]
    fn tsv_round_trip_and_quotes() {
        let tsv_text = "Date\tTask\tNotes\n5 Oct\t\"Line one\nLine \"\"two\"\"\"\tnote\n6 Oct\tPlain\t\n";
        let p = parse(tsv_text, today());
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.rows[0].task, "Line one\nLine \"two\"");
        assert_eq!(p.rows[0].notes, "Notes: note");
        let again = parse(&to_tsv(&read_table(tsv_text)), today());
        assert_eq!(again.rows, p.rows);
    }

    #[test]
    fn html_from_clipboard() {
        let html = "<meta charset='utf-8'><table><tr><th>Dates</th><th>Topics</th></tr>\
                    <tr><td>5&nbsp;Oct</td><td><p>Arrays &amp; Hashing</p><p>Two pointers</p></td></tr>\
                    <tr><td>12–18 Oct</td><td>Stack<br>Binary Search</td></tr></table>";
        let table = html_table(html);
        assert_eq!(table.len(), 3);
        let p = plan_from_table(&table, today());
        assert_eq!(p.rows[0].task, "Arrays & Hashing\nTwo pointers");
        assert_eq!(p.rows[1].task, "Stack\nBinary Search");
        assert_eq!((p.rows[1].start, p.rows[1].end), (d(2026, 10, 12), d(2026, 10, 18)));
    }

    #[test]
    fn prefers_notion_markdown_over_its_broken_html() {
        let md = include_str!("../tests/fixtures/notion_plan.md").to_string();
        let html = to_tsv(&html_table(include_str!("../tests/fixtures/notion_chromium_clipboard.html")));
        assert!(!parse(&html, today()).skipped.is_empty(), "fixture HTML is the broken kind");
        assert_eq!(best_source(&[md.clone(), html.clone()], today()), Some(md.clone()));
        assert_eq!(best_source(&[html.clone(), md.clone()], today()), Some(md));
        assert_eq!(best_source(&["just words".into()], today()), None);
    }

    #[test]
    fn rows_without_dates_are_reported() {
        let p = parse("| Date | Task |\n|---|---|\n| 5 Oct | A |\n| | Orphan |\n| someday | B |\n", today());
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.skipped, vec!["No date: “Orphan”", "Couldn’t read the date “someday”"]);
    }

    #[test]
    fn headerless_table_finds_date_column() {
        let p = parse("Write tests\t5 Oct\nShip it\t6 Oct\n", today());
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.rows[1].task, "Ship it");
    }
}
