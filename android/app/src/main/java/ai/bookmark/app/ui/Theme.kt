// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val Indigo = Color(0xFF4F46E5)
private val Bg = Color(0xFF0D0F14)
private val Surface = Color(0xFF15181F)
private val SurfaceVariant = Color(0xFF1C2033)
private val Text = Color(0xFFE8EAED)
private val Muted = Color(0xFF9AA3AF)

private val Colors = darkColorScheme(
    primary = Indigo,
    onPrimary = Color.White,
    primaryContainer = SurfaceVariant,
    onPrimaryContainer = Text,
    secondary = Color(0xFF7C83FF),
    background = Bg,
    onBackground = Text,
    surface = Surface,
    onSurface = Text,
    surfaceVariant = SurfaceVariant,
    onSurfaceVariant = Muted,
    outline = Color(0xFF333A4A),
    error = Color(0xFFF87171),
)

@Composable
fun BkTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = Colors, content = content)
}
