package dev.vamsi.planner

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import dev.vamsi.planner.ui.PlannerApp
import dev.vamsi.planner.ui.PlannerViewModel
import dev.vamsi.planner.ui.theme.PlannerTheme

class MainActivity : ComponentActivity() {
    private val vm: PlannerViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) handleShare(intent)
        setContent { PlannerTheme { PlannerApp(vm) } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleShare(intent)
    }

    /** Text shared from Notion (or anything) opens the importer with it. */
    private fun handleShare(intent: Intent?) {
        if (intent?.action == Intent.ACTION_SEND) {
            intent.getStringExtra(Intent.EXTRA_TEXT)?.let { vm.openImport(it) }
        }
    }

    override fun onResume() {
        super.onResume()
        vm.refreshToday()
        vm.drive.syncNow()
    }

    override fun onStop() {
        super.onStop()
        vm.flush()
    }
}
