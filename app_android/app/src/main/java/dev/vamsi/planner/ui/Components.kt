package dev.vamsi.planner.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.vamsi.planner.R
import dev.vamsi.planner.ui.theme.priorityColor
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset

val PRIORITIES = listOf("No priority", "P1 · Highest", "P2 · High", "P3 · Medium", "P4 · Low")

fun displayName(name: String) = name.ifBlank { "Untitled" }

@Composable
fun Ic(@DrawableRes id: Int, description: String?, modifier: Modifier = Modifier, tint: Color = LocalContentColor.current) =
    Icon(painterResource(id), description, modifier, tint)

/** "P1"… pill, coloured like the desktop app. */
@Composable
fun PriorityBadge(priority: Int, modifier: Modifier = Modifier) {
    if (priority <= 0) return
    val c = priorityColor(priority, MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f))
    Text(
        "P$priority",
        modifier
            .clip(RoundedCornerShape(50))
            .background(c.copy(alpha = 0.16f))
            .padding(horizontal = 6.dp, vertical = 1.dp),
        color = c,
        fontSize = 11.sp,
        fontWeight = FontWeight.ExtraBold,
    )
}

@Composable
fun ColorDot(color: Color, size: Dp = 10.dp) {
    Box(Modifier.size(size).clip(CircleShape).background(color))
}

/** Coloured dot + "09:00–10:30" for a row that has time on the clock. */
@Composable
fun TimePill(times: String, color: Color) {
    Row(
        Modifier
            .clip(RoundedCornerShape(50))
            .background(MaterialTheme.colorScheme.onSurface.copy(alpha = 0.07f))
            .padding(horizontal = 8.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        ColorDot(color, 8.dp)
        Text(times, style = MaterialTheme.typography.labelSmall)
    }
}

private val EMOJIS = listOf(
    "🎯", "💼", "📚", "🏃", "🧘", "💪", "🛠️", "💻", "🧠", "📈", "💰", "🏠", "✈️", "🎨", "🎵", "🍎",
    "🌱", "📝", "📅", "⏰", "🔥", "⭐", "🚀", "🏆", "❤️", "😊", "🎓", "🧪", "📷", "🎮", "⚽", "🚴",
    "🧹", "🍳", "👪", "🐶", "🌍", "🗣️", "✍️", "📖", "🧩", "🎤", "🏋️", "🛌", "💧", "🥗", "☕", "📦",
)

/** Notion-style page icon picker: common emoji, or type any. */
@Composable
fun EmojiPickerDialog(onPick: (String) -> Unit, onDismiss: () -> Unit) {
    var custom by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Choose an icon") },
        text = {
            Column {
                LazyVerticalGrid(GridCells.Adaptive(44.dp), Modifier.height(260.dp)) {
                    items(EMOJIS) { e ->
                        Box(
                            Modifier.size(44.dp).clip(RoundedCornerShape(10.dp)).clickable { onPick(e) },
                            contentAlignment = Alignment.Center,
                        ) { Text(e, fontSize = 24.sp) }
                    }
                }
                OutlinedTextField(
                    custom, { custom = it.take(8) },
                    label = { Text("Or type any emoji") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                )
            }
        },
        confirmButton = { TextButton(onClick = { if (custom.isNotBlank()) onPick(custom.trim()) }, enabled = custom.isNotBlank()) { Text("Use") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

private fun LocalDate.toUtcMillis() = atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli()
private fun Long.toUtcDate(): LocalDate = Instant.ofEpochMilli(this).atZone(ZoneOffset.UTC).toLocalDate()

/** Pick a single day. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DayPickerDialog(initial: LocalDate, onPick: (LocalDate) -> Unit, onDismiss: () -> Unit) {
    val state = rememberDatePickerState(initialSelectedDateMillis = initial.toUtcMillis())
    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = { TextButton(onClick = { state.selectedDateMillis?.let { onPick(it.toUtcDate()) } }) { Text("Go") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    ) { DatePicker(state) }
}

/** A row's date: start day + how many days it lasts (1 = daily todo, 7 = a week focus). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DateSpanDialog(start: LocalDate, days: Long, onConfirm: (LocalDate, Long) -> Unit, onDismiss: () -> Unit) {
    val state = rememberDatePickerState(initialSelectedDateMillis = start.toUtcMillis())
    var span by remember { mutableLongStateOf(days) }
    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(onClick = { onConfirm(state.selectedDateMillis?.toUtcDate() ?: start, span) }) { Text("Save") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    ) {
        Column {
            DatePicker(state, showModeToggle = false)
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Lasts", Modifier.weight(1f))
                FilledTonalIconButton(onClick = { if (span > 1) span-- }) { Text("−", fontSize = 20.sp) }
                Text(
                    if (span == 1L) "1 day" else "$span days",
                    Modifier.padding(horizontal = 12.dp),
                    fontWeight = FontWeight.SemiBold,
                )
                FilledTonalIconButton(onClick = { if (span < 366) span++ }) { Ic(R.drawable.ic_add, "Longer") }
            }
            Spacer(Modifier.height(4.dp))
        }
    }
}
