package dev.vamsi.planner.data

import java.time.LocalDate
import java.time.temporal.ChronoUnit
import kotlin.math.abs

/*
 * Turn a pasted table into plan rows — a port of the desktop import.rs.
 *
 * Accepts what Notion (and most apps) put on the clipboard or share sheet:
 * Markdown pipe tables (cells may span lines), tab-separated text with
 * "quoted" multi-line cells, and HTML tables. Column roles (Date / Week /
 * Task / notes) are guessed from the header, else from the content.
 */

typealias Table = List<List<String>>

data class ImportedRow(val start: LocalDate, val end: LocalDate, val task: String, val notes: String) {
    val isDay get() = start == end
}

data class Parsed(
    val rows: List<ImportedRow> = emptyList(),
    val skipped: List<String> = emptyList(),
    val dateColumn: String? = null,
    val taskColumn: String? = null,
)

object NotionImport {

    fun parse(text: String, today: LocalDate): Parsed = planFromTable(readTable(text), today)

    /**
     * Pick the clipboard representation that parses best. Notion (via
     * Chromium) offers correct Markdown as text and a broken HTML table where
     * multi-line cells become extra rows. Ties go to the earlier candidate.
     */
    fun bestSource(candidates: List<String>, today: LocalDate): String? {
        var best: String? = null
        var bestScore = 0
        for (c in candidates) {
            val p = parse(c, today)
            val score = p.rows.size * 2 - p.skipped.size
            if (score > bestScore) {
                best = c
                bestScore = score
            }
        }
        return best
    }

    fun readTable(input: String): Table {
        val text = input.replace("\r\n", "\n")
        val first = text.lineSequence().firstOrNull { it.isNotBlank() } ?: ""
        return when {
            first.trimStart().startsWith("|") -> markdown(text)
            text.contains('\t') -> tsv(text)
            else -> emptyList()
        }
    }

    // ---------------------------------------------------------------- Markdown

    private fun markdown(text: String): Table {
        val rows = mutableListOf<List<String>>()
        val cur = StringBuilder()
        var ncols = 0
        for (line in text.lines()) {
            if (cur.isEmpty()) {
                if (!line.trimStart().startsWith("|")) continue
                cur.append(line.trim())
            } else {
                // A cell continued on the next line (Notion keeps raw newlines).
                cur.append('\n').append(line.trim())
            }
            val pipes = countPipes(cur)
            val closed = cur.endsWith("|") && !cur.endsWith("\\|")
            if (closed && pipes >= 2 && (ncols == 0 || pipes > ncols)) {
                val cells = splitMd(cur.toString())
                cur.clear()
                if (cells.all(::isSeparator)) continue
                if (ncols == 0) ncols = cells.size
                rows += cells
            }
        }
        if (cur.isNotBlank()) rows += splitMd(cur.toString())
        return rows
    }

    private fun countPipes(s: CharSequence): Int {
        var n = 0
        for (i in s.indices) if (s[i] == '|' && (i == 0 || s[i - 1] != '\\')) n++
        return n
    }

    private fun splitMd(row: String): List<String> {
        var r = row.trim().removePrefix("|")
        r = r.removeSuffix("|")
        val cells = mutableListOf<String>()
        val cell = StringBuilder()
        var i = 0
        while (i < r.length) {
            val c = r[i]
            when {
                c == '\\' && i + 1 < r.length && r[i + 1] == '|' -> { cell.append('|'); i++ }
                c == '|' -> { cells += clean(cell.toString()); cell.clear() }
                else -> cell.append(c)
            }
            i++
        }
        cells += clean(cell.toString())
        return cells
    }

    private fun isSeparator(cell: String) = cell.contains('-') && cell.all { it == '-' || it == ':' || it == ' ' }

    // ---------------------------------------------------------------- TSV

    private fun tsv(text: String): Table {
        val rows = mutableListOf<List<String>>()
        var row = mutableListOf<String>()
        val field = StringBuilder()
        var inQuotes = false
        var atStart = true
        var i = 0
        while (i < text.length) {
            val c = text[i]
            if (inQuotes) {
                if (c == '"') {
                    if (i + 1 < text.length && text[i + 1] == '"') { field.append('"'); i++ } else inQuotes = false
                } else field.append(c)
                i++
                continue
            }
            when (c) {
                '"' -> if (atStart) { inQuotes = true; atStart = false } else { field.append(c); atStart = false }
                '\t' -> { row += clean(field.toString()); field.clear(); atStart = true }
                '\n' -> { row += clean(field.toString()); field.clear(); rows += row; row = mutableListOf(); atStart = true }
                else -> { field.append(c); atStart = false }
            }
            i++
        }
        if (field.isNotEmpty() || row.isNotEmpty()) { row += clean(field.toString()); rows += row }
        return rows
    }

    /** Serialize back to TSV (quoting cells with tabs, newlines or quotes). */
    fun toTsv(table: Table): String = table.joinToString("\n") { row ->
        row.joinToString("\t") { c ->
            if (c.any { it == '\t' || it == '\n' || it == '"' }) "\"" + c.replace("\"", "\"\"") + "\"" else c
        }
    }

    // ---------------------------------------------------------------- HTML

    /** Rows of the first table in clipboard HTML; <br>, </p>, </li> become newlines. */
    fun htmlTable(html: String): Table {
        val rows = mutableListOf<List<String>>()
        var row: MutableList<String>? = null
        var cell: StringBuilder? = null
        var rest = html
        while (rest.isNotEmpty()) {
            val lt = rest.indexOf('<')
            if (lt < 0) { cell?.append(rest); break }
            cell?.append(rest, 0, lt)
            val gt = rest.indexOf('>', lt)
            if (gt < 0) break
            val tag = rest.substring(lt + 1, gt).trim().lowercase()
            rest = rest.substring(gt + 1)
            val closing = tag.startsWith("/")
            val name = tag.trimStart('/').takeWhile { it.isLetterOrDigit() }
            when {
                name == "tr" && !closing -> row = mutableListOf()
                name == "tr" && closing -> { row?.let { rows += it }; row = null }
                (name == "td" || name == "th") && !closing -> cell = StringBuilder()
                (name == "td" || name == "th") && closing -> {
                    val c = cell
                    if (c != null && row != null) row.add(clean(decodeEntities(c.toString())))
                    cell = null
                }
                name == "br" || (closing && (name == "p" || name == "div" || name == "li")) -> cell?.append('\n')
                name == "table" && closing && rows.isNotEmpty() -> break
            }
        }
        return rows
    }

    private fun decodeEntities(s: String): String {
        val out = StringBuilder()
        var i = 0
        while (i < s.length) {
            val amp = s.indexOf('&', i)
            if (amp < 0) { out.append(s, i, s.length); break }
            out.append(s, i, amp)
            val semi = s.indexOf(';', amp)
            val entity = if (semi in amp + 1..amp + 9) s.substring(amp + 1, semi) else null
            val decoded: Char? = when {
                entity == null -> null
                entity == "amp" -> '&'
                entity == "lt" -> '<'
                entity == "gt" -> '>'
                entity == "quot" -> '"'
                entity == "apos" || entity == "#39" -> '\''
                entity == "nbsp" -> ' '
                entity.startsWith("#x") -> entity.drop(2).toIntOrNull(16)?.toChar()
                entity.startsWith("#") -> entity.drop(1).toIntOrNull()?.toChar()
                else -> null
            }
            if (decoded != null) { out.append(decoded); i = semi + 1 } else { out.append('&'); i = amp + 1 }
        }
        return out.toString()
    }

    /** Normalise a cell: <br> → newline, trim each line, drop blank lines. */
    private fun clean(s: String): String = s.replace(' ', ' ')
        .replace("<br />", "\n").replace("<br/>", "\n").replace("<br>", "\n")
        .lines().map { it.trim() }.filter { it.isNotEmpty() }.joinToString("\n")

    // ---------------------------------------------------------------- Dates

    private data class Part(var day: Int? = null, var month: Int? = null, var year: Int? = null)

    private val months = listOf("january", "february", "march", "april", "may", "june", "july", "august",
        "september", "october", "november", "december")
    private val weekdays = listOf("monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday")

    /**
     * "5 Oct", "Mon 5 Oct", "Oct 5th", "5–11 Oct", "26 Oct – 1 Nov",
     * "October 5, 2026 → October 11, 2026", "2026-10-05". Missing years are
     * chosen to put the date nearest [anchor].
     */
    fun parseRange(input: String, anchor: LocalDate): Pair<LocalDate, LocalDate>? {
        val s = input.trim()
        if (s.isEmpty()) return null
        val iso = isoDates(s)
        val (start, end) = if (iso.isNotEmpty()) iso.first() to iso.last() else {
            val parts = splitRange(s)
            when (parts.size) {
                1 -> resolve(parsePart(parts[0]) ?: return null, anchor)?.let { it to it } ?: return null
                2 -> {
                    val a = parsePart(parts[0]) ?: return null
                    val b = parsePart(parts[1]) ?: return null
                    if (a.month == null) a.month = b.month
                    if (b.month == null) b.month = a.month
                    if (a.year == null && b.year != null) {
                        // "26 Dec – 2 Jan 2027": start takes the end's year, minus one if after the end.
                        val end = resolve(b, anchor) ?: return null
                        a.year = b.year
                        var start = resolve(a, anchor) ?: return null
                        if (start.isAfter(end)) { a.year = a.year!! - 1; start = resolve(a, anchor) ?: return null }
                        start to end
                    } else {
                        val start = resolve(a, anchor) ?: return null
                        if (b.year == null) b.year = start.year
                        var end = resolve(b, anchor) ?: return null
                        if (end.isBefore(start)) { b.year = b.year!! + 1; end = resolve(b, anchor) ?: return null }
                        start to end
                    }
                }
                else -> return null
            }
        }
        return if (!start.isAfter(end) && ChronoUnit.DAYS.between(start, end) <= 366) start to end else null
    }

    private fun isoDates(s: String): List<LocalDate> = s.split(Regex("[\\s–—→,|]+"))
        .mapNotNull { w -> if (Regex("\\d{4}-\\d{2}-\\d{2}").matches(w)) runCatching { LocalDate.parse(w) }.getOrNull() else null }

    private fun splitRange(s0: String): List<String> {
        var s = s0.replace('–', '|').replace('—', '|').replace('→', '|').replace(" to ", "|").replace(" - ", "|")
        // "5-11 Oct": a hyphen between two numbers is a range too.
        s = buildString {
            s.forEachIndexed { i, c ->
                val between = i > 0 && i + 1 < s.length && s[i - 1].isDigit() && s[i + 1].isDigit()
                append(if (c == '-' && between) '|' else c)
            }
        }
        return s.split('|').map { it.trim() }.filter { it.isNotEmpty() }
    }

    private fun parsePart(s: String): Part? {
        val p = Part()
        var any = false
        for (tok in tokens(s)) {
            if (tok.all { it.isDigit() }) {
                val n = tok.toIntOrNull() ?: return null
                when {
                    tok.length == 4 -> p.year = n
                    n in 1..31 && p.day == null -> p.day = n
                    else -> return null
                }
            } else {
                val t = tok.lowercase()
                val m = if (t.length >= 3) months.indexOfFirst { it.startsWith(t) } else -1
                when {
                    m >= 0 -> p.month = m + 1
                    t.length >= 2 && weekdays.any { it.startsWith(t) } -> {}
                    t in setOf("st", "nd", "rd", "th", "of") -> {}
                    else -> return null
                }
            }
            any = true
        }
        return if (any) p else null
    }

    /** Alphanumeric runs, digits and letters split apart ("5th" → "5", "th"). */
    private fun tokens(s: String): List<String> {
        val out = mutableListOf<String>()
        for (word in s.split(Regex("[^\\p{L}\\p{N}]+")).filter { it.isNotEmpty() }) {
            val cur = StringBuilder()
            for (c in word) {
                if (cur.isNotEmpty() && cur.last().isDigit() != c.isDigit()) { out += cur.toString(); cur.clear() }
                cur.append(c)
            }
            out += cur.toString()
        }
        return out
    }

    private fun resolve(p: Part, anchor: LocalDate): LocalDate? {
        val day = p.day ?: return null
        val month = p.month ?: return null
        p.year?.let { return runCatching { LocalDate.of(it, month, day) }.getOrNull() }
        return (anchor.year - 1..anchor.year + 1)
            .mapNotNull { y -> runCatching { LocalDate.of(y, month, day) }.getOrNull() }
            .minByOrNull { abs(ChronoUnit.DAYS.between(anchor, it)) }
    }

    // ---------------------------------------------------------------- Table → rows

    private val taskWords = listOf("task", "topic", "todo", "to-do", "title", "name", "goal", "plan", "focus", "what")

    fun planFromTable(table: Table, today: LocalDate): Parsed {
        val ncols = table.maxOfOrNull { it.size } ?: 0
        if (ncols == 0) return Parsed()
        fun cell(r: List<String>, c: Int) = r.getOrNull(c)?.trim().orEmpty()

        // A first row with no dates in it is a header.
        val hasHeader = table[0].none { parseRange(it, today) != null }
        val headers = (0 until ncols).map { if (hasHeader) cell(table[0], it) else "" }
        val lower = headers.map { it.lowercase() }
        val body = if (hasHeader) table.drop(1) else table

        fun hits(c: Int) = body.count { parseRange(cell(it, c), today) != null }
        val byName = (0 until ncols).firstOrNull { c -> listOf("date", "day", "when").any { lower[c].contains(it) } && hits(c) > 0 }
        val dateCol = byName ?: (0 until ncols).filter { hits(it) > 0 }.maxByOrNull { hits(it) }
            ?: return Parsed(skipped = listOf("No date column found — add one with dates like “5 Oct” or “12–18 Oct”."))
        val weekCol = (0 until ncols).firstOrNull { it != dateCol && lower[it].contains("week") }
        val others = (0 until ncols).filter { it != dateCol && it != weekCol }
        val taskCol = others.firstOrNull { c -> taskWords.any { lower[c].contains(it) } } ?: others.firstOrNull()
        val noteCols = others.filter { it != taskCol }

        val rows = mutableListOf<ImportedRow>()
        val skipped = mutableListOf<String>()
        var anchor = today
        for (r in body) {
            if (r.all { it.isBlank() }) continue
            val dateText = cell(r, dateCol)
            val range = parseRange(dateText, anchor)
            if (range == null) {
                skipped += if (dateText.isEmpty()) {
                    "No date: “${r.map { it.trim() }.firstOrNull { it.isNotEmpty() }.orEmpty().lineSequence().first()}”"
                } else "Couldn’t read the date “$dateText”"
                continue
            }
            anchor = range.first
            var task = taskCol?.let { cell(r, it) }.orEmpty()
            weekCol?.let { cell(r, it) }?.takeIf { it.isNotEmpty() }?.let { w ->
                val label = if (w.first().isDigit()) "Week $w" else w
                task = if (task.isEmpty()) label else "$label — $task"
            }
            val notes = noteCols.mapNotNull { c ->
                val v = cell(r, c)
                if (v.isEmpty()) null else if (headers[c].isEmpty()) v else "${headers[c]}: $v"
            }.joinToString("\n")
            rows += ImportedRow(range.first, range.second, task, notes)
        }
        return Parsed(rows, skipped, headers[dateCol], taskCol?.let { headers[it] })
    }
}
