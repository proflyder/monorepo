package dev.proflyder.home.core.presentation

import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentManager
import androidx.lifecycle.LifecycleOwner
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json

abstract class ResultKey<T : Any>(private val serializer: KSerializer<T>) {
    val requestKey: String get() = javaClass.name

    fun pack(value: T): Bundle = Bundle().apply { putString(VALUE, codec.encodeToString(serializer, value)) }

    fun unpack(result: Bundle): T? =
        result.getString(VALUE)?.let { runCatching { codec.decodeFromString(serializer, it) }.getOrNull() }

    private companion object {
        const val VALUE = "value"
        val codec = Json { ignoreUnknownKeys = true }
    }
}

fun <T : Any> FragmentManager.sendResult(key: ResultKey<T>, value: T) {
    setFragmentResult(key.requestKey, key.pack(value))
}

fun <T : Any> FragmentManager.listenResult(key: ResultKey<T>, owner: LifecycleOwner, onValue: (T) -> Unit) {
    setFragmentResultListener(key.requestKey, owner) { _, result -> key.unpack(result)?.let(onValue) }
}

fun <T : Any> Fragment.setResult(key: ResultKey<T>, value: T) {
    parentFragmentManager.sendResult(key, value)
}

fun <T : Any> Fragment.onResult(key: ResultKey<T>, onValue: (T) -> Unit) {
    childFragmentManager.listenResult(key, this, onValue)
}
