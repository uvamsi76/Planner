package dev.vamsi.planner.ui

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconToggleButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.vamsi.planner.R
import dev.vamsi.planner.data.Goal
import dev.vamsi.planner.data.PlanRow
import dev.vamsi.planner.data.fmtRange
import kotlinx.coroutines.launch
import java.time.LocalDate
import kotlin.math.roundToInt

private const val INSERT_W = 36f
private const val CHECK_W = 44f
private const val DELETE_W = 44f
private const val MIN_COL = 72f

private enum class Field { Task, Notes }

/** Column widths in dp. Stored widths are the minimum; spare width is shared by Task and Notes. */
private class Columns(date: Float, task: Float, notes: Float) {
    var date by mutableFloatStateOf(date)
    var task by mutableFloatStateOf(task)
    var notes by mutableFloatStateOf(notes)

    /** Effective (date, task, notes) widths for a table [available] dp wide. */
    fun fit(available: Float): Triple<Float, Float, Float> {
        val fixed = INSERT_W + CHECK_W + DELETE_W + date
        val spare = available - fixed - task - notes
        if (spare <= 0) return Triple(date, task, notes)
        val share = task / (task + notes)
        return Triple(date, task + spare * share, notes + spare * (1 - share))
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GoalScreen(vm: PlannerViewModel, gid: Long, snackbar: SnackbarHostState, onMenu: (() -> Unit)?) {
    val goal = vm.store.goal(gid) ?: run { vm.screen = Screen.Today; return }
    var menu by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var pickingIcon by remember { mutableStateOf(false) }
    var editing by remember(gid) { mutableStateOf<Pair<Long, Field>?>(null) }
    var dateRow by remember(gid) { mutableStateOf<PlanRow?>(null) }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val focus = LocalFocusManager.current
    val cols = remember(gid) {
        val saved = vm.repo.columnWidths(gid)
        Columns(saved?.first ?: 104f, saved?.second ?: 220f, saved?.third ?: 200f)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(displayName(goal.name), maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    if (onMenu != null) IconButton(onClick = onMenu) { Ic(R.drawable.ic_menu, "Menu") }
                    else IconButton(onClick = { vm.screen = Screen.Today }) { Ic(R.drawable.ic_prev, "Back") }
                },
                actions = {
                    // Google Drive: database icon with ✕ (signed out) or ✓ (synced).
                    DriveButton(vm)
                    IconToggleButton(checked = vm.hidePast, onCheckedChange = { vm.hidePast = it }) { Ic(R.drawable.ic_hide, "Hide past rows") }
                    Box {
                        IconButton(onClick = { menu = true }) { Ic(R.drawable.ic_more, "More") }
                        DropdownMenu(menu, { menu = false }) {
                            DropdownMenuItem(text = { Text("Import rows from Notion…") }, onClick = { menu = false; vm.openImport(target = gid) })
                            DropdownMenuItem(
                                text = { Text("Delete goal…", color = MaterialTheme.colorScheme.error) },
                                onClick = { menu = false; confirmDelete = true },
                            )
                        }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { pad ->
        BoxWithConstraints(
            Modifier
                .fillMaxSize()
                .padding(pad)
                // Tapping empty space finishes editing, like pressing Enter.
                .pointerInput(Unit) { detectTapGestures(onTap = { focus.clearFocus() }) },
        ) {
            val side = if (maxWidth >= 720.dp) 40.dp else 16.dp
            val available = (maxWidth - side * 2).value
            val (dateW, taskW, notesW) = cols.fit(available)
            val tableW = INSERT_W + CHECK_W + DELETE_W + dateW + taskW + notesW
            val hScroll = rememberScrollState()
            val rows = goal.sortedRows().filter { !(vm.hidePast && it.end.isBefore(vm.today)) }

            LazyColumn(state = listState, modifier = Modifier.fillMaxSize(), contentPadding = PaddingValues(horizontal = side, vertical = 16.dp)) {
                item(key = "head") { GoalHeader(vm, goal, onIcon = { pickingIcon = true }) }
                item(key = "cols") {
                    TableHeader(cols, dateW, taskW, notesW, tableW, hScroll) {
                        vm.repo.saveColumnWidths(gid, cols.date, cols.task, cols.notes)
                    }
                }
                items(rows, key = { it.id }) { r ->
                    TableRow(
                        vm, goal, r, dateW, taskW, notesW, tableW, hScroll,
                        editing = editing?.takeIf { it.first == r.id }?.second,
                        onEdit = { editing = r.id to it },
                        onDoneEditing = { if (editing?.first == r.id) editing = null },
                        onDate = { dateRow = r },
                        onInsertDay = {
                            val newId = insertDayInWeek(vm, gid, r)
                            editing = newId to Field.Task
                            scope.launch {
                                val index = vm.store.goal(gid)!!.sortedRows().indexOfFirst { it.id == newId }
                                if (index >= 0) listState.animateScrollToItem(index + 2)
                            }
                        },
                        onDelete = {
                            vm.update { it.removeRow(gid, r.id) }
                            vm.toastUndo(if (r.task.isEmpty()) "Row deleted" else "Deleted “${r.task.lines().first()}”") {
                                vm.update { s -> s.updateGoal(gid) { g -> g.copy(rows = g.rows + r) } }
                            }
                        },
                    )
                }
                if (rows.isEmpty()) item(key = "empty") {
                    Text(
                        "No rows yet. Add some below, or share a Notion table to Planner.",
                        Modifier.padding(vertical = 16.dp),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                item(key = "footer") {
                    Row(Modifier.padding(top = 6.dp).horizontalScroll(rememberScrollState())) {
                        for ((label, count, span, ranges) in listOf(
                            Quad("New day", 1, 1L, false), Quad("Next 7 days", 7, 1L, false), Quad("Week focus", 1, 7L, true),
                        )) {
                            TextButton(onClick = {
                                val first = addRows(vm, gid, count, span, ranges)
                                editing = first to Field.Task
                                scope.launch { listState.animateScrollToItem(listState.layoutInfo.totalItemsCount) }
                            }) {
                                Ic(R.drawable.ic_add, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(" $label", color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                }
            }
        }
    }

    dateRow?.let { r ->
        DateSpanDialog(r.start, r.days, onConfirm = { start, days ->
            vm.update { it.updateRow(gid, r.id) { row -> row.copy(start = start, end = start.plusDays(days - 1)) } }
            dateRow = null
        }, onDismiss = { dateRow = null })
    }
    if (pickingIcon) EmojiPickerDialog(onPick = { e ->
        vm.update { it.updateGoal(gid) { g -> g.copy(icon = e) } }
        pickingIcon = false
    }, onDismiss = { pickingIcon = false })
    if (confirmDelete) AlertDialog(
        onDismissRequest = { confirmDelete = false },
        title = { Text("Delete “${displayName(goal.name)}”?") },
        text = { Text("Its ${goal.rows.size} row(s) will be deleted too. This can’t be undone.") },
        confirmButton = {
            TextButton(onClick = {
                confirmDelete = false
                vm.screen = Screen.Today
                vm.update { it.removeGoal(gid) }
            }) { Text("Delete", color = MaterialTheme.colorScheme.error) }
        },
        dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
    )
}

private data class Quad(val label: String, val count: Int, val span: Long, val ranges: Boolean)

@Composable
private fun GoalHeader(vm: PlannerViewModel, goal: Goal, onIcon: () -> Unit) {
    Column {
        Text(goal.icon, fontSize = 52.sp, modifier = Modifier.clip(RoundedCornerShape(10.dp)).clickable(onClick = onIcon).padding(4.dp))
        var name by remember(goal.id) { mutableStateOf(goal.name) }
        BasicTextField(
            name,
            { name = it; vm.update { s -> s.updateGoal(goal.id) { g -> g.copy(name = it) } } },
            textStyle = MaterialTheme.typography.displaySmall.copy(color = MaterialTheme.colorScheme.onSurface),
            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
            singleLine = true,
            decorationBox = { inner ->
                if (name.isEmpty()) Text("Untitled", style = MaterialTheme.typography.displaySmall, color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f))
                inner()
            },
            modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        )
        // Notion-style property row: Priority.
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Priority", Modifier.width(80.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
            var open by remember { mutableStateOf(false) }
            Box {
                AssistChip(onClick = { open = true }, label = { Text(PRIORITIES[goal.priority.coerceIn(0, 4)]) })
                DropdownMenu(open, { open = false }) {
                    PRIORITIES.forEachIndexed { i, p ->
                        DropdownMenuItem(text = { Text(p) }, onClick = {
                            vm.update { it.updateGoal(goal.id) { g -> g.copy(priority = i) } }
                            open = false
                        })
                    }
                }
            }
        }
        Text(
            "One row per day for daily todos. Give a row a longer span (like a week) for a focus that shows on each of those days.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 4.dp, bottom = 16.dp),
        )
    }
}

/** Header with draggable borders after Date and after Task. */
@Composable
private fun TableHeader(cols: Columns, dateW: Float, taskW: Float, notesW: Float, tableW: Float, h: ScrollState, onResized: () -> Unit) {
    val density = LocalDensity.current
    val dim = MaterialTheme.colorScheme.onSurfaceVariant
    Column(Modifier.horizontalScroll(h)) {
        Box(Modifier.width(tableW.dp).height(36.dp)) {
            Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.width((INSERT_W + CHECK_W).dp))
                HeaderLabel("Date", dateW)
                HeaderLabel("Task", taskW)
                HeaderLabel("Notes", notesW)
            }
            // Drag handles: wide touch targets centred on each border.
            val borders = listOf(INSERT_W + CHECK_W + dateW, INSERT_W + CHECK_W + dateW + taskW)
            borders.forEachIndexed { i, x ->
                Box(
                    Modifier
                        .offset { IntOffset(with(density) { (x - 12f).dp.roundToPx() }, 0) }
                        .width(24.dp)
                        .fillMaxHeight()
                        .pointerInput(i) {
                            detectHorizontalDragGestures(onDragEnd = onResized) { change, dx ->
                                change.consume()
                                val d = with(density) { dx.toDp().value }
                                if (i == 0) cols.date = (cols.date + d).coerceIn(MIN_COL, 260f)
                                else {
                                    // Task|Notes: move width from one to the other.
                                    val t = (cols.task + d).coerceIn(MIN_COL, cols.task + cols.notes - MIN_COL)
                                    cols.notes += cols.task - t
                                    cols.task = t
                                }
                            }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Box(Modifier.width(2.dp).height(18.dp).background(dim.copy(alpha = 0.35f), RoundedCornerShape(1.dp)))
                }
            }
        }
        HorizontalDivider(Modifier.width(tableW.dp), color = MaterialTheme.colorScheme.outlineVariant)
    }
}

@Composable
private fun HeaderLabel(text: String, width: Float) {
    Text(
        text, Modifier.width(width.dp).padding(start = 8.dp),
        style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun TableRow(
    vm: PlannerViewModel, goal: Goal, r: PlanRow,
    dateW: Float, taskW: Float, notesW: Float, tableW: Float, h: ScrollState,
    editing: Field?, onEdit: (Field) -> Unit, onDoneEditing: () -> Unit,
    onDate: () -> Unit, onInsertDay: () -> Unit, onDelete: () -> Unit,
) {
    val current = r.covers(vm.today)
    val accent = MaterialTheme.colorScheme.primary
    Column(Modifier.horizontalScroll(h)) {
        Row(
            Modifier
                .width(tableW.dp)
                .background(if (!r.isDay) accent.copy(alpha = 0.08f) else androidx.compose.ui.graphics.Color.Transparent)
                .heightIn(min = 48.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // "+" on week (range) rows: add a day of that week below it.
            Box(Modifier.width(INSERT_W.dp), contentAlignment = Alignment.Center) {
                if (!r.isDay) IconButton(onClick = onInsertDay, Modifier.size(32.dp)) {
                    Ic(R.drawable.ic_add, "Add a day of this week below", tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Box(Modifier.width(CHECK_W.dp)) {
                Checkbox(r.done, { done -> vm.update { it.updateRow(goal.id, r.id) { row -> row.copy(completedOn = if (done) vm.today else null) } } })
            }
            Text(
                fmtRange(r.start, r.end),
                Modifier.width(dateW.dp).clickable(onClick = onDate).padding(horizontal = 8.dp, vertical = 12.dp),
                color = if (current) accent else MaterialTheme.colorScheme.onSurface,
                fontWeight = if (current) FontWeight.Bold else FontWeight.Normal,
                maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
            Cell(
                r.task, "Untitled", taskW, editing == Field.Task, bold = !r.isDay, strike = r.done,
                onEdit = { onEdit(Field.Task) }, onDone = onDoneEditing,
                onChange = { v -> vm.update { it.updateRow(goal.id, r.id) { row -> row.copy(task = v) } } },
            )
            Cell(
                r.notes, "", notesW, editing == Field.Notes, bold = false, strike = false,
                onEdit = { onEdit(Field.Notes) }, onDone = onDoneEditing,
                onChange = { v -> vm.update { it.updateRow(goal.id, r.id) { row -> row.copy(notes = v) } } },
            )
            IconButton(onClick = onDelete, Modifier.width(DELETE_W.dp)) {
                Ic(R.drawable.ic_delete, "Delete row", tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f))
            }
        }
        HorizontalDivider(Modifier.width(tableW.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f))
    }
}

/**
 * A table cell: wrapped text; tap to edit in place. Done (keyboard), Enter
 * (hardware keyboard), or tapping elsewhere finishes; Shift+Enter adds a line.
 */
@Composable
private fun Cell(
    value: String, placeholder: String, width: Float, editing: Boolean, bold: Boolean, strike: Boolean,
    onEdit: () -> Unit, onDone: () -> Unit, onChange: (String) -> Unit,
) {
    val style = TextStyle(
        fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal,
        textDecoration = if (strike) TextDecoration.LineThrough else null,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (strike) 0.5f else 1f),
        fontSize = 15.sp,
    )
    val mod = Modifier.width(width.dp).padding(horizontal = 2.dp, vertical = 4.dp)
    if (!editing) {
        Text(
            value.ifEmpty { placeholder },
            mod.clickable(onClick = onEdit).padding(horizontal = 6.dp, vertical = 8.dp),
            style = style.copy(color = if (value.isEmpty()) style.color.copy(alpha = 0.35f) else style.color),
        )
        return
    }
    val focus = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    var field by remember { mutableStateOf(TextFieldValue(value, TextRange(value.length))) }
    var hadFocus by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { focus.requestFocus() }
    BasicTextField(
        field,
        { field = it; if (it.text != value) onChange(it.text) },
        textStyle = style,
        cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { focusManager.clearFocus() }),
        modifier = mod
            .background(MaterialTheme.colorScheme.onSurface.copy(alpha = 0.06f), RoundedCornerShape(6.dp))
            .padding(horizontal = 6.dp, vertical = 8.dp)
            .focusRequester(focus)
            .onFocusChanged { if (it.isFocused) hadFocus = true else if (hadFocus) onDone() }
            .onPreviewKeyEvent { e ->
                if (e.type == KeyEventType.KeyDown && (e.key == Key.Enter || e.key == Key.NumPadEnter) && !e.isShiftPressed) {
                    focusManager.clearFocus(); true
                } else if (e.type == KeyEventType.KeyDown && e.key == Key.Escape) {
                    focusManager.clearFocus(); true
                } else false
            },
    )
}

/** Add a day inside week row [week] (its first day without a todo, else its last day). Returns the new id. */
private fun insertDayInWeek(vm: PlannerViewModel, gid: Long, week: PlanRow): Long = vm.updateWith { s ->
    val taken = s.goal(gid)?.rows.orEmpty().filter { it.isDay }.map { it.start }.toSet()
    var d = week.start
    var date: LocalDate = week.end
    while (!d.isAfter(week.end)) {
        if (d !in taken) { date = d; break }
        d = d.plusDays(1)
    }
    val (s2, row) = s.addRow(gid, date, date)
    s2 to row.id
}

/** Append [count] rows, each [span] days, after the last day row (or range row). Returns the first id. */
private fun addRows(vm: PlannerViewModel, gid: Long, count: Int, span: Long, ranges: Boolean): Long = vm.updateWith { s0 ->
    var s = s0
    val start = s.goal(gid)!!.nextFreeDate(ranges, vm.today)
    var first = -1L
    for (i in 0 until count) {
        val st = start.plusDays(i * span)
        val (s2, row) = s.addRow(gid, st, st.plusDays(span - 1))
        s = s2
        if (first < 0) first = row.id
    }
    s to first
}
