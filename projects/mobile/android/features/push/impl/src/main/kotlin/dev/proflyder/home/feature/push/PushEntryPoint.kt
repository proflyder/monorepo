package dev.proflyder.home.feature.push

/**
 * Process-wide access to the push collaborators for components the framework
 * instantiates itself (`FirebaseMessagingService`, the foreground service, the
 * ringing activity). They cannot be constructor-injected by Metro, so the
 * application installs this entry point in `onCreate`, exactly like the call
 * feature does.
 */
object PushEntryPoint {
    private var dependencies: Dependencies? = null

    /** Everything the framework-instantiated components need. */
    class Dependencies(
        val registrar: FirebasePushRegistrar,
        val notifications: PushNotifier,
        val findPhone: FindPhoneController,
    )

    fun install(value: Dependencies) {
        dependencies = value
    }

    fun require(): Dependencies =
        checkNotNull(dependencies) { "PushEntryPoint was not installed by the application" }
}
