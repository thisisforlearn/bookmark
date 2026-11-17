// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

import ai.bookmark.app.BkApi
import ai.bookmark.app.BuildConfig
import ai.bookmark.app.Prefs
import ai.bookmark.app.ServerService
import ai.bookmark.app.ServerState
import ai.bookmark.app.Session
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private enum class Tab(val label: String, val icon: ImageVector) {
    Library("Library", Icons.Default.Home),
    Add("Add", Icons.Default.Add),
    Capture("Capture", Icons.Default.Share),
    Settings("Settings", Icons.Default.Settings),
}

@Composable
fun BkApp(prefs: Prefs) {
    var session by remember { mutableStateOf(prefs.session()) }
    BkTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            val current = session
            if (current == null) {
                Onboarding { s ->
                    prefs.save(s)
                    session = s
                }
            } else {
                Home(
                    session = current,
                    prefs = prefs,
                    onSignOut = {
                        prefs.clearSession()
                        session = null
                    },
                )
            }
        }
    }
}

// ------------------------------------------------------------- onboarding ---

@Composable
private fun Onboarding(onConnected: (Session) -> Unit) {
    val scope = rememberCoroutineScope()
    var host by rememberSaveable { mutableStateOf("") }
    var code by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically),
    ) {
        Text("◆ bookmark", style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.primary)
        Text("Save links and screenshots from any app.", style = MaterialTheme.typography.bodyLarge)
        Text(
            "On your server open Settings → Connect a phone to get a code.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        OutlinedTextField(
            value = host,
            onValueChange = { host = it },
            label = { Text("Server address") },
            placeholder = { Text("192.168.1.10:8080") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = code,
            onValueChange = { code = it.uppercase().take(8) },
            label = { Text("Pairing code") },
            placeholder = { Text("8 characters") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }

        Button(
            onClick = {
                busy = true
                error = null
                scope.launch {
                    try {
                        val s = withContext(Dispatchers.IO) {
                            BkApi.pair(host, code.trim(), "phone")
                        }
                        onConnected(s)
                    } catch (e: Exception) {
                        error = e.message ?: "Could not connect"
                    } finally {
                        busy = false
                    }
                }
            },
            enabled = !busy && host.isNotBlank() && code.length >= 6,
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (busy) {
                CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
            } else {
                Text("Connect")
            }
        }
    }
}

// ------------------------------------------------------------------- home ---

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Home(session: Session, prefs: Prefs, onSignOut: () -> Unit) {
    var tab by rememberSaveable { mutableStateOf(Tab.Library) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("bookmark", fontWeight = FontWeight.SemiBold) },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
        bottomBar = {
            NavigationBar(containerColor = MaterialTheme.colorScheme.surface) {
                Tab.entries.forEach { t ->
                    NavigationBarItem(
                        selected = tab == t,
                        onClick = { tab = t },
                        icon = { Icon(t.icon, contentDescription = t.label) },
                        label = { Text(t.label) },
                    )
                }
            }
        },
        containerColor = MaterialTheme.colorScheme.background,
    ) { pad ->
        Box(modifier = Modifier.padding(pad)) {
            when (tab) {
                Tab.Library -> LibraryScreen(session)
                Tab.Add -> AddScreen(session)
                Tab.Capture -> CaptureScreen(session)
                Tab.Settings -> SettingsScreen(session, prefs, onSignOut)
            }
        }
    }
}

// ---------------------------------------------------------------- library ---

@Composable
private fun LibraryScreen(session: Session) {
    val scope = rememberCoroutineScope()
    var query by rememberSaveable { mutableStateOf("") }
    var hits by remember { mutableStateOf<List<BkApi.Hit>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }

    fun load(q: String) {
        scope.launch {
            loading = true
            error = null
            try {
                hits = withContext(Dispatchers.IO) {
                    if (q.isBlank()) BkApi.list(session.host, session.token)
                    else BkApi.search(session.host, session.token, q)
                }
            } catch (e: Exception) {
                error = e.message ?: "Could not load"
            } finally {
                loading = false
            }
        }
    }

    LaunchedEffect(Unit) { load("") }

    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                placeholder = { Text("Search your library…") },
                singleLine = true,
                modifier = Modifier.weight(1f),
            )
            IconButton(onClick = { load(query) }) { Icon(Icons.Default.Search, "Search") }
            IconButton(onClick = { load(query) }) { Icon(Icons.Default.Refresh, "Refresh") }
        }

        if (loading) LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
        error?.let {
            Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(16.dp))
        }

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(hits, key = { it.id }) { hit -> HitCard(hit) }
            if (hits.isEmpty() && !loading) {
                item {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(40.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text("Nothing here yet", style = MaterialTheme.typography.titleMedium)
                        Text(
                            "Share a link to bookmark, or use the Add tab.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun HitCard(hit: BkApi.Hit) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(modifier = Modifier.padding(14.dp)) {
            Text(
                hit.title.ifBlank { hit.url },
                style = MaterialTheme.typography.titleMedium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (hit.site.isNotBlank() || hit.url.isNotBlank()) {
                Text(
                    hit.site.ifBlank { hit.url },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            val snippet = plain(hit.snippet)
            if (snippet.isNotBlank()) {
                Text(
                    snippet,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }
            if (hit.tags.isNotEmpty()) {
                Text(
                    hit.tags.joinToString("  ") { "#$it" },
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }
        }
    }
}

private fun plain(html: String): String =
    html.replace(Regex("<[^>]*>"), "")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")

// -------------------------------------------------------------------- add ---

@Composable
private fun AddScreen(session: Session) {
    val scope = rememberCoroutineScope()
    var target by rememberSaveable { mutableStateOf("") }
    var tags by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<String?>(null) }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Add a bookmark", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(
            value = target,
            onValueChange = { target = it },
            label = { Text("URL or text") },
            modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp),
        )
        OutlinedTextField(
            value = tags,
            onValueChange = { tags = it },
            label = { Text("Tags (comma separated)") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Button(
            onClick = {
                busy = true
                message = null
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) {
                            val t = target.trim()
                            if (t.startsWith("http://") || t.startsWith("https://")) {
                                BkApi.saveText(session.host, session.token, t, null, tags)
                            } else {
                                BkApi.saveText(session.host, session.token, null, t, tags)
                            }
                        }
                        message = "Saved"
                        target = ""
                        tags = ""
                    } catch (e: Exception) {
                        message = e.message ?: "Could not save"
                    } finally {
                        busy = false
                    }
                }
            },
            enabled = !busy && target.isNotBlank(),
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (busy) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
            else Text("Save")
        }
        message?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

// ---------------------------------------------------------------- capture ---

@Composable
private fun CaptureScreen(session: Session) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    var busy by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<String?>(null) }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        busy = true
        message = null
        scope.launch {
            try {
                val bytes = withContext(Dispatchers.IO) {
                    context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                }
                if (bytes == null) {
                    message = "Could not read that image"
                } else {
                    withContext(Dispatchers.IO) {
                        BkApi.saveImage(session.host, session.token, bytes, "screenshot.png", "")
                    }
                    message = "Screenshot saved"
                }
            } catch (e: Exception) {
                message = e.message ?: "Could not upload"
            } finally {
                busy = false
            }
        }
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Capture", style = MaterialTheme.typography.titleLarge)
        Text(
            "Take a screenshot in any app, tap Markup to circle or draw, then Share → bookmark. " +
                "Or pick an image here.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Button(
            onClick = { picker.launch("image/*") },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Choose a screenshot") }

        Button(
            onClick = {
                val text = clipboard.getText()?.text
                if (text.isNullOrBlank()) {
                    message = "Clipboard is empty"
                    return@Button
                }
                busy = true
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) {
                            BkApi.saveText(session.host, session.token, null, text, "")
                        }
                        message = "Clipboard saved"
                    } catch (e: Exception) {
                        message = e.message ?: "Could not save"
                    } finally {
                        busy = false
                    }
                }
            },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Save clipboard text") }

        if (busy) LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
        message?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

// --------------------------------------------------------------- settings ---

@Composable
private fun SettingsScreen(session: Session, prefs: Prefs, onSignOut: () -> Unit) {
    val context = LocalContext.current
    var running by remember { mutableStateOf(ServerState.running) }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Settings", style = MaterialTheme.typography.titleLarge)
        InfoRow("Server", session.host)
        InfoRow("Signed in as", session.user)

        if (BuildConfig.FLAVOR == "nerd") {
            Text("On-device server", style = MaterialTheme.typography.titleMedium)
            Text(
                if (running) "Running at http://127.0.0.1:8080" else "Stopped",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = {
                        ServerService.start(context)
                        running = true
                    },
                    enabled = !running,
                ) { Text("Start") }
                Button(
                    onClick = {
                        ServerService.stop(context)
                        running = false
                    },
                    enabled = running,
                ) { Text("Stop") }
            }
            if (prefs.adminPassword.isNotEmpty()) {
                Text(
                    "Admin password: ${prefs.adminPassword}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text(
                "Open http://127.0.0.1:8080, sign in, then Settings → Connect a phone to pair this app.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        Button(onClick = onSignOut, modifier = Modifier.fillMaxWidth()) { Text("Sign out") }
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    Column {
        Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, style = MaterialTheme.typography.bodyMedium)
    }
}
