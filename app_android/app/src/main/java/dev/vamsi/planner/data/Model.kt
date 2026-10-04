package dev.vamsi.planner.data

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import java.util.Locale

/*
 * Data model — a port of the desktop app's model.rs, reading and writing the
 * same JSON, so a data file can move between desktop and phone unchanged.
 *
 * A Goal owns a table of PlanRows. Each row covers a date range: one day is a
 * daily todo; a longer range (e.g. a week) is a focus shown on every day it
 * covers. The Today page is just a query over these rows.
 */

object DateSerializer : KSerializer<LocalDate> {
    override val descriptor = PrimitiveSerialDescriptor("LocalDate", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: LocalDate) = encoder.encodeString(value.toString())
    override fun deserialize(decoder: Decoder): LocalDate = LocalDate.parse(decoder.decodeString())
}

typealias Day = @Serializable(with = DateSerializer::class) LocalDate

@Serializable
data class PlanRow(
    val id: Long,
    val start: Day,
    val end: Day,
    val task: String = "",
    val notes: String = "",
    @SerialName("completed_on") val completedOn: Day? = null,
) {
    val isDay get() = start == end
    val done get() = completedOn != null
    fun covers(date: LocalDate) = !date.isBefore(start) && !date.isAfter(end)
    val days get() = ChronoUnit.DAYS.between(start, end) + 1
}

@Serializable
data class Goal(
    val id: Long,
    val name: String,
    val icon: String = "🎯",
    /** 1 (highest) … 4 (lowest); 0 = none. */
    val priority: Int = 0,
    // Desktop table layout; kept so it survives a round trip through the phone.
    @SerialName("date_width") val dateWidth: Int? = null,
    @SerialName("task_ratio") val taskRatio: Double? = null,
    val rows: List<PlanRow> = emptyList(),
) {
    /** Chronological, with a range listed before the days it contains. */
    fun sortedRows() = rows.sortedWith(compareBy<PlanRow> { it.start }.thenByDescending { it.end }.thenBy { it.id })

    /** First free date after the last day-row (or range-row), or [today]. */
    fun nextFreeDate(ranges: Boolean, today: LocalDate): LocalDate =
        rows.filter { it.isDay != ranges }.maxOfOrNull { it.end.plusDays(1) } ?: today
}

/** A run of consecutive half-hour slots given to one row. */
data class Block(val start: Int, val len: Int, val row: Long) {
    val label get() = "${slotTime(start)}–${slotTime(start + len)}"
}

const val SLOTS = 48

fun slotTime(slot: Int) = "%02d:%02d".format(slot / 2, if (slot % 2 == 1) 30 else 0)

/** Sort key for a goal priority: P1 first, "none" last. */
fun priorityRank(p: Int) = if (p == 0) 5 else p

data class AgendaGroup(val goal: Goal, val ranges: List<PlanRow>, val days: List<PlanRow>)

@Serializable
data class Store(
    val goals: List<Goal> = emptyList(),
    @SerialName("next_id") val nextId: Long = 0,
    /** date (YYYY-MM-DD) → half-hour slot (0..47) → row id. */
    val schedule: Map<String, Map<Int, Long>> = emptyMap(),
) {
    fun goal(id: Long) = goals.firstOrNull { it.id == id }

    fun row(rid: Long): Pair<Goal, PlanRow>? =
        goals.firstNotNullOfOrNull { g -> g.rows.firstOrNull { it.id == rid }?.let { g to it } }

    /** Goals by priority (P1 first, none last), otherwise in creation order. */
    fun goalOrder(): List<Goal> = goals.sortedBy { priorityRank(it.priority) }

    fun agenda(date: LocalDate): List<AgendaGroup> = goalOrder().mapNotNull { g ->
        val rows = g.sortedRows().filter { it.covers(date) }
        val (days, ranges) = rows.partition { it.isDay }
        if (rows.isEmpty()) null else AgendaGroup(g, ranges, days)
    }

    /** Unfinished single-day todos from before [date]. */
    fun overdue(date: LocalDate): List<Pair<Goal, PlanRow>> = goalOrder().flatMap { g ->
        g.sortedRows().filter { it.isDay && it.end.isBefore(date) && !it.done }.map { g to it }
    }

    /** (done, total) over the single-day todos on [date]. */
    fun progress(date: LocalDate): Pair<Int, Int> {
        val todos = goals.flatMap { it.rows }.filter { it.isDay && it.covers(date) }
        return todos.count { it.done } to todos.size
    }

    /** Slot → row for [date], ignoring rows that no longer exist. */
    fun slots(date: LocalDate): Map<Int, Long> =
        schedule[date.toString()].orEmpty().filterValues { row(it) != null }.toSortedMap()

    /** The day's slots merged into runs, in time order. */
    fun blocks(date: LocalDate): List<Block> {
        val out = mutableListOf<Block>()
        for ((slot, rid) in slots(date)) {
            val last = out.lastOrNull()
            if (last != null && last.row == rid && last.start + last.len == slot) {
                out[out.lastIndex] = last.copy(len = last.len + 1)
            } else {
                out += Block(slot, 1, rid)
            }
        }
        return out
    }

    // ---- updates (all return a new Store) ----

    fun withId(): Pair<Store, Long> = copy(nextId = nextId + 1) to nextId + 1

    fun newGoal(name: String, icon: String = "🎯"): Pair<Store, Long> {
        val (s, id) = withId()
        return s.copy(goals = s.goals + Goal(id = id, name = name, icon = icon)) to id
    }

    fun updateGoal(gid: Long, f: (Goal) -> Goal) = copy(goals = goals.map { if (it.id == gid) f(it) else it })

    fun updateRow(gid: Long, rid: Long, f: (PlanRow) -> PlanRow) =
        updateGoal(gid) { g -> g.copy(rows = g.rows.map { if (it.id == rid) f(it) else it }) }

    fun addRow(gid: Long, start: LocalDate, end: LocalDate, task: String = "", notes: String = ""): Pair<Store, PlanRow> {
        val (s, id) = withId()
        val row = PlanRow(id = id, start = start, end = end, task = task, notes = notes)
        return s.updateGoal(gid) { it.copy(rows = it.rows + row) } to row
    }

    fun removeRow(gid: Long, rid: Long) =
        updateGoal(gid) { g -> g.copy(rows = g.rows.filterNot { it.id == rid }) }.unscheduleRow(rid)

    fun removeGoal(gid: Long): Store {
        val ids = goal(gid)?.rows?.map { it.id }.orEmpty().toSet()
        return copy(goals = goals.filterNot { it.id == gid }).let { s -> ids.fold(s) { acc, id -> acc.unscheduleRow(id) } }
    }

    /** Assign (or with null, clear) one half-hour slot on [date]. */
    fun setSlot(date: LocalDate, slot: Int, row: Long?): Store {
        val key = date.toString()
        val day = schedule[key].orEmpty().toMutableMap()
        if (row == null) day.remove(slot) else day[slot] = row
        val all = schedule.toMutableMap()
        if (day.isEmpty()) all.remove(key) else all[key] = day
        return copy(schedule = all)
    }

    /** Forget a deleted row's time slots on every day. */
    fun unscheduleRow(rid: Long) = copy(
        schedule = schedule.mapValues { (_, day) -> day.filterValues { it != rid } }.filterValues { it.isNotEmpty() },
    )

    fun toJson(): String = json.encodeToString(serializer(), this)

    companion object {
        val json = Json {
            ignoreUnknownKeys = true
            encodeDefaults = true
            prettyPrint = true
        }

        fun fromJson(text: String): Store = migrate(json.decodeFromString(serializer(), text), text)

        /** v1 desktop files kept text in `cells`; fold them into task/notes. */
        private fun migrate(store: Store, text: String): Store {
            if (!text.contains("\"cells\"")) return store
            val raw = json.parseToJsonElement(text)
            val cells = mutableMapOf<Long, List<String>>()
            raw.jsonObjectOrNull()?.get("goals")?.jsonArrayOrNull()?.forEach { g ->
                g.jsonObjectOrNull()?.get("rows")?.jsonArrayOrNull()?.forEach { r ->
                    val o = r.jsonObjectOrNull() ?: return@forEach
                    val id = o["id"]?.primitiveContent()?.toLongOrNull() ?: return@forEach
                    val c = o["cells"]?.jsonArrayOrNull()?.mapNotNull { it.primitiveContent() } ?: return@forEach
                    cells[id] = c
                }
            }
            return store.copy(goals = store.goals.map { g ->
                g.copy(rows = g.rows.map { r ->
                    val c = cells[r.id]
                    if (c == null || c.isEmpty() || r.task.isNotEmpty()) r
                    else r.copy(task = c.first(), notes = c.drop(1).filter { it.isNotBlank() }.joinToString(" · "))
                })
            })
        }

        /** First-run example: the DSA plan from the Notion table. */
        fun sample(): Store {
            fun d(m: Int, day: Int) = LocalDate.of(2026, m, day)
            var (s, gid) = Store().newGoal("Career — DSA", "💼")
            val weeks = listOf(
                Triple(d(10, 5) to d(10, 11), "Week 1 — Re-solve your 47 solved problems",
                    "Patterns to own: hashmap counting, prefix/suffix products, two pointers\nLandmark problems: Product of Array Except Self, Longest Consecutive Sequence, 3Sum, Trapping Rain Water"),
                Triple(d(10, 12) to d(10, 18), "Week 2 — Stack (7), Binary Search (7)",
                    "Patterns to own: monotonic stack, bracket matching, bound templates, search on the answer\nLandmark problems: Daily Temperatures, Largest Rectangle in Histogram, Koko Eating Bananas"),
                Triple(d(10, 19) to d(10, 25), "Week 3 — Sliding Window (6), Linked List (11)",
                    "Patterns to own: fixed/variable windows, monotonic deque, fast & slow pointers\nLandmark problems: Minimum Window Substring, LRU Cache, Merge K Sorted Lists"),
            )
            for ((range, task, notes) in weeks) s = s.addRow(gid, range.first, range.second, task, notes).first
            listOf(
                5 to "Arrays & Hashing (7)", 6 to "Arrays & Hashing (2) + Two Pointers (5)",
                7 to "Stack (5) + Binary Search (2)", 8 to "Binary Search (3) + Sliding window (2) + Linked List (2)",
                9 to "Linked List (2) + Trees (4) + Backtracking (1)", 10 to "Backtracking (1) + Heap / Priority Queue (4) + Graphs (2)",
                11 to "Graphs (3) + DP (1) + Adv Graph (1)",
            ).forEach { (day, task) -> s = s.addRow(gid, d(10, day), d(10, day), task).first }
            s = s.updateGoal(gid) { it.copy(priority = 1) }
            return s
        }
    }
}

private fun kotlinx.serialization.json.JsonElement.jsonObjectOrNull() = this as? kotlinx.serialization.json.JsonObject
private fun kotlinx.serialization.json.JsonElement.jsonArrayOrNull() = this as? kotlinx.serialization.json.JsonArray
private fun kotlinx.serialization.json.JsonElement.primitiveContent() =
    (this as? kotlinx.serialization.json.JsonPrimitive)?.content

private val dayFmt = DateTimeFormatter.ofPattern("EEE d MMM", Locale.getDefault())
private val dFmt = DateTimeFormatter.ofPattern("d", Locale.getDefault())
private val dMonFmt = DateTimeFormatter.ofPattern("d MMM", Locale.getDefault())

/** "Mon 5 Oct" / "5–11 Oct" / "26 Oct – 1 Nov" */
fun fmtRange(start: LocalDate, end: LocalDate): String = when {
    start == end -> start.format(dayFmt)
    start.month == end.month -> "${start.format(dFmt)}–${end.format(dMonFmt)}"
    else -> "${start.format(dMonFmt)} – ${end.format(dMonFmt)}"
}

fun firstLine(s: String) = s.lineSequence().firstOrNull()?.trim().orEmpty()

/** 3 → "1h 30m". */
fun duration(slots: Int): String {
    val h = slots / 2
    val m = if (slots % 2 == 1) 30 else 0
    return when {
        h == 0 -> "${m}m"
        m == 0 -> "${h}h"
        else -> "${h}h ${m}m"
    }
}
