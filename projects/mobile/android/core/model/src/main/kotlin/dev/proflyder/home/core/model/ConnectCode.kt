package dev.proflyder.home.core.model

enum class ConnectCode(val wire: String) {
    CANCELED("canceled"),
    UNKNOWN("unknown"),
    INVALID_ARGUMENT("invalid_argument"),
    DEADLINE_EXCEEDED("deadline_exceeded"),
    NOT_FOUND("not_found"),
    ALREADY_EXISTS("already_exists"),
    PERMISSION_DENIED("permission_denied"),
    RESOURCE_EXHAUSTED("resource_exhausted"),
    FAILED_PRECONDITION("failed_precondition"),
    ABORTED("aborted"),
    OUT_OF_RANGE("out_of_range"),
    UNIMPLEMENTED("unimplemented"),
    INTERNAL("internal"),
    UNAVAILABLE("unavailable"),
    DATA_LOSS("data_loss"),
    UNAUTHENTICATED("unauthenticated"),
    ;

    companion object {
        fun fromWire(value: String): ConnectCode = entries.firstOrNull { it.wire == value } ?: UNKNOWN
    }
}
