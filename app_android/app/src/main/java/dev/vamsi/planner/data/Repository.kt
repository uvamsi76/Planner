package dev.vamsi.planner.data

import android.content.Context
import java.io.File

/**
 * Stores the data as `files/data.json` in app-private storage (same JSON as
 * the desktop app). Writes are atomic: temp file + rename. Android Auto
 * Backup includes this file (see res/xml/backup_rules.xml).
 */
class Repository(context: Context) {
    private val file = File(context.filesDir, "data.json")
    private val prefs = context.getSharedPreferences("layout", Context.MODE_PRIVATE)

    fun load(): Store = try {
        if (file.exists()) Store.fromJson(file.readText()) else Store.sample().also { save(it) }
    } catch (e: Exception) {
        // Don't overwrite a file we couldn't read; keep it aside and start fresh.
        file.copyTo(File(file.parentFile, "data.unreadable.json"), overwrite = true)
        Store()
    }

    @Synchronized
    fun save(store: Store) {
        val tmp = File(file.parentFile, "data.json.tmp")
        tmp.writeText(store.toJson())
        if (!tmp.renameTo(file)) {
            file.writeText(store.toJson())
            tmp.delete()
        }
    }

    // Column widths are a per-device layout choice, so they live in prefs, not in the data.
    fun columnWidths(gid: Long): Triple<Float, Float, Float>? {
        val v = prefs.getString("cols_$gid", null)?.split(',')?.mapNotNull { it.toFloatOrNull() }
        return if (v != null && v.size == 3) Triple(v[0], v[1], v[2]) else null
    }

    fun saveColumnWidths(gid: Long, date: Float, task: Float, notes: Float) {
        prefs.edit().putString("cols_$gid", "$date,$task,$notes").apply()
    }
}
