package dev.proflyder.home.feature.chat.data

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.SystemClock
import dev.proflyder.home.core.model.ChatAttachmentKind
import java.io.ByteArrayOutputStream
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal object WavWriter {
    const val SAMPLE_RATE = 16_000
    private const val HEADER_BYTES = 44
    private const val BYTES_PER_SAMPLE = 2

    fun encode(pcm: ByteArray): ByteArray {
        val out = ByteArray(HEADER_BYTES + pcm.size)
        ascii(out, 0, "RIFF")
        int32(out, 4, 36 + pcm.size)
        ascii(out, 8, "WAVEfmt ")
        int32(out, 16, 16)
        int16(out, 20, 1)
        int16(out, 22, 1)
        int32(out, 24, SAMPLE_RATE)
        int32(out, 28, SAMPLE_RATE * BYTES_PER_SAMPLE)
        int16(out, 32, BYTES_PER_SAMPLE)
        int16(out, 34, 16)
        ascii(out, 36, "data")
        int32(out, 40, pcm.size)
        pcm.copyInto(out, HEADER_BYTES)
        return out
    }

    fun durationMillis(pcmBytes: Int): Long = pcmBytes / BYTES_PER_SAMPLE * 1000L / SAMPLE_RATE

    private fun ascii(out: ByteArray, at: Int, text: String) {
        text.toByteArray(Charsets.US_ASCII).copyInto(out, at)
    }

    private fun int16(out: ByteArray, at: Int, value: Int) {
        out[at] = value.toByte()
        out[at + 1] = (value shr 8).toByte()
    }

    private fun int32(out: ByteArray, at: Int, value: Int) {
        int16(out, at, value)
        int16(out, at + 2, value shr 16)
    }
}

internal class VoiceRecorder(
    private val newFile: () -> File,
    private val clock: () -> Long = SystemClock::elapsedRealtime,
) {
    private var job: Job? = null
    private var buffer = ByteArrayOutputStream()
    private var startedAt = 0L

    val recording: Boolean get() = job?.isActive == true

    fun elapsedMillis(): Long = if (recording) clock() - startedAt else 0L

    @SuppressLint("MissingPermission")
    fun start(scope: CoroutineScope): Boolean {
        val minimum = AudioRecord.getMinBufferSize(WavWriter.SAMPLE_RATE, CHANNEL, ENCODING)
        if (minimum <= 0 || recording) return false
        val record = AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, WavWriter.SAMPLE_RATE, CHANNEL, ENCODING, minimum * 2)
        if (record.state != AudioRecord.STATE_INITIALIZED) return false.also { record.release() }
        buffer = ByteArrayOutputStream()
        startedAt = clock()
        record.startRecording()
        job = scope.launch(Dispatchers.IO) { pump(record, minimum) }
        return true
    }

    private suspend fun pump(record: AudioRecord, size: Int) {
        val chunk = ByteArray(size)
        while (currentCoroutineContext()[Job]?.isActive == true) {
            val read = record.read(chunk, 0, chunk.size)
            if (read > 0) buffer.write(chunk, 0, read)
        }
        record.stop()
        record.release()
    }

    suspend fun stop(): LocalAttachment? {
        val running = job ?: return null
        running.cancel()
        running.join()
        job = null
        val pcm = buffer.toByteArray()
        if (WavWriter.durationMillis(pcm.size) < MIN_MILLIS) return null
        return withContext(Dispatchers.IO) { save(pcm) }
    }

    fun cancel() {
        job?.cancel()
        job = null
    }

    private fun save(pcm: ByteArray): LocalAttachment {
        val file = newFile()
        file.writeBytes(WavWriter.encode(pcm))
        return LocalAttachment(
            path = file.path,
            kind = ChatAttachmentKind.VOICE,
            mime = WAV_MIME,
            name = file.name,
            size = file.length(),
            durationMs = WavWriter.durationMillis(pcm.size),
        )
    }

    companion object {
        const val MAX_MILLIS = 120_000L
        const val MIN_MILLIS = 600L
        private const val WAV_MIME = "audio/wav"
        private const val CHANNEL = AudioFormat.CHANNEL_IN_MONO
        private const val ENCODING = AudioFormat.ENCODING_PCM_16BIT
    }
}
