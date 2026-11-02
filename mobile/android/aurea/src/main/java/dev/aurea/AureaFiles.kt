package dev.aurea

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import java.io.File

/**
 * The system document picker and share sheet. Picked files are copied into the app's cache, so
 * Rust gets plain paths, the same as on iOS.
 */
internal class AureaFiles(private val activity: Activity) {

    /** False when nothing can show. */
    fun pick(extensions: Array<String>, multiple: Boolean, id: Long): Boolean {
        val types = extensions.map(::mimeOf)
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
            .addCategory(Intent.CATEGORY_OPENABLE)
            .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple)
        if (types.isEmpty() || types.any { it == ANY }) {
            intent.type = ANY
        } else {
            intent.type = types.singleOrNull() ?: ANY
            intent.putExtra(Intent.EXTRA_MIME_TYPES, types.toTypedArray())
        }
        return start(intent, Pending.Pick(id))
    }

    /** One file picks its name, several pick a folder. */
    fun export(paths: Array<String>, id: Long): Boolean {
        val single = paths.singleOrNull()
        val intent = if (single != null) {
            Intent(Intent.ACTION_CREATE_DOCUMENT)
                .addCategory(Intent.CATEGORY_OPENABLE)
                .setType(typeOf(single))
                .putExtra(Intent.EXTRA_TITLE, File(single).name)
        } else {
            Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
        }
        return paths.isNotEmpty() && start(intent, Pending.Export(id, paths))
    }

    /** Android does not say if the files went anywhere. */
    fun share(paths: Array<String>): Boolean {
        if (paths.isEmpty()) return false
        val folder = File(File(activity.cacheDir, AureaFileProvider.FOLDER), System.nanoTime().toString())
        if (!folder.mkdirs()) return false
        val uris = ArrayList<Uri>()
        for (path in paths) {
            val from = File(path)
            val to = File(folder, from.name)
            try {
                from.copyTo(to)
            } catch (_: java.io.IOException) {
                return false
            }
            uris.add(AureaFileProvider.uriOf(activity, folder.name, to.name))
        }
        val intent = if (uris.size == 1) {
            Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_STREAM, uris[0])
        } else {
            Intent(Intent.ACTION_SEND_MULTIPLE).putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
        }
        intent.type = paths.map(::typeOf).distinct().singleOrNull() ?: ANY
        intent.clipData = ClipData.newRawUri(null, uris[0]).apply {
            uris.drop(1).forEach { addItem(ClipData.Item(it)) }
        }
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        return try {
            activity.startActivity(Intent.createChooser(intent, null))
            true
        } catch (_: android.content.ActivityNotFoundException) {
            false
        }
    }

    fun clipboardText(): String? =
        clipboard().primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(activity)?.toString()

    fun setClipboardText(text: String) {
        clipboard().setPrimaryClip(ClipData.newPlainText("text", text))
    }

    /** PNG bytes of a copied image, up to `max` bytes. */
    fun clipboardPng(max: Long): ByteArray? {
        val uri = clipboard().primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.uri ?: return null
        val resolver = activity.contentResolver
        if (resolver.getType(uri) != "image/png") return null
        return try {
            resolver.openInputStream(uri)?.use { input ->
                val bytes = input.readNBytesCompat(max + 1)
                bytes.takeIf { it.size <= max }
            }
        } catch (_: java.io.IOException) {
            null
        } catch (_: SecurityException) {
            null
        }
    }

    fun setClipboardPng(png: ByteArray): Boolean {
        val folder = File(File(activity.cacheDir, AureaFileProvider.FOLDER), "clipboard")
        if (!folder.isDirectory && !folder.mkdirs()) return false
        val file = File(folder, "${System.nanoTime()}.png")
        return try {
            file.writeBytes(png)
            val uri = AureaFileProvider.uriOf(activity, folder.name, file.name)
            clipboard().setPrimaryClip(ClipData.newUri(activity.contentResolver, "image", uri))
            true
        } catch (_: java.io.IOException) {
            false
        }
    }

    private fun clipboard() = activity.getSystemService(ClipboardManager::class.java)

    private fun java.io.InputStream.readNBytesCompat(limit: Long): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val chunk = ByteArray(64 * 1024)
        while (out.size() < limit) {
            val read = read(chunk)
            if (read < 0) break
            out.write(chunk, 0, read)
        }
        return out.toByteArray()
    }

    /** True when `code` was ours. */
    fun result(code: Int, resultCode: Int, data: Intent?): Boolean {
        val request = pending.remove(code) ?: return false
        val uris = if (resultCode == Activity.RESULT_OK) urisOf(data) else emptyList()
        // copies can be big: off the main thread.
        Thread {
            val paths = try {
                when (request) {
                    is Pending.Pick -> copyIn(uris)
                    is Pending.Export -> copyOut(request.paths, uris)
                }
            } catch (_: java.io.IOException) {
                emptyList()
            } catch (_: SecurityException) {
                emptyList()
            }
            activity.runOnUiThread { nativeFilesDone(request.id, paths.toTypedArray()) }
        }.start()
        return true
    }

    private fun start(intent: Intent, request: Pending): Boolean {
        val code = next
        next = if (next >= LAST_CODE) FIRST_CODE else next + 1
        pending[code] = request
        return try {
            activity.startActivityForResult(intent, code)
            true
        } catch (_: android.content.ActivityNotFoundException) {
            pending.remove(code)
            false
        }
    }

    private fun urisOf(data: Intent?): List<Uri> {
        val clip = data?.clipData
        if (clip != null) return (0 until clip.itemCount).map { clip.getItemAt(it).uri }
        return listOfNotNull(data?.data)
    }

    private fun copyIn(uris: List<Uri>): List<String> {
        val folder = File(File(activity.cacheDir, PICKED), System.nanoTime().toString())
        if (uris.isNotEmpty()) folder.mkdirs()
        return uris.mapIndexed { i, uri ->
            val name = File(displayName(uri) ?: "file").name.ifEmpty { "file" }
            val to = File(folder, if (File(folder, name).exists()) "$i-$name" else name)
            val input = activity.contentResolver.openInputStream(uri)
                ?: throw java.io.FileNotFoundException(uri.toString())
            input.use { from -> to.outputStream().use { from.copyTo(it) } }
            to.path
        }
    }

    private fun copyOut(paths: Array<String>, uris: List<Uri>): List<String> {
        val target = uris.firstOrNull() ?: return emptyList()
        val resolver = activity.contentResolver
        val targets = if (paths.size == 1) {
            listOf(target)
        } else {
            val tree = DocumentsContract.buildDocumentUriUsingTree(
                target,
                DocumentsContract.getTreeDocumentId(target),
            )
            paths.map {
                DocumentsContract.createDocument(resolver, tree, typeOf(it), File(it).name)
                    ?: throw java.io.IOException("no document")
            }
        }
        for ((path, uri) in paths.zip(targets)) {
            val output = resolver.openOutputStream(uri) ?: throw java.io.FileNotFoundException(uri.toString())
            output.use { to -> File(path).inputStream().use { it.copyTo(to) } }
        }
        return targets.map(Uri::toString)
    }

    private fun displayName(uri: Uri): String? =
        activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor: Cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }

    private sealed class Pending(val id: Long) {
        class Pick(id: Long) : Pending(id)
        class Export(id: Long, val paths: Array<String>) : Pending(id)
    }

    private companion object {
        const val ANY = "*/*"
        const val PICKED = "aurea-picked"
        // kept apart from an app's own request codes.
        const val FIRST_CODE = 0xA000
        const val LAST_CODE = 0xAFFF

        // static: the Activity can be recreated while a picker is up.
        val pending = HashMap<Int, Pending>()
        var next = FIRST_CODE

        fun mimeOf(extension: String): String =
            MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension.lowercase()) ?: ANY

        fun typeOf(path: String): String {
            val type = mimeOf(File(path).extension)
            return if (type == ANY) "application/octet-stream" else type
        }

        @JvmStatic external fun nativeFilesDone(id: Long, paths: Array<String>)
    }
}
