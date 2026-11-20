package dev.proflyder.home.feature.chat.data

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.ChatClient
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal sealed interface Prepared {
    data class Ready(val attachment: LocalAttachment) : Prepared

    data class Rejected(val key: String) : Prepared
}

internal const val REJECT_TOO_LARGE = "chat_attachment_too_large"
internal const val REJECT_UNSUPPORTED = "chat_attachment_unsupported"

@SingleIn(AppScope::class)
@Inject
class ChatMedia(
    private val context: Context,
    private val client: ChatClient,
) {
    private val directory: File by lazy { File(context.filesDir, MEDIA_DIR).apply { mkdirs() } }

    internal fun newFile(extension: String): File = File(directory, "local-${UUID.randomUUID()}.$extension")

    internal suspend fun fileFor(id: String): File? = withContext(Dispatchers.IO) {
        val cached = File(directory, id)
        when {
            cached.isFile -> cached
            else -> downloadTo(id, cached)
        }
    }

    private suspend fun downloadTo(id: String, target: File): File? =
        when (val result = client.download(id)) {
            is Outcome.Success -> target.also { it.writeBytes(result.value) }
            is Outcome.Failure -> null
        }

    internal suspend fun prepareImage(uri: Uri): Prepared = withContext(Dispatchers.IO) { imageFrom(uri) }

    internal suspend fun prepareFile(uri: Uri): Prepared = withContext(Dispatchers.IO) { fileFrom(uri) }

    private fun imageFrom(uri: Uri): Prepared {
        val bitmap = decodeScaled(uri) ?: return Prepared.Rejected(REJECT_UNSUPPORTED)
        val file = newFile(JPEG_EXTENSION)
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, it) }
        bitmap.recycle()
        return Prepared.Ready(LocalAttachment(file.path, ChatAttachmentKind.IMAGE, JPEG_MIME, file.name, file.length()))
    }

    private fun fileFrom(uri: Uri): Prepared {
        val mime = context.contentResolver.getType(uri)?.lowercase().orEmpty()
        val bytes = readLimited(uri)
        return when {
            mime !in TEXT_MIMES -> Prepared.Rejected(REJECT_UNSUPPORTED)
            bytes == null -> Prepared.Rejected(REJECT_TOO_LARGE)
            else -> Prepared.Ready(saveFile(bytes, mime, displayName(uri)))
        }
    }

    private fun saveFile(bytes: ByteArray, mime: String, name: String): LocalAttachment {
        val file = newFile(TEXT_EXTENSION)
        file.writeBytes(bytes)
        return LocalAttachment(file.path, ChatAttachmentKind.FILE, mime, name, bytes.size.toLong())
    }

    private fun readLimited(uri: Uri): ByteArray? {
        val bytes = context.contentResolver.openInputStream(uri)?.use { readUpTo(it, MAX_FILE_BYTES + 1) }
        return bytes?.takeIf { it.size <= MAX_FILE_BYTES }
    }

    private fun readUpTo(stream: InputStream, limit: Int): ByteArray {
        val out = ByteArrayOutputStream()
        val buffer = ByteArray(BUFFER_BYTES)
        while (out.size() < limit) {
            val read = stream.read(buffer, 0, minOf(buffer.size, limit - out.size()))
            if (read < 0) break
            out.write(buffer, 0, read)
        }
        return out.toByteArray()
    }

    private fun displayName(uri: Uri): String =
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0).orEmpty() else ""
        }.orEmpty()

    private fun decodeScaled(uri: Uri): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        val largest = maxOf(bounds.outWidth, bounds.outHeight)
        if (largest <= 0) return null
        val options = BitmapFactory.Options().apply { inSampleSize = sampleSize(largest) }
        return context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, options) }
    }

    private fun sampleSize(largest: Int): Int =
        generateSequence(1) { it * 2 }.first { largest / it <= MAX_IMAGE_SIDE }

    private companion object {
        const val MEDIA_DIR = "chat-media"
        const val JPEG_EXTENSION = "jpg"
        const val TEXT_EXTENSION = "txt"
        const val JPEG_MIME = "image/jpeg"
        const val JPEG_QUALITY = 85
        const val MAX_IMAGE_SIDE = 2048
        const val MAX_FILE_BYTES = 256 * 1024
        const val BUFFER_BYTES = 8192
        val TEXT_MIMES = setOf("text/plain", "text/markdown", "text/csv", "application/json", "text/x-log")
    }
}
