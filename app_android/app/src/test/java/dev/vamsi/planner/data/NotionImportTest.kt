package dev.vamsi.planner.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDate

class NotionImportTest {
    private fun d(y: Int, m: Int, day: Int) = LocalDate.of(y, m, day)
    private val today = d(2026, 10, 4)
    private fun fixture(name: String) = javaClass.classLoader!!.getResource(name)!!.readText()

    @Test
    fun dates() {
        val t = today
        fun r(s: String) = NotionImport.parseRange(s, t)
        assertEquals(d(2026, 10, 5) to d(2026, 10, 5), r("5 Oct"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 5), r("Mon 5 Oct"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 5), r("Oct 5th"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 11), r("5–11 Oct"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 11), r("5-11 Oct"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 11), r("Oct 5–11"))
        assertEquals(d(2026, 10, 26) to d(2026, 11, 1), r("26 Oct – 1 Nov"))
        assertEquals(d(2026, 12, 28) to d(2027, 1, 3), r("28 Dec – 3 Jan"))
        assertEquals(d(2026, 12, 28) to d(2027, 1, 3), r("28 Dec – 3 Jan 2027"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 11), r("October 5, 2026 → October 11, 2026"))
        assertEquals(d(2026, 10, 5) to d(2026, 10, 5), r("2026-10-05"))
        assertEquals(d(2027, 1, 12) to d(2027, 1, 12), r("12 Jan"))
        assertNull(r("Dates"))
        assertNull(r("1"))
        assertNull(r("Stack (5)"))
        assertNull(r("31 Feb"))
    }

    @Test
    fun notionMarkdownWithMultilineCells() {
        val p = NotionImport.parse(fixture("notion_plan.md"), today)
        assertTrue(p.skipped.toString(), p.skipped.isEmpty())
        assertEquals("Dates", p.dateColumn)
        assertEquals("Topics", p.taskColumn)
        assertEquals(12, p.rows.count { !it.isDay })
        assertEquals(7, p.rows.count { it.isDay })
        val w1 = p.rows[0]
        assertEquals(d(2026, 10, 5) to d(2026, 10, 11), w1.start to w1.end)
        assertTrue(w1.task.startsWith("Week 1 — Re-solve your 47 solved problems:\nArrays & Hashing (9),"))
        assertTrue(w1.task.endsWith("Adv Graph (1)"))
        assertEquals(
            "Patterns to own: Hashmap counting, prefix and suffix products, two pointers on sorted input\n" +
                "Landmark problems: Product of Array Except Self, Longest Consecutive Sequence, 3Sum, Trapping Rain Water",
            w1.notes,
        )
        assertEquals("Arrays & Hashing (7)", p.rows[1].task)
        val w12 = p.rows.last()
        assertEquals(d(2026, 12, 21) to d(2026, 12, 27), w12.start to w12.end)
    }

    @Test
    fun tsvRoundTripAndQuotes() {
        val tsv = "Date\tTask\tNotes\n5 Oct\t\"Line one\nLine \"\"two\"\"\"\tnote\n6 Oct\tPlain\t\n"
        val p = NotionImport.parse(tsv, today)
        assertEquals(2, p.rows.size)
        assertEquals("Line one\nLine \"two\"", p.rows[0].task)
        assertEquals("Notes: note", p.rows[0].notes)
        assertEquals(p.rows, NotionImport.parse(NotionImport.toTsv(NotionImport.readTable(tsv)), today).rows)
    }

    @Test
    fun htmlFromClipboard() {
        val html = "<meta charset='utf-8'><table><tr><th>Dates</th><th>Topics</th></tr>" +
            "<tr><td>5&nbsp;Oct</td><td><p>Arrays &amp; Hashing</p><p>Two pointers</p></td></tr>" +
            "<tr><td>12–18 Oct</td><td>Stack<br>Binary Search</td></tr></table>"
        val p = NotionImport.planFromTable(NotionImport.htmlTable(html), today)
        assertEquals("Arrays & Hashing\nTwo pointers", p.rows[0].task)
        assertEquals("Stack\nBinary Search", p.rows[1].task)
    }

    @Test
    fun prefersNotionMarkdownOverItsBrokenHtml() {
        val md = fixture("notion_plan.md")
        val html = NotionImport.toTsv(NotionImport.htmlTable(fixture("notion_chromium_clipboard.html")))
        assertTrue(NotionImport.parse(html, today).skipped.isNotEmpty())
        assertEquals(md, NotionImport.bestSource(listOf(md, html), today))
        assertEquals(md, NotionImport.bestSource(listOf(html, md), today))
        assertNull(NotionImport.bestSource(listOf("just words"), today))
    }

    @Test
    fun rowsWithoutDatesAreReported() {
        val p = NotionImport.parse("| Date | Task |\n|---|---|\n| 5 Oct | A |\n| | Orphan |\n| someday | B |\n", today)
        assertEquals(1, p.rows.size)
        assertEquals(listOf("No date: “Orphan”", "Couldn’t read the date “someday”"), p.skipped)
    }
}
