package dev.vamsi.planner.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDate

class ModelTest {
    private fun d(day: Int) = LocalDate.of(2026, 10, day)

    @Test
    fun goalsOrderedByPriority() {
        var s = Store()
        s = s.newGoal("none").first
        val (s2, b) = s.newGoal("p3"); s = s2
        val (s3, c) = s.newGoal("p1"); s = s3
        s = s.updateGoal(b) { it.copy(priority = 3) }.updateGoal(c) { it.copy(priority = 1) }
        assertEquals(listOf("p1", "p3", "none"), s.goalOrder().map { it.name })
    }

    @Test
    fun slotsMergeIntoBlocksAndIgnoreDeletedRows() {
        var (s, g) = Store().newGoal("g")
        val (s1, r1) = s.addRow(g, d(5), d(5)); s = s1
        val (s2, r2) = s.addRow(g, d(5), d(5)); s = s2
        for (slot in 18..20) s = s.setSlot(d(5), slot, r1.id)
        s = s.setSlot(d(5), 21, r2.id).setSlot(d(5), 30, r1.id).setSlot(d(5), 40, 999)
        assertEquals(
            listOf(Block(18, 3, r1.id), Block(21, 1, r2.id), Block(30, 1, r1.id)),
            s.blocks(d(5)),
        )
        assertEquals("09:00–10:30", s.blocks(d(5))[0].label)
        s = s.setSlot(d(5), 21, null).unscheduleRow(r1.id).unscheduleRow(999)
        assertTrue(s.schedule.isEmpty())
    }

    /** The Android app reads and writes the desktop app's JSON. */
    @Test
    fun readsDesktopJsonAndRoundTrips() {
        val desktop = """
            {"goals":[{"id":1,"name":"Career","icon":"💼","priority":1,"date_width":140,"task_ratio":0.6,
              "rows":[{"id":2,"start":"2026-10-05","end":"2026-10-11","task":"Week 1","notes":"n","completed_on":null},
                      {"id":3,"start":"2026-10-05","end":"2026-10-05","task":"Arrays","notes":"","completed_on":"2026-10-05"}]}],
             "next_id":3,"schedule":{"2026-10-05":{"18":3,"19":3}}}
        """.trimIndent()
        val s = Store.fromJson(desktop)
        assertEquals(2, s.goals[0].rows.size)
        assertEquals(d(5), s.goals[0].rows[1].completedOn)
        assertEquals(listOf(Block(18, 2, 3)), s.blocks(d(5)))
        assertEquals(140, s.goals[0].dateWidth)
        assertEquals(s, Store.fromJson(s.toJson()))
    }

    @Test
    fun migratesV1Cells() {
        val v1 = """{"goals":[{"id":1,"name":"DSA","columns":["Task","Notes"],"color":[1,2,3],
            "rows":[{"id":2,"start":"2026-10-05","end":"2026-10-05","cells":["Do it","later",""]}]}],"next_id":2}"""
        val row = Store.fromJson(v1).goals[0].rows[0]
        assertEquals("Do it", row.task)
        assertEquals("later", row.notes)
    }
}
