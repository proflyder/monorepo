package dev.proflyder.home.feature.push

object PushEntryPoint {
    private var dependencies: Dependencies? = null

    class Dependencies(
        val registrar: FirebasePushRegistrar,
        val notifications: PushNotifier,
        val findPhone: FindPhoneController,
        val history: NotificationHistoryRecorder,
    )

    fun install(value: Dependencies) {
        dependencies = value
    }

    fun require(): Dependencies =
        checkNotNull(dependencies) { "PushEntryPoint was not installed by the application" }
}
