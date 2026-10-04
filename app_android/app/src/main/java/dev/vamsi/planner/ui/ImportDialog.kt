package dev.vamsi.planner.ui

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.vamsi.planner.R
import dev.vamsi.planner.data.ImportedRow
import dev.vamsi.planner.data.NotionImport
import dev.vamsi.planner.data.fmtRange

/** Read plain text and HTML from the clipboard; keep whichever parses best. */
private fun readClipboard(context: Context, vm: PlannerViewModel, onlyIfTable: Boolean): String? {
    val cm = context.getSystemService(ClipboardManager::class.java) ?: return null
    val item = cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0) ?: return null
    val candidates = buildList {
        item.coerceToText(context)?.toString()?.let(::add)
        item.htmlText?.let { html ->
            val table = NotionImport.htmlTable(html)
            if (table.any { it.size >= 2 }) add(NotionImport.toTsv(table))
        }
    }
    return NotionImport.bestSource(candidates, vm.today) ?: if (onlyIfTable) null else candidates.firstOrNull()
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ImportDialog(vm: PlannerViewModel) {
    val context = LocalContext.current
    var text by remember { mutableStateOf(vm.importText.orEmpty()) }
    // Opening the importer picks up a copied table straight away.
    LaunchedEffect(Unit) {
        if (text.isEmpty()) readClipboard(context, vm, onlyIfTable = true)?.let { text = it }
    }
    val parsed = remember(text) { NotionImport.parse(text, vm.today) }
    val goals = vm.store.goalOrder()
    var target by remember { mutableStateOf(vm.importTarget) } // null = new goal
    var name by remember { mutableStateOf("Imported plan") }
    var replace by remember { mutableStateOf(false) }
    var targetMenu by remember { mutableStateOf(false) }
    val close = { vm.importText = null }

    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text("Import a plan") },
                    navigationIcon = { IconButton(onClick = close) { Ic(R.drawable.ic_close, "Close") } },
                    actions = {
                        TextButton(onClick = {
                            doImport(vm, parsed.rows, target, name, replace)
                            close()
                        }, enabled = parsed.rows.isNotEmpty()) { Text("Import", fontWeight = FontWeight.Bold) }
                    },
                )
            },
        ) { pad ->
            LazyColumn(Modifier.fillMaxSize().padding(pad), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                item {
                    Text(
                        "Copy a table in Notion and paste it here, or share it to Planner from Notion. " +
                            "It needs a date column like “5 Oct”, “12–18 Oct” or “26 Oct – 1 Nov”; a Week column and extra columns are picked up too.",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                item {
                    OutlinedTextField(
                        text, { text = it },
                        modifier = Modifier.fillMaxWidth(),
                        minLines = 6, maxLines = 10,
                        textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                        placeholder = { Text("| Week | Dates | Topics | …") },
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                        OutlinedButton(onClick = { readClipboard(context, vm, onlyIfTable = false)?.let { text = it } }) {
                            Ic(R.drawable.ic_paste, null)
                            Text("  Paste from clipboard")
                        }
                        TextButton(onClick = { text = "" }) { Text("Clear") }
                    }
                }
                item {
                    Surface(shape = RoundedCornerShape(12.dp), border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
                        Column {
                            Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text("Import into", Modifier.weight(1f))
                                Box {
                                    val label = goals.firstOrNull { it.id == target }?.let { "${it.icon} ${displayName(it.name)}" } ?: "New goal"
                                    TextButton(onClick = { targetMenu = true }) { Text("$label  ▾") }
                                    DropdownMenu(targetMenu, { targetMenu = false }) {
                                        DropdownMenuItem(text = { Text("New goal") }, onClick = { target = null; targetMenu = false })
                                        for (g in goals) DropdownMenuItem(
                                            text = { Text("${g.icon} ${displayName(g.name)}") },
                                            onClick = { target = g.id; targetMenu = false },
                                        )
                                    }
                                }
                            }
                            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                            if (target == null) {
                                OutlinedTextField(name, { name = it }, label = { Text("New goal name") }, singleLine = true,
                                    modifier = Modifier.fillMaxWidth().padding(12.dp))
                            } else {
                                Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text("Replace existing rows")
                                        Text("Off: rows are added and exact duplicates are skipped",
                                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    }
                                    Switch(replace, { replace = it })
                                }
                            }
                        }
                    }
                }
                item {
                    val ranges = parsed.rows.count { !it.isDay }
                    val days = parsed.rows.size - ranges
                    Text(
                        when {
                            parsed.rows.isNotEmpty() -> "$ranges focus range${if (ranges == 1) "" else "s"} · $days daily todo${if (days == 1) "" else "s"}"
                            text.isBlank() -> "Nothing pasted yet"
                            else -> "No plan rows found"
                        },
                        style = MaterialTheme.typography.titleMedium,
                    )
                    if (parsed.rows.isNotEmpty()) {
                        val cols = listOfNotNull(
                            parsed.dateColumn?.takeIf { it.isNotEmpty() }?.let { "Dates from “$it”" },
                            parsed.taskColumn?.takeIf { it.isNotEmpty() }?.let { "tasks from “$it”" },
                        )
                        if (cols.isNotEmpty()) Text(cols.joinToString(" · "), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (text.isNotBlank() && parsed.rows.isEmpty() && parsed.skipped.isEmpty()) {
                        Text("That doesn’t look like a table. Paste a Notion table, a Markdown table, or tab-separated text.", color = MaterialTheme.colorScheme.error)
                    }
                    for (reason in parsed.skipped) {
                        Text("Skipped · $reason", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                    }
                }
                items(parsed.rows.take(200)) { r -> PreviewRow(r) }
                if (parsed.rows.size > 200) item { Text("…and ${parsed.rows.size - 200} more", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
        }
    }
}

@Composable
private fun PreviewRow(r: ImportedRow) {
    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
        Text(
            fmtRange(r.start, r.end), Modifier.width(110.dp),
            color = if (r.isDay) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary,
            fontSize = 13.sp,
        )
        Column(Modifier.weight(1f)) {
            Text(r.task.ifEmpty { "Untitled" }, fontWeight = if (r.isDay) FontWeight.Normal else FontWeight.Bold)
            if (r.notes.isNotEmpty()) Text(r.notes, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** [target] null = new goal. Shows an Undo snackbar. */
private fun doImport(vm: PlannerViewModel, rows: List<ImportedRow>, target: Long?, name: String, replace: Boolean) {
    val before = vm.store
    var s = vm.store
    val gid = if (target == null) {
        val (s2, id) = s.newGoal(name.ifBlank { "Imported plan" }, "📋")
        s = s2
        id
    } else target
    if (replace) s = s.updateGoal(gid) { it.copy(rows = emptyList()) }
    var added = 0
    var dupes = 0
    for (r in rows) {
        val existing = s.goal(gid)?.rows.orEmpty()
        if (existing.any { it.start == r.start && it.end == r.end && it.task == r.task }) { dupes++; continue }
        s = s.addRow(gid, r.start, r.end, r.task, r.notes).first
        added++
    }
    vm.replace(s)
    vm.screen = Screen.GoalPage(gid)
    val msg = "Imported $added row${if (added == 1) "" else "s"}" + if (dupes > 0) " · $dupes duplicate${if (dupes == 1) "" else "s"} skipped" else ""
    vm.toastUndo(msg) {
        vm.replace(before)
        if (target == null) vm.screen = Screen.Today
    }
}
