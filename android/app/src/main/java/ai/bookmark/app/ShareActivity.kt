// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Headless share target. Android starts this activity when the user picks
 * "bookmark" in the Share sheet, or "Save to bookmark" from the text selection
 * menu. It uploads and finishes — no UI.
 */
class ShareActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val prefs = Prefs(this)
        val session = prefs.session()
        if (session == null) {
            toast("Open bookmark and connect to a server first")
            finish()
            return
        }

        val action = intent?.action
        val type = intent?.type.orEmpty()

        when {
            action == Intent.ACTION_PROCESS_TEXT -> {
                val text = intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString()
                sendText(session, text)
            }
            action == Intent.ACTION_SEND && type.startsWith("text/") -> {
                val text = intent.getStringExtra(Intent.EXTRA_TEXT)
                sendText(session, text)
            }
            action == Intent.ACTION_SEND && type.startsWith("image/") -> {
                val uri = if (android.os.Build.VERSION.SDK_INT >= 33) {
                    intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
                }
                sendImage(session, uri)
            }
            action == Intent.ACTION_SEND_MULTIPLE -> {
                val uris = if (android.os.Build.VERSION.SDK_INT >= 33) {
                    intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)
                }
                sendImages(session, uris.orEmpty())
            }
            else -> {
                toast("Nothing to save")
                finish()
            }
        }
    }

    private fun sendText(session: Session, text: String?) {
        if (text.isNullOrBlank()) {
            toast("Nothing to save")
            finish()
            return
        }
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                runCatching {
                    if (text.startsWith("http://") || text.startsWith("https://")) {
                        BkApi.saveText(session.host, session.token, text, null, "")
                    } else {
                        BkApi.saveText(session.host, session.token, null, text, "")
                    }
                }.isSuccess
            }
            toast(if (ok) "Saved to bookmark" else "Could not save")
            finish()
        }
    }

    private fun sendImage(session: Session, uri: Uri?) {
        if (uri == null) {
            toast("Nothing to save")
            finish()
            return
        }
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                runCatching {
                    val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() }
                        ?: return@runCatching
                    BkApi.saveImage(session.host, session.token, bytes, "screenshot.png", "")
                }.isSuccess
            }
            toast(if (ok) "Screenshot saved" else "Could not save")
            finish()
        }
    }

    private fun sendImages(session: Session, uris: List<Uri>) {
        if (uris.isEmpty()) {
            toast("Nothing to save")
            finish()
            return
        }
        lifecycleScope.launch {
            val ok = withContext(Dispatchers.IO) {
                runCatching {
                    for (uri in uris) {
                        val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: continue
                        BkApi.saveImage(session.host, session.token, bytes, "screenshot.png", "")
                    }
                }.isSuccess
            }
            toast(if (ok) "Saved ${uris.size} image(s)" else "Could not save")
            finish()
        }
    }

    private fun toast(message: String) {
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
    }
}
