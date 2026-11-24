package dev.proflyder.home.core.model

data class SecretEntry(
    val path: String,
    val description: String,
    val consumer: String?,
    val live: Boolean,
    val inCatalog: Boolean,
) {
    val title: String get() = path.substringAfterLast('/')

    val group: String get() = path.substringBefore('/')

    fun matches(query: String): Boolean =
        query.isBlank() ||
            path.contains(query, ignoreCase = true) ||
            description.contains(query, ignoreCase = true)
}

data class SecretCatalog(
    val entries: List<SecretEntry>,
    val truncated: Boolean,
)

data class SecretVersion(
    val version: Long,
    val createdTime: String,
    val deletionTime: String,
    val destroyed: Boolean,
) {
    val available: Boolean get() = !destroyed && deletionTime.isEmpty()
}

data class SecretMetadata(
    val path: String,
    val currentVersion: Long,
    val oldestVersion: Long,
    val versions: List<SecretVersion>,
    val keys: List<String>,
) {
    fun version(number: Long): SecretVersion? = versions.firstOrNull { it.version == number }
}

data class SecretValue(
    val path: String,
    val version: Long,
    val key: String,
    val value: String,
)

data class SecretVersionRef(
    val path: String,
    val version: Long,
    val key: String?,
)

data class SecretRollback(
    val path: String,
    val fromVersion: Long,
    val version: Long,
)

enum class SecretOperation(val wire: String) {
    SESSION("session"),
    UPDATE("update"),
    ROLLBACK("rollback"),
    DELETE("delete"),
}

data class StepUpSession(
    val session: String,
    val expiresInSeconds: Long,
)

data class StepUpChallenge(
    val challenge: String,
    val payload: String,
    val expiresInSeconds: Long,
)

data class DeviceEnrollment(
    val enrolled: Boolean,
    val deviceId: String?,
    val enrolledAt: Long,
    val publicKey: String? = null,
)

data class DeviceAttestationChallenge(
    val challenge: String,
    val expiresInSeconds: Long,
)
