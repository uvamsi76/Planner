package dev.vamsi.planner.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

// Neutral, Notion-like fallback palette (used before Android 12; later the
// system's own colours are used, like the desktop app follows the GNOME accent).
private val Light = lightColorScheme(
    primary = Color(0xFF2F6FEB),
    background = Color(0xFFFFFFFF),
    surface = Color(0xFFFFFFFF),
    surfaceVariant = Color(0xFFF1F1EF),
    onBackground = Color(0xFF37352F),
    onSurface = Color(0xFF37352F),
)
private val Dark = darkColorScheme(
    primary = Color(0xFF7AA2F7),
    background = Color(0xFF191919),
    surface = Color(0xFF191919),
    surfaceVariant = Color(0xFF2A2A2A),
    onBackground = Color(0xFFE6E6E4),
    onSurface = Color(0xFFE6E6E4),
)

val PlannerType = Typography().let { t ->
    t.copy(
        displaySmall = t.displaySmall.copy(fontWeight = FontWeight.ExtraBold, fontSize = 32.sp, lineHeight = 38.sp),
        headlineMedium = t.headlineMedium.copy(fontWeight = FontWeight.ExtraBold),
        titleMedium = t.titleMedium.copy(fontWeight = FontWeight.Bold),
    )
}

@Composable
fun PlannerTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val ctx = LocalContext.current
    val scheme = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
        dark -> Dark
        else -> Light
    }
    MaterialTheme(colorScheme = scheme, typography = PlannerType, content = content)
}

/** Priority colours shared with the desktop app. */
fun priorityColor(p: Int, fallback: Color): Color = when (p) {
    1 -> Color(0xFFE5484D)
    2 -> Color(0xFFF08C28)
    3 -> Color(0xFF3E8EF7)
    else -> fallback
}

/** Day-plan palette shared with the desktop app. */
val Palette = listOf(
    Color(0xFF4F8CF7), Color(0xFFF58C33), Color(0xFF38B878), Color(0xFFDB5794), Color(0xFF9473E0),
    Color(0xFF2EB3C2), Color(0xFFEDBD33), Color(0xFFE35C54), Color(0xFF80A347), Color(0xFF8F91A8),
)
