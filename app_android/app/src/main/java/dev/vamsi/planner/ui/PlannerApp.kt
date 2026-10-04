package dev.vamsi.planner.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.PermanentDrawerSheet
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.vamsi.planner.R
import kotlinx.coroutines.launch

@Composable
fun PlannerApp(vm: PlannerViewModel) {
    val snackbar = remember { SnackbarHostState() }
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()

    LaunchedEffect(vm.undoEvent) {
        val ev = vm.undoEvent ?: return@LaunchedEffect
        snackbar.currentSnackbarData?.dismiss()
        val result = snackbar.showSnackbar(
            ev.message,
            actionLabel = if (ev.undo != null) "Undo" else null,
            withDismissAction = true,
            duration = SnackbarDuration.Short,
        )
        if (result == SnackbarResult.ActionPerformed) ev.undo?.invoke()
    }
    BackHandler(enabled = drawer.isOpen) { scope.launch { drawer.close() } }
    BackHandler(enabled = !drawer.isOpen && vm.screen is Screen.GoalPage) { vm.screen = Screen.Today }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        // Tablets get a permanent sidebar like the desktop; phones a slide-out drawer.
        val wide = maxWidth >= 840.dp
        val go: (Screen) -> Unit = { s ->
            vm.screen = s
            if (s == Screen.Today) vm.date = vm.today
            scope.launch { drawer.close() }
        }
        val content: @Composable (onMenu: (() -> Unit)?) -> Unit = { onMenu ->
            when (val s = vm.screen) {
                Screen.Today -> TodayScreen(vm, snackbar, onMenu)
                is Screen.GoalPage -> GoalScreen(vm, s.id, snackbar, onMenu)
            }
        }
        if (wide) {
            Row {
                PermanentDrawerSheet(Modifier.width(280.dp)) { Sidebar(vm, go) }
                content(null)
            }
        } else {
            ModalNavigationDrawer(
                drawerState = drawer,
                drawerContent = { ModalDrawerSheet(Modifier.width(300.dp)) { Sidebar(vm, go) } },
            ) { content { scope.launch { drawer.open() } } }
        }
    }

    if (vm.importText != null) ImportDialog(vm)
    ConflictDialog(vm)
}

/** Today, then goals by priority with P-badges and today's remaining counts. */
@Composable
private fun Sidebar(vm: PlannerViewModel, go: (Screen) -> Unit) {
    val store = vm.store
    val today = vm.today
    fun remaining(rows: List<dev.vamsi.planner.data.PlanRow>) = rows.count { it.isDay && it.covers(today) && !it.done }
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp)) {
        Text("Planner", Modifier.padding(16.dp), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
        NavigationDrawerItem(
            label = { Text("Today") },
            icon = { Text("📅") },
            badge = { remaining(store.goals.flatMap { it.rows }).takeIf { it > 0 }?.let { Text("$it") } },
            selected = vm.screen == Screen.Today,
            onClick = { go(Screen.Today) },
        )
        Text(
            "Goals", Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp),
            style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        for (g in store.goalOrder()) {
            NavigationDrawerItem(
                label = { Text(displayName(g.name), maxLines = 1) },
                icon = { Text(g.icon) },
                badge = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        PriorityBadge(g.priority)
                        remaining(g.rows).takeIf { it > 0 }?.let { Text("  $it") }
                    }
                },
                selected = vm.screen == Screen.GoalPage(g.id),
                onClick = { go(Screen.GoalPage(g.id)) },
            )
        }
        Spacer(Modifier.height(8.dp))
        HorizontalDivider()
        NavigationDrawerItem(
            label = { Text("New goal") },
            icon = { Ic(R.drawable.ic_add, null) },
            selected = false,
            onClick = { go(Screen.GoalPage(vm.updateWith { it.newGoal("") })) },
        )
        NavigationDrawerItem(
            label = { Text("Import from Notion") },
            icon = { Ic(R.drawable.ic_paste, null) },
            selected = false,
            onClick = { go(vm.screen); vm.openImport() },
        )
        Spacer(Modifier.height(16.dp))
    }
}
