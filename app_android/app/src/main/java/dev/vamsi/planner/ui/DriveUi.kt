package dev.vamsi.planner.ui

import androidx.activity.result.IntentSenderRequest
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.vamsi.planner.data.FILE_NAME
import java.text.DateFormat
import java.util.Date

/** Database cylinder with a badge: ✕ signed out, ✓ synced, arc syncing, ! error. */
@Composable
fun DriveIcon(status: SyncStatus, size: Dp = 24.dp) {
    val fg = MaterialTheme.colorScheme.onSurface
    Canvas(Modifier.size(size)) {
        val u = this.size.width / 24f
        val stroke = Stroke(width = 1.6f * u)
        val left = 3f * u
        val w = 14f * u
        val top = 3f * u
        val h = 15f * u
        val ry = 2.6f * u
        drawOval(fg, Offset(left, top), Size(w, ry * 2), style = stroke)
        for (y in listOf(top + h / 2 - ry, top + h - 2 * ry)) {
            drawArc(fg, 0f, 180f, false, Offset(left, y), Size(w, ry * 2), style = stroke)
        }
        drawLine(fg, Offset(left, top + ry), Offset(left, top + h - ry), 1.6f * u)
        drawLine(fg, Offset(left + w, top + ry), Offset(left + w, top + h - ry), 1.6f * u)

        val c = Offset(18f * u, 18f * u)
        val r = 5.5f * u
        val badge = when (status) {
            SyncStatus.SignedOut -> Color(0xFFE04A4D)
            SyncStatus.Synced -> Color(0xFF33AD66)
            SyncStatus.Syncing -> Color(0xFF3D85F2)
            is SyncStatus.Error -> Color(0xFFF09429)
        }
        drawCircle(badge, r, c)
        val white = Color.White
        val sw = 1.5f * u
        when (status) {
            SyncStatus.SignedOut -> {
                val d = 2.2f * u
                drawLine(white, c + Offset(-d, -d), c + Offset(d, d), sw, StrokeCap.Round)
                drawLine(white, c + Offset(d, -d), c + Offset(-d, d), sw, StrokeCap.Round)
            }
            SyncStatus.Synced -> {
                drawLine(white, c + Offset(-2.6f * u, 0.2f * u), c + Offset(-0.6f * u, 2.2f * u), sw, StrokeCap.Round)
                drawLine(white, c + Offset(-0.6f * u, 2.2f * u), c + Offset(2.8f * u, -2f * u), sw, StrokeCap.Round)
            }
            SyncStatus.Syncing -> drawArc(white, 20f, 270f, false, c - Offset(2.6f * u, 2.6f * u), Size(5.2f * u, 5.2f * u), style = Stroke(sw, cap = StrokeCap.Round))
            is SyncStatus.Error -> {
                drawLine(white, c + Offset(0f, -2.8f * u), c + Offset(0f, 0.6f * u), sw, StrokeCap.Round)
                drawCircle(white, 0.9f * u, c + Offset(0f, 2.6f * u))
            }
        }
    }
}

@Composable
fun DriveButton(vm: PlannerViewModel) {
    IconButton(onClick = { vm.driveOpen = true }) { DriveIcon(vm.drive.status) }
}

private fun lastSyncText(millis: Long?): String =
    millis?.let { "Last synced " + DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(it)) } ?: "Synced"

@Composable
fun DriveDialog(vm: PlannerViewModel, launch: (IntentSenderRequest) -> Unit, onDismiss: () -> Unit) {
    val drive = vm.drive
    val acct = drive.account
    val status = drive.status
    var folder by remember(acct.folder) { mutableStateOf(acct.folder) }
    val error = (status as? SyncStatus.Error)?.message

    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { DriveIcon(status, 48.dp) },
        title = {
            Text(
                if (acct.signedIn) "Signed in as ${acct.email ?: "your Google account"}" else "Store your plans in Google Drive",
                textAlign = TextAlign.Center,
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    if (acct.signedIn) "Your goals, plans and day schedules are saved to My Drive › ${acct.folder} › $FILE_NAME."
                    else "Sign in and Planner keeps your data in My Drive › ${acct.folder} › $FILE_NAME, shared with Planner on your computer. " +
                        "A local copy stays for offline use.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(
                    folder, { folder = it },
                    label = { Text("Folder in My Drive") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    trailingIcon = {
                        if (acct.signedIn && folder.isNotBlank() && folder.trim() != acct.folder) {
                            TextButton(onClick = { drive.changeFolder(folder) }) { Text("Apply") }
                        }
                    },
                )
                if (acct.signedIn) {
                    Text(
                        when {
                            error != null -> error
                            status == SyncStatus.Syncing -> "Syncing…"
                            else -> lastSyncText(acct.lastSync)
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = if (error != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(onClick = { drive.syncNow() }, enabled = status != SyncStatus.Syncing) { Text("Sync now") }
                        TextButton(onClick = { drive.signOut() }) { Text("Sign out", color = MaterialTheme.colorScheme.error) }
                    }
                } else {
                    Text(
                        "Planner can only see files it creates, and keeps its data in this one folder.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    // Selectable, so the package name / SHA-1 can be copied into Google Cloud.
                    if (error != null) SelectionContainer {
                        Text(error, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                    }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                        if (drive.signingIn) {
                            CircularProgressIndicator(Modifier.size(20.dp).padding(end = 8.dp), strokeWidth = 2.dp)
                            Text("  Waiting for Google…")
                        } else {
                            Button(onClick = { drive.signIn(folder, launch) }, enabled = folder.isNotBlank()) {
                                Text("Sign in with Google")
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}
