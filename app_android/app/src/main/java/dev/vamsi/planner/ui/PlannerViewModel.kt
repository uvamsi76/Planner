package dev.vamsi.planner.ui

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.vamsi.planner.data.Repository
import dev.vamsi.planner.data.Store
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.LocalDate

sealed interface Screen {
    data object Today : Screen
    data class GoalPage(val id: Long) : Screen
}

/** What dragging on the day-plan clock paints. */
sealed interface Brush {
    data object None : Brush
    data object Eraser : Brush
    data class Row(val id: Long) : Brush
}

/** A message with an Undo action, shown as a snackbar. */
data class UndoEvent(val message: String, val undo: (() -> Unit)?, val stamp: Long = System.nanoTime())

/**
 * All app state. The data is one immutable [Store]; every change replaces it
 * (Compose recomposes what read it) and schedules a debounced save.
 */
class PlannerViewModel(app: Application) : AndroidViewModel(app) {
    val repo = Repository(app)

    var store by mutableStateOf(repo.load())
        private set
    var screen by mutableStateOf<Screen>(Screen.Today)
    var today by mutableStateOf(LocalDate.now())
        private set
    /** Day shown on the Today page. */
    var date by mutableStateOf(today)
    var brush by mutableStateOf<Brush>(Brush.None)
    var quickOpen by mutableStateOf(false)
    var hidePast by mutableStateOf(false)
    var quickGoal by mutableStateOf<Long?>(null)
    /** Text waiting for the import dialog (from the share sheet), or "" to open it empty. */
    var importText by mutableStateOf<String?>(null)
    var importTarget by mutableStateOf<Long?>(null)
    var undoEvent by mutableStateOf<UndoEvent?>(null)

    private var saveJob: Job? = null

    /** Google Drive sync; applying Drive's copy saves locally without marking it dirty. */
    val drive = DriveSync(
        app = app,
        scope = viewModelScope,
        currentStore = { store },
        applyStore = { s ->
            store = s
            saveJob?.cancel()
            viewModelScope.launch(Dispatchers.IO) { repo.save(s) }
        },
        notify = { toastUndo(it) },
    )

    fun update(f: (Store) -> Store) {
        store = f(store)
        scheduleSave()
    }

    /** Update and return a value from the new state (e.g. a new id). */
    fun <T> updateWith(f: (Store) -> Pair<Store, T>): T {
        val (s, value) = f(store)
        store = s
        scheduleSave()
        return value
    }

    /** Swap in a whole store (e.g. after undo). */
    fun replace(s: Store) {
        store = s
        scheduleSave()
    }

    fun toastUndo(message: String, undo: (() -> Unit)? = null) {
        undoEvent = UndoEvent(message, undo)
    }

    private fun scheduleSave() {
        saveJob?.cancel()
        val snapshot = store
        saveJob = viewModelScope.launch {
            delay(400)
            withContext(Dispatchers.IO) { repo.save(snapshot) }
            drive.localSaved()
        }
    }

    /** Write now (app going to background). */
    fun flush() {
        val pending = saveJob?.isActive == true // read before cancelling
        saveJob?.cancel()
        val snapshot = store
        viewModelScope.launch(NonCancellable + Dispatchers.IO) { repo.save(snapshot) }
        if (pending) drive.localSaved()
        drive.flush()
    }

    /** Follow the calendar past midnight; keeps "today" in step on resume. */
    fun refreshToday() {
        val now = LocalDate.now()
        if (now != today) {
            if (date == today) date = now
            today = now
        }
    }

    fun openImport(text: String = "", target: Long? = null) {
        importTarget = target
        importText = text
    }
}
