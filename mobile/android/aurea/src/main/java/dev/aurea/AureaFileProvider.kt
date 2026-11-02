package dev.aurea

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import java.io.File
import java.io.FileNotFoundException

/**
 * Lends files to the app a share went to, read only, and only from [FOLDER] in the cache. Not
 * exported: only a URI granted by the share opens.
 */
class AureaFileProvider : ContentProvider() {

    override fun onCreate(): Boolean = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw FileNotFoundException("read only")
        return ParcelFileDescriptor.open(fileOf(uri), ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(
        uri: Uri,
        projection: Array<String>?,
        selection: String?,
        selectionArgs: Array<String>?,
        sortOrder: String?,
    ): Cursor {
        val file = fileOf(uri)
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        val known = columns.filter { it == OpenableColumns.DISPLAY_NAME || it == OpenableColumns.SIZE }
        val cursor = MatrixCursor(known.toTypedArray())
        cursor.addRow(known.map { if (it == OpenableColumns.SIZE) file.length() else file.name })
        return cursor
    }

    override fun getType(uri: Uri): String {
        val extension = File(uri.path.orEmpty()).extension.lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            ?: "application/octet-stream"
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<String>?,
    ): Int = 0

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<String>?): Int = 0

    private fun fileOf(uri: Uri): File {
        val context = context ?: throw FileNotFoundException()
        val root = File(context.cacheDir, FOLDER).canonicalFile
        val file = File(root, uri.path.orEmpty()).canonicalFile
        if (!file.path.startsWith(root.path + File.separator) || !file.isFile) {
            throw FileNotFoundException()
        }
        return file
    }

    internal companion object {
        const val FOLDER = "aurea-share"

        fun uriOf(context: Context, folder: String, name: String): Uri = Uri.Builder()
            .scheme("content")
            .authority(context.packageName + ".aurea.files")
            .appendPath(folder)
            .appendPath(name)
            .build()
    }
}
