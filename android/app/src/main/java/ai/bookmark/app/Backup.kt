// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

package ai.bookmark.app

import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

/**
 * Backup/restore of the on-device library. The whole server data directory is
 * zipped (SQLite database plus models) so a restore is a full rollback.
 */
object Backup {

    fun zip(dir: File, out: OutputStream) {
        ZipOutputStream(BufferedOutputStream(out)).use { zos ->
            val base = dir.toPath()
            dir.walkTopDown().filter { it.isFile }.forEach { f ->
                val relative = base.relativize(f.toPath()).toString()
                zos.putNextEntry(ZipEntry(relative))
                f.inputStream().use { it.copyTo(zos) }
                zos.closeEntry()
            }
        }
    }

    fun unzip(dir: File, input: InputStream) {
        ZipInputStream(BufferedInputStream(input)).use { zis ->
            var entry = zis.nextEntry
            while (entry != null) {
                val target = File(dir, entry.name)
                // Reject paths that escape the destination (zip-slip).
                if (target.canonicalPath.startsWith(dir.canonicalPath + File.separator)) {
                    target.parentFile?.mkdirs()
                    if (!entry.isDirectory) {
                        target.outputStream().use { zis.copyTo(it) }
                    }
                }
                zis.closeEntry()
                entry = zis.nextEntry
            }
        }
    }
}
