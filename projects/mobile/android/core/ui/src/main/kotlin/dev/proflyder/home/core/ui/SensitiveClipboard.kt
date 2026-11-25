package dev.proflyder.home.core.ui

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class SensitiveClipboard(
    private val context: Context,
    private val label: String,
    private val scope: CoroutineScope,
    private val delayMillis: Long = CLEAR_AFTER_MILLIS,
) {
    private val manager: ClipboardManager? = context.getSystemService(ClipboardManager::class.java)
    private var pending: Job? = null

    fun copy(value: String) {
        val clipboard = manager ?: return
        val clip = ClipData.newPlainText(label, value)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            clip.description.extras = PersistableBundle().apply {
                putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
            }
        }
        clipboard.setPrimaryClip(clip)
        pending?.cancel()
        pending = scope.launch {
            delay(delayMillis)
            if (clipboard.primaryClip?.getItemAt(0)?.text?.toString() == value) clear(clipboard)
        }
    }

    private fun clear(clipboard: ClipboardManager) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            clipboard.clearPrimaryClip()
            return
        }
        clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
    }

    private companion object {
        const val CLEAR_AFTER_MILLIS = 30_000L
    }
}
