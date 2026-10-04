package dev.vamsi.planner.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconToggleButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.unit.dp
import dev.vamsi.planner.R
import dev.vamsi.planner.data.Block
import dev.vamsi.planner.data.Goal
import dev.vamsi.planner.data.PlanRow
import dev.vamsi.planner.data.firstLine
import dev.vamsi.planner.data.fmtRange
import dev.vamsi.planner.ui.theme.Palette
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import java.util.Locale

private val titleFmt = DateTimeFormatter.ofPattern("EEEE, d MMMM", Locale.getDefault())

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TodayScreen(vm: PlannerViewModel, snackbar: SnackbarHostState, onMenu: (() -> Unit)?) {
    var picking by remember { mutableStateOf(false) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Today") },
                navigationIcon = { onMenu?.let { IconButton(onClick = it) { Ic(R.drawable.ic_menu, "Menu") } } },
                actions = {
                    IconButton(onClick = { vm.date = vm.date.minusDays(1) }) { Ic(R.drawable.ic_prev, "Previous day") }
                    IconButton(onClick = { vm.date = vm.date.plusDays(1) }) { Ic(R.drawable.ic_next, "Next day") }
                    // Small "+" that reveals the add-a-todo bar (hidden by default).
                    IconToggleButton(checked = vm.quickOpen, onCheckedChange = { vm.quickOpen = it }) {
                        Ic(R.drawable.ic_add, "Add a todo for this day")
                    }
                    IconButton(onClick = { picking = true }) { Ic(R.drawable.ic_calendar, "Pick a date") }
                    // Google Drive: database icon with ✕ (signed out) or ✓ (synced).
                    DriveButton(vm)
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { pad ->
        BoxWithConstraints(Modifier.fillMaxSize().padding(pad)) {
            val date = vm.date
            val items = planItems(vm)
            val blocks = vm.store.blocks(date)
            if (maxWidth >= 720.dp) {
                // Tablet / landscape: todos left, day plan right (like the desktop).
                Row(Modifier.fillMaxSize()) {
                    LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(24.dp)) {
                        todos(vm, items, blocks)
                        focusSection(vm)
                    }
                    DayPlanPanel(
                        vm, date, items,
                        Modifier.width(360.dp).verticalScroll(rememberScrollState()).padding(top = 24.dp, end = 24.dp, bottom = 24.dp),
                    )
                }
            } else {
                // Phone: one column, the day plan under the todos.
                LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(horizontal = 20.dp, vertical = 12.dp)) {
                    todos(vm, items, blocks)
                    item(key = "dayplan") {
                        Spacer(Modifier.height(28.dp))
                        DayPlanPanel(vm, date, items)
                    }
                    focusSection(vm)
                    item(key = "bottom") { Spacer(Modifier.height(24.dp)) }
                }
            }
        }
    }
    if (picking) {
        DayPickerDialog(vm.date, onPick = { vm.date = it; picking = false }, onDismiss = { picking = false })
    }
}

/** Everything schedulable on the day, in priority order, each with a colour. */
private fun planItems(vm: PlannerViewModel): List<PlanItem> {
    val s = vm.store
    val rows = mutableListOf<Pair<Goal, PlanRow>>()
    if (vm.date == vm.today) rows += s.overdue(vm.date)
    // Ranges (week focuses) aren't tasks: no chip, no time on the clock.
    for (g in s.agenda(vm.date)) rows += g.days.map { g.goal to it }
    return rows.mapIndexed { i, (g, r) ->
        PlanItem(r.id, firstLine(r.task).ifEmpty { "Untitled" }, "${g.icon} ${displayName(g.name)}", Palette[i % Palette.size])
    }
}

private fun timeText(rid: Long, blocks: List<Block>) = blocks.filter { it.row == rid }.joinToString(", ") { it.label }

private fun LazyListScope.todos(vm: PlannerViewModel, items: List<PlanItem>, blocks: List<Block>) {
    val date = vm.date
    val today = vm.today
    val store = vm.store
    val color = { rid: Long -> items.firstOrNull { it.rid == rid }?.color }

    item(key = "header") {
        val days = ChronoUnit.DAYS.between(today, date)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                when {
                    days == 0L -> "Today"
                    days == 1L -> "Tomorrow"
                    days == -1L -> "Yesterday"
                    days > 0 -> "In $days days"
                    else -> "${-days} days ago"
                },
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontWeight = FontWeight.SemiBold,
            )
            // Jump back (lives here, not in the top bar, to leave room on phones).
            if (days != 0L) TextButton(onClick = { vm.date = today }) { Text("Back to today") }
        }
        Text(date.format(titleFmt), style = MaterialTheme.typography.displaySmall, modifier = Modifier.padding(bottom = 8.dp))
        val (done, total) = store.progress(date)
        if (total > 0) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                LinearProgressIndicator(
                    progress = { done.toFloat() / total },
                    modifier = Modifier.width(180.dp),
                    drawStopIndicator = {},
                )
                Text("$done of $total done", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }

    item(key = "quickadd") { QuickAdd(vm) }

    val overdue = if (date == today) store.overdue(date) else emptyList()
    if (overdue.isNotEmpty()) {
        item(key = "overdue-title") {
            Text(
                "Carried over · ${overdue.size}",
                Modifier.padding(top = 24.dp, bottom = 4.dp),
                style = MaterialTheme.typography.titleMedium,
                color = Palette[6],
            )
        }
        for ((g, r) in overdue) item(key = "o${r.id}") {
            TodoRow(vm, g, r, notes = "${g.icon} ${displayName(g.name)} · ${fmtRange(r.start, r.end)}", time = timeText(r.id, blocks), color = color(r.id))
            TextButton(
                onClick = { vm.update { it.updateRow(g.id, r.id) { row -> row.copy(start = today, end = today) } } },
                modifier = Modifier.padding(start = 32.dp),
            ) { Text("Move to today") }
        }
    }

    // Only checkbox todos here; week focuses go in a collapsed section at the end.
    val agenda = store.agenda(date)
    for (group in agenda.filter { it.days.isNotEmpty() }) {
        val g = group.goal
        item(key = "g${g.id}") {
            Row(
                Modifier
                    .padding(top = 24.dp, bottom = 6.dp)
                    .clickable { vm.screen = Screen.GoalPage(g.id) }
                    .padding(vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(g.icon)
                Text(displayName(g.name), style = MaterialTheme.typography.titleMedium)
                PriorityBadge(g.priority)
            }
        }
        for (r in group.days) item(key = "d${r.id}") {
            TodoRow(vm, g, r, notes = r.notes, time = timeText(r.id, blocks), color = color(r.id))
        }
    }

    if (overdue.isEmpty() && agenda.all { it.days.isEmpty() }) {
        item(key = "empty") {
            Column(Modifier.padding(top = 32.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text("No todos for this day.", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Add rows to a goal’s plan, share a Notion table to Planner, or tap + above to jot something down.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/**
 * Week focuses (and other ranges) covering the day, collapsed by default:
 * they're context, not tasks, so they stay out of the way at the end.
 */
private fun LazyListScope.focusSection(vm: PlannerViewModel) {
    val focuses = vm.store.agenda(vm.date).flatMap { g -> g.ranges.map { g.goal to it } }
    if (focuses.isEmpty()) return
    item(key = "focus-head") {
        Row(
            Modifier
                .padding(top = 24.dp, bottom = 4.dp)
                .clickable { vm.focusOpen = !vm.focusOpen }
                .padding(vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("📌")
            Text("Week focus · ${focuses.size}", style = MaterialTheme.typography.titleSmall)
            Ic(
                if (vm.focusOpen) R.drawable.ic_expand_less else R.drawable.ic_expand_more,
                if (vm.focusOpen) "Hide" else "Show",
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    if (vm.focusOpen) for ((g, r) in focuses) item(key = "r${r.id}") {
        Surface(
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.055f),
            shape = RoundedCornerShape(10.dp),
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        ) {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(r.task.ifEmpty { "Untitled focus" }, Modifier.weight(1f), fontWeight = FontWeight.Bold)
                    Text("${g.icon} · ${fmtRange(r.start, r.end)}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (r.notes.isNotBlank()) Text(r.notes, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}

@Composable
private fun TodoRow(vm: PlannerViewModel, g: Goal, r: PlanRow, notes: String, time: String, color: androidx.compose.ui.graphics.Color?) {
    Row(Modifier.fillMaxWidth().padding(vertical = 2.dp), verticalAlignment = Alignment.Top) {
        Checkbox(checked = r.done, onCheckedChange = { done ->
            vm.update { it.updateRow(g.id, r.id) { row -> row.copy(completedOn = if (done) vm.date else null) } }
        })
        Column(Modifier.weight(1f).padding(top = 12.dp)) {
            Text(
                r.task.ifEmpty { "Untitled" },
                textDecoration = if (r.done) TextDecoration.LineThrough else null,
                color = if (r.done) MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f) else MaterialTheme.colorScheme.onSurface,
            )
            // Keep the page to the todos: long notes are cut to two lines.
            if (notes.isNotBlank()) Text(
                notes, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2, overflow = TextOverflow.Ellipsis,
            )
        }
        if (time.isNotEmpty() && color != null) Box(Modifier.padding(top = 12.dp, start = 8.dp)) { TimePill(time, color) }
    }
}

@Composable
private fun QuickAdd(vm: PlannerViewModel) {
    val goals = vm.store.goalOrder()
    AnimatedVisibility(vm.quickOpen) {
        if (goals.isEmpty()) {
            Button(onClick = {
                val id = vm.updateWith { it.newGoal("") }
                vm.screen = Screen.GoalPage(id)
            }, Modifier.padding(vertical = 12.dp)) { Text("Create your first goal") }
            return@AnimatedVisibility
        }
        var text by remember { mutableStateOf("") }
        var menu by remember { mutableStateOf(false) }
        val target = goals.firstOrNull { it.id == vm.quickGoal } ?: goals.first()
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { focus.requestFocus() }
        val submit = {
            if (text.isNotBlank()) {
                vm.update { it.addRow(target.id, vm.date, vm.date, text.trim()).first }
                vm.quickGoal = target.id
                text = ""
            }
        }
        Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                text, { text = it },
                placeholder = { Text("Add a todo for ${vm.date.format(DateTimeFormatter.ofPattern("EEE d MMM"))}…") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { submit() }),
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focus)
                    .onPreviewKeyEvent { if (it.key == Key.Escape) { vm.quickOpen = false; true } else false },
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) {
                    TextButton(onClick = { menu = true }) { Text("${target.icon} ${displayName(target.name)}  ▾") }
                    DropdownMenu(menu, { menu = false }) {
                        for (g in goals) DropdownMenuItem(text = { Text("${g.icon} ${displayName(g.name)}") }, onClick = { vm.quickGoal = g.id; menu = false })
                    }
                }
                FilledIconButton(onClick = submit, enabled = text.isNotBlank()) { Ic(R.drawable.ic_add, "Add") }
            }
        }
    }
}
