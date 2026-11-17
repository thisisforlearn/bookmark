// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app

import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/**
 * Minimal HTTP client. Deliberately uses HttpURLConnection + org.json so the APK
 * carries no OkHttp, Retrofit, or serialization libraries.
 */
object BkApi {

    data class Hit(
        val id: Long,
        val title: String,
        val url: String,
        val site: String,
        val snippet: String,
        val tags: List<String>,
        val pinned: Boolean,
    )

    fun normalizeHost(raw: String): String {
        var h = raw.trim()
        if (h.isEmpty()) return h
        if (!h.startsWith("http://") && !h.startsWith("https://")) h = "http://$h"
        return h.trimEnd('/')
    }

    private fun open(host: String, path: String, method: String, token: String?): HttpURLConnection {
        val url = URL(normalizeHost(host) + path)
        val c = url.openConnection() as HttpURLConnection
        c.requestMethod = method
        c.connectTimeout = 10_000
        c.readTimeout = 25_000
        c.setRequestProperty("Accept", "application/json")
        if (token != null) c.setRequestProperty("Authorization", "Bearer $token")
        return c
    }

    private fun body(c: HttpURLConnection): String {
        val stream = if (c.responseCode in 200..299) c.inputStream else c.errorStream
        val text = stream?.bufferedReader()?.use { it.readText() } ?: ""
        if (c.responseCode !in 200..299) {
            val msg = try {
                JSONObject(text).optString("error", "HTTP ${c.responseCode}")
            } catch (_: Exception) {
                "HTTP ${c.responseCode}"
            }
            throw RuntimeException(msg)
        }
        return text
    }

    fun pair(host: String, code: String, name: String): Session {
        val form = "code=${enc(code)}&name=${enc(name)}"
        val c = open(host, "/api/v1/pair", "POST", null)
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
        c.outputStream.use { it.write(form.toByteArray()) }
        val json = JSONObject(body(c))
        return Session(normalizeHost(host), json.getString("token"), json.optString("user"))
    }

    fun whoami(host: String, token: String): String? = try {
        val c = open(host, "/api/v1/whoami", "GET", token)
        JSONObject(body(c)).optString("user", null)
    } catch (_: Exception) {
        null
    }

    fun saveText(host: String, token: String, url: String?, text: String?, tags: String): Long {
        val c = open(host, "/api/v1/save", "POST", token)
        val boundary = "----bk" + System.currentTimeMillis()
        c.doOutput = true
        c.setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
        val out = ByteArrayOutputStream()
        fun part(name: String, value: String) {
            out.write("--$boundary\r\n".toByteArray())
            out.write("Content-Disposition: form-data; name=\"$name\"\r\n\r\n".toByteArray())
            out.write(value.toByteArray())
            out.write("\r\n".toByteArray())
        }
        if (!url.isNullOrBlank()) part("url", url)
        if (!text.isNullOrBlank()) part("text", text)
        if (tags.isNotBlank()) part("tags", tags)
        out.write("--$boundary--\r\n".toByteArray())
        c.outputStream.use { it.write(out.toByteArray()) }
        return JSONObject(body(c)).optLong("id")
    }

    fun saveImage(host: String, token: String, bytes: ByteArray, filename: String, tags: String): Long {
        val c = open(host, "/api/v1/save", "POST", token)
        val boundary = "----bk" + System.currentTimeMillis()
        c.doOutput = true
        c.setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
        val out = ByteArrayOutputStream()
        if (tags.isNotBlank()) {
            out.write("--$boundary\r\n".toByteArray())
            out.write("Content-Disposition: form-data; name=\"tags\"\r\n\r\n".toByteArray())
            out.write(tags.toByteArray())
            out.write("\r\n".toByteArray())
        }
        out.write("--$boundary\r\n".toByteArray())
        out.write(
            "Content-Disposition: form-data; name=\"image\"; filename=\"$filename\"\r\n".toByteArray()
        )
        out.write("Content-Type: image/*\r\n\r\n".toByteArray())
        out.write(bytes)
        out.write("\r\n--$boundary--\r\n".toByteArray())
        c.outputStream.use { it.write(out.toByteArray()) }
        return JSONObject(body(c)).optLong("id")
    }

    fun search(host: String, token: String, query: String): List<Hit> {
        val c = open(host, "/api/v1/search?q=${enc(query)}", "GET", token)
        return parseHits(JSONObject(body(c)).optJSONArray("hits"))
    }

    fun list(host: String, token: String): List<Hit> {
        val c = open(host, "/api/v1/bookmarks", "GET", token)
        return parseHits(JSONArray(body(c)))
    }

    private fun parseHits(arr: JSONArray?): List<Hit> {
        if (arr == null) return emptyList()
        val out = ArrayList<Hit>(arr.length())
        for (i in 0 until arr.length()) {
            val o = arr.getJSONObject(i)
            val tags = ArrayList<String>()
            val t = o.optJSONArray("tags")
            if (t != null) for (j in 0 until t.length()) tags.add(t.getString(j))
            out.add(
                Hit(
                    id = o.optLong("id"),
                    title = o.optString("title"),
                    url = o.optString("url"),
                    site = o.optString("site"),
                    snippet = o.optString("snippet"),
                    tags = tags,
                    pinned = o.optBoolean("pinned"),
                )
            )
        }
        return out
    }

    private fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")
}
