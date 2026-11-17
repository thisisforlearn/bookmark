// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import ai.bookmark.app.ui.BkApp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = Prefs(this)
        setContent { BkApp(prefs) }
    }
}
