package dev.proflyder.home.feature.update

import kotlinx.coroutines.flow.StateFlow

data class AvailableUpdate(
    val versionName: String,
    val versionCode: Long,
    val releaseNotes: String,
)

enum class UpdatePhase {
    IDLE,
    CHECKING,
    UP_TO_DATE,
    AVAILABLE,
    SIGN_IN_REQUIRED,
    SIGNING_IN,
    DOWNLOADING,
    INSTALLING,
    FAILED,
}

enum class UpdateFailureKind {
    SIGN_IN_REQUIRED,
    SIGN_IN_CANCELLED,
    NOT_AVAILABLE,
    OFFLINE,
    DOWNLOAD,
    INSTALL,
    INSTALL_CANCELLED,
    UNSUPPORTED,
    UNKNOWN,
}

data class UpdateState(
    val currentVersion: String = "",
    val phase: UpdatePhase = UpdatePhase.IDLE,
    val available: AvailableUpdate? = null,
    val progressPercent: Int? = null,
    val failure: UpdateFailureKind? = null,
    val promptVisible: Boolean = false,
)

interface UpdateCenter {
    val supported: Boolean

    val state: StateFlow<UpdateState>

    fun checkNow()

    fun signIn()

    fun install()

    fun dismissPrompt()
}
