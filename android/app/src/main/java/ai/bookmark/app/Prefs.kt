// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app

import android.content.Context

data class Session(val host: String, val token: String, val user: String)

class Prefs(context: Context) {
    private val sp = context.getSharedPreferences("bookmark", Context.MODE_PRIVATE)

    var host: String
        get() = sp.getString("host", "") ?: ""
        set(v) = sp.edit().putString("host", v).apply()

    var token: String
        get() = sp.getString("token", "") ?: ""
        set(v) = sp.edit().putString("token", v).apply()

    var user: String
        get() = sp.getString("user", "") ?: ""
        set(v) = sp.edit().putString("user", v).apply()

    /** Password generated for the embedded server on first run (nerd flavor). */
    var adminPassword: String
        get() = sp.getString("admin_pw", "") ?: ""
        set(v) = sp.edit().putString("admin_pw", v).apply()

    fun session(): Session? {
        if (host.isBlank() || token.isBlank()) return null
        return Session(host, token, user)
    }

    fun save(s: Session) {
        host = s.host
        token = s.token
        user = s.user
    }

    fun clearSession() {
        token = ""
        user = ""
    }
}
