package dev.proflyder.home.feature.scenarios.presentation.editor

import dev.proflyder.home.core.navigation.destination
import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.feature.scenarios.ScenarioEditorDestination
import dev.proflyder.home.feature.scenarios.domain.PendingCreate
import dev.proflyder.home.feature.scenarios.domain.PendingCreateStore
import kotlin.properties.ReadWriteProperty
import kotlin.reflect.KProperty

internal class SavedEditorState(private val handle: SavedStateHandle) : PendingCreateStore {
    val scenarioId: String? get() = handle.destination(ScenarioEditorDestination)?.scenarioId

    private var pendingKey: String? by handle.saved(KEY_CREATE_KEY)
    private var pendingEndpoint: String? by handle.saved(KEY_CREATE_ENDPOINT)

    var draft: ScenarioEditorDraft?
        get() = ScenarioEditorDraftCodec.fromMap(handle.keys().associateWith { handle.get<Any?>(it) })
        set(value) {
            value?.let(ScenarioEditorDraftCodec::toMap)?.forEach { (key, encoded) -> handle[key] = encoded }
        }

    override var pending: PendingCreate?
        get() = pendingKey?.let { key -> pendingEndpoint?.let { PendingCreate(key, it) } }
        set(value) {
            pendingKey = value?.key
            pendingEndpoint = value?.endpoint
        }

    private companion object {
        const val KEY_CREATE_KEY = "scenario_create_key"
        const val KEY_CREATE_ENDPOINT = "scenario_create_endpoint"
    }
}

internal fun <T : Any> SavedStateHandle.saved(key: String): ReadWriteProperty<Any?, T?> = SavedHandleProperty(this, key)

private class SavedHandleProperty<T : Any>(
    private val handle: SavedStateHandle,
    private val key: String,
) : ReadWriteProperty<Any?, T?> {
    override fun getValue(thisRef: Any?, property: KProperty<*>): T? = handle[key]

    override fun setValue(thisRef: Any?, property: KProperty<*>, value: T?) {
        if (value == null) handle.remove<T>(key) else handle[key] = value
    }
}
