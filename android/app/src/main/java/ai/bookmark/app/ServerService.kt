// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import java.io.File
import java.security.SecureRandom

/** Shared state for the nerd-flavor server. */
object ServerState {
    @Volatile
    var running: Boolean = false

    val log = StringBuilder()

    fun append(line: String) {
        synchronized(log) {
            log.append(line).append('\n')
            if (log.length > 8000) log.delete(0, log.length - 8000)
        }
    }
}

/**
 * Runs the embedded server in a foreground service (nerd flavor).
 *
 * Android 10+ forbids exec() from app data (W^X), but permits it from
 * `nativeLibraryDir`, where `lib*.so` files from the APK are extracted. The
 * server binary is shipped as `libbookmark.so`, so this path is allowed.
 */
class ServerService : Service() {

    companion object {
        private const val CHANNEL = "bookmark-server"
        private const val NOTIF_ID = 1001

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, ServerService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ServerService::class.java))
        }
    }

    @Volatile
    private var process: Process? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        val nm = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "bookmark server", NotificationManager.IMPORTANCE_LOW)
            )
        }
    }

    private fun notification(text: String): Notification =
        NotificationCompat.Builder(this, CHANNEL)
            .setContentTitle("bookmark")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .build()

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val n = notification("Server running on 127.0.0.1:8080")
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIF_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIF_ID, n)
        }
        if (!ServerState.running) {
            ServerState.running = true
            Thread({ runServer() }, "bookmark-server").start()
        }
        return START_STICKY
    }

    private fun runServer() {
        try {
            val exe = File(applicationInfo.nativeLibraryDir, "libbookmark.so")
            if (!exe.exists()) {
                ServerState.append("This build has no embedded server binary.")
                return
            }
            val dataDir = File(filesDir, "data")
            dataDir.mkdirs()
            File(dataDir, "models").mkdirs()

            val cfg = File(filesDir, "config.toml")
            cfg.writeText(
                buildString {
                    append("data_dir = \"").append(dataDir.absolutePath).append("\"\n")
                    append("models_dir = \"").append(File(dataDir, "models").absolutePath).append("\"\n")
                    append("host = \"127.0.0.1\"\n")
                    append("port = 8080\n")
                    append("open_registration = false\n")
                }
            )

            val prefs = Prefs(this)
            if (prefs.adminPassword.isEmpty()) {
                val pw = randomPassword()
                prefs.adminPassword = pw
                exec(exe, listOf("init"), cfg, pw, wait = true)
            }
            exec(exe, listOf("serve", "--host", "127.0.0.1", "--port", "8080"), cfg, null, wait = true)
        } catch (e: Exception) {
            ServerState.append("error: ${e.message}")
        } finally {
            ServerState.running = false
        }
    }

    private fun exec(exe: File, args: List<String>, cfg: File, adminPw: String?, wait: Boolean) {
        val pb = ProcessBuilder(listOf(exe.absolutePath) + args)
        pb.environment()["HOME"] = filesDir.absolutePath
        pb.environment()["BOOKMARK_CONFIG"] = cfg.absolutePath
        pb.environment()["LD_LIBRARY_PATH"] = applicationInfo.nativeLibraryDir
        if (adminPw != null) pb.environment()["BOOKMARK_ADMIN_PASSWORD"] = adminPw
        pb.redirectErrorStream(true)
        val p = pb.start()
        process = p
        Thread({
            p.inputStream.bufferedReader().forEachLine { ServerState.append(it) }
        }, "bookmark-log").start()
        if (wait) p.waitFor()
    }

    private fun randomPassword(): String {
        val alphabet = "abcdefghjkmnpqrstuvwxyzABCDEFGHJKMNPQRSTUVWXYZ23456789"
        val rnd = SecureRandom()
        return (0 until 18).map { alphabet[rnd.nextInt(alphabet.length)] }.joinToString("")
    }

    override fun onDestroy() {
        process?.destroy()
        process = null
        ServerState.running = false
        super.onDestroy()
    }
}
