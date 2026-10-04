package dev.vamsi.planner.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.vamsi.planner.R
import dev.vamsi.planner.data.Block
import dev.vamsi.planner.data.SLOTS
import dev.vamsi.planner.data.duration
import dev.vamsi.planner.data.slotTime
import kotlinx.coroutines.delay
import java.time.LocalDate
import java.time.LocalTime
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.min
import kotlin.math.sin

/** Something schedulable on the day (a todo or a focus), with its colour. */
data class PlanItem(val rid: Long, val title: String, val goal: String, val color: Color)

private enum class Mode { Paint, EraseOwn, EraseAll }

/** The "Day plan": clock, task chips, schedule list. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun DayPlanPanel(vm: PlannerViewModel, date: LocalDate, items: List<PlanItem>, modifier: Modifier = Modifier) {
    val blocks = vm.store.blocks(date)
    val planned = blocks.sumOf { it.len }
    val unscheduled = items.count { i -> blocks.none { it.row == i.rid } }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("Day plan", style = MaterialTheme.typography.titleMedium)
        Text(
            duration(planned) + " planned" + if (unscheduled > 0) " · $unscheduled unscheduled" else "",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        DayClock(vm, date, items, Modifier.align(Alignment.CenterHorizontally).widthIn(max = 380.dp).fillMaxWidth())

        val brush = vm.brush
        Text(
            when (brush) {
                Brush.None -> "Pick a task below, then tap or drag around the clock to give it time."
                Brush.Eraser -> "Eraser: drag over slots to clear them."
                is Brush.Row -> "Painting “${items.firstOrNull { it.rid == brush.id }?.title ?: "this task"}”. Drag over its own slots to clear them."
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        if (items.isEmpty()) {
            Text("Nothing to plan for this day yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            fun timeFor(rid: Long) = blocks.filter { it.row == rid }.sumOf { it.len }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                // Unscheduled first (already in priority order), then scheduled.
                for (item in items.sortedBy { timeFor(it.rid) > 0 }) {
                    val t = timeFor(item.rid)
                    FilterChip(
                        selected = brush == Brush.Row(item.rid),
                        onClick = { vm.brush = if (brush == Brush.Row(item.rid)) Brush.None else Brush.Row(item.rid) },
                        label = {
                            Text(item.title, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 200.dp))
                            if (t > 0) Text("  " + duration(t), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        },
                        leadingIcon = { ColorDot(item.color) },
                        shape = RoundedCornerShape(50),
                    )
                }
                FilterChip(
                    selected = brush == Brush.Eraser,
                    onClick = { vm.brush = if (brush == Brush.Eraser) Brush.None else Brush.Eraser },
                    label = { Text("Eraser") },
                    leadingIcon = { Ic(R.drawable.ic_eraser, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
                    shape = RoundedCornerShape(50),
                    colors = FilterChipDefaults.filterChipColors(),
                )
            }
        }

        if (blocks.isNotEmpty()) {
            Text("Schedule", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 6.dp))
            Surface(
                shape = RoundedCornerShape(12.dp),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                color = MaterialTheme.colorScheme.surface,
            ) {
                Column {
                    blocks.forEachIndexed { i, b ->
                        if (i > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        ScheduleRow(vm, date, b, items)
                    }
                }
            }
        }
    }
}

@Composable
private fun ScheduleRow(vm: PlannerViewModel, date: LocalDate, b: Block, items: List<PlanItem>) {
    val item = items.firstOrNull { it.rid == b.row }
    Row(Modifier.padding(start = 14.dp), verticalAlignment = Alignment.CenterVertically) {
        ColorDot(item?.color ?: MaterialTheme.colorScheme.outline)
        Text(b.label, Modifier.padding(start = 10.dp).widthIn(min = 96.dp), fontWeight = FontWeight.SemiBold)
        Text(
            item?.title ?: vm.store.row(b.row)?.second?.task.orEmpty(),
            Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        IconButton(onClick = { vm.update { s -> (b.start until b.start + b.len).fold(s) { acc, slot -> acc.setSlot(date, slot, null) } } }) {
            Ic(R.drawable.ic_close, "Remove from the plan")
        }
    }
}

/** 24 h ring of 48 half-hour slots: midnight at the top, clockwise. */
@Composable
fun DayClock(vm: PlannerViewModel, date: LocalDate, items: List<PlanItem>, modifier: Modifier = Modifier) {
    val slots = vm.store.slots(date)
    val colors = items.associate { it.rid to it.color }
    val titles = items.associate { it.rid to it.title }
    val isToday = date == vm.today
    var active by remember { mutableStateOf<Int?>(null) }
    var now by remember { mutableStateOf(LocalTime.now()) }
    LaunchedEffect(isToday) {
        while (isToday) {
            now = LocalTime.now()
            delay(60_000)
        }
    }
    val measurer = rememberTextMeasurer()
    val fg = MaterialTheme.colorScheme.onSurface
    val currentSlots by rememberUpdatedState(slots)

    Canvas(
        modifier
            .aspectRatio(1f)
            .pointerInput(date) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    val geo = Geo(size.width.toFloat(), size.height.toFloat())
                    val start = geo.slotAt(down.position, ringOnly = true) ?: return@awaitEachGesture
                    val current = currentSlots[start]
                    val mode: Mode
                    val rid: Long?
                    when (val brush = vm.brush) {
                        is Brush.Row -> {
                            rid = brush.id
                            mode = if (current == brush.id) Mode.EraseOwn else Mode.Paint
                        }
                        Brush.Eraser -> { rid = null; mode = Mode.EraseAll }
                        Brush.None -> {
                            // No brush: tapping a planned slot picks up its task.
                            if (current != null) vm.brush = Brush.Row(current)
                            else vm.toastUndo("Pick a task below first, then tap the clock.")
                            down.consume()
                            return@awaitEachGesture
                        }
                    }
                    fun apply(slot: Int) {
                        val cur = vm.store.slots(date)[slot]
                        val new = when (mode) {
                            Mode.Paint -> rid
                            Mode.EraseOwn -> if (cur == rid) null else return
                            Mode.EraseAll -> null
                        }
                        if (new != cur) vm.update { it.setSlot(date, slot, new) }
                    }
                    down.consume()
                    apply(start)
                    active = start
                    var last = start
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) break
                        val slot = geo.slotAt(change.position, ringOnly = false) ?: continue
                        // Walk the short way round so fast drags don't skip slots.
                        val fwd = Math.floorMod(slot - last, SLOTS)
                        val step = if (fwd <= SLOTS / 2) 1 else -1
                        var s = last
                        while (s != slot) {
                            s = Math.floorMod(s + step, SLOTS)
                            apply(s)
                        }
                        last = slot
                        active = slot
                        change.consume()
                    }
                    active = null
                }
            },
    ) {
        val geo = Geo(size.width, size.height)
        val ring = geo.outer - geo.inner
        val mid = (geo.outer + geo.inner) / 2
        val arcTopLeft = Offset(geo.cx - mid, geo.cy - mid)
        val arcSize = Size(mid * 2, mid * 2)
        val slotDeg = 360f / SLOTS
        for (s in 0 until SLOTS) {
            val rid = slots[s]
            val night = s !in 12 until 44 // before 06:00 and after 22:00
            val color = when {
                rid != null -> (colors[rid] ?: Color.Gray).copy(alpha = if (active == s) 1f else 0.9f)
                else -> fg.copy(alpha = (if (night) 0.05f else 0.09f) + if (active == s) 0.12f else 0f)
            }
            drawArc(
                color = color,
                startAngle = -90f + s * slotDeg + 0.4f,
                sweepAngle = slotDeg - 0.8f,
                useCenter = false,
                topLeft = arcTopLeft,
                size = arcSize,
                style = Stroke(width = ring),
            )
        }
        // Hour ticks and labels.
        val labelStyle = TextStyle(fontSize = 11.sp, color = fg.copy(alpha = 0.6f))
        for (hour in 0 until 24) {
            val a = angle(hour * 2f)
            val len = if (hour % 6 == 0) 8.dp.toPx() else 4.dp.toPx()
            val r0 = geo.outer + 3.dp.toPx()
            drawLine(
                fg.copy(alpha = if (hour % 6 == 0) 0.55f else 0.3f),
                Offset(geo.cx + cos(a) * r0, geo.cy + sin(a) * r0),
                Offset(geo.cx + cos(a) * (r0 + len), geo.cy + sin(a) * (r0 + len)),
                strokeWidth = 1.2.dp.toPx(),
            )
            if (hour % 3 == 0) {
                val text = measurer.measure("%02d".format(hour), labelStyle)
                val r = geo.outer + 20.dp.toPx()
                drawText(text, topLeft = Offset(geo.cx + cos(a) * r - text.size.width / 2f, geo.cy + sin(a) * r - text.size.height / 2f))
            }
        }
        // Current time.
        if (isToday) {
            val a = angle((now.hour * 60 + now.minute) / 30f)
            val red = Color(0xFFE6474D)
            val r0 = geo.inner - 8.dp.toPx()
            drawLine(
                red, Offset(geo.cx + cos(a) * r0, geo.cy + sin(a) * r0),
                Offset(geo.cx + cos(a) * (geo.outer + 4.dp.toPx()), geo.cy + sin(a) * (geo.outer + 4.dp.toPx())),
                strokeWidth = 2.dp.toPx(), cap = StrokeCap.Round,
            )
            drawCircle(red, 3.dp.toPx(), Offset(geo.cx + cos(a) * r0, geo.cy + sin(a) * r0))
        }
        // Centre: the slot under your finger, else the day's total.
        val (big, small) = active?.let { s ->
            "${slotTime(s)}–${slotTime(s + 1)}" to (slots[s]?.let { titles[it] ?: "" }?.take(22) ?: "Free")
        } ?: (duration(slots.size) to "planned")
        val bigText = measurer.measure(big, TextStyle(fontSize = 20.sp, fontWeight = FontWeight.Bold, color = fg.copy(alpha = 0.9f)))
        val smallText = measurer.measure(small, TextStyle(fontSize = 12.sp, color = fg.copy(alpha = 0.6f)))
        drawText(bigText, topLeft = Offset(geo.cx - bigText.size.width / 2f, geo.cy - bigText.size.height + 4.dp.toPx()))
        drawText(smallText, topLeft = Offset(geo.cx - smallText.size.width / 2f, geo.cy + 6.dp.toPx()))
    }
}

private fun angle(slot: Float) = (-PI / 2 + slot / SLOTS * 2 * PI).toFloat()

private class Geo(w: Float, h: Float) {
    val cx = w / 2
    val cy = h / 2
    val outer = min(w, h) / 2 - min(w, h) * 0.085f
    val inner = outer * 0.6f

    /** Slot under a point; [ringOnly] requires the point to be on the ring. */
    fun slotAt(p: Offset, ringOnly: Boolean): Int? {
        val dx = p.x - cx
        val dy = p.y - cy
        val r = hypot(dx, dy)
        if (ringOnly && r !in (inner - 12f)..(outer + 24f)) return null
        val deg = ((Math.toDegrees(atan2(dy, dx).toDouble()) + 90.0) % 360.0 + 360.0) % 360.0
        return min((deg / 360.0 * SLOTS).toInt(), SLOTS - 1)
    }
}
