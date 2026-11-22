package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.test.TestConfig
import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class NoDirectStateWriteTest {
    @Test
    fun `reports an assignment to mutableState value`() {
        val code = """
            class Screen : BaseViewModel<State>(State()) {
                fun load() {
                    mutableState.value = State()
                }
            }
        """.trimIndent()

        assertEquals(1, NoDirectStateWrite().lint(code).size)
    }

    @Test
    fun `reports a qualified assignment and an update call`() {
        val code = """
            class Screen : BaseViewModel<State>(State()) {
                fun load() {
                    this.mutableState.value = State()
                    mutableState.update { it.copy(loading = true) }
                }
            }
        """.trimIndent()

        assertEquals(2, NoDirectStateWrite().lint(code).size)
    }

    @Test
    fun `accepts updateState and reading state`() {
        val code = """
            class Screen : BaseViewModel<State>(State()) {
                fun load() {
                    updateState { copy(loading = mutableState.value.loading) }
                }
            }
        """.trimIndent()

        assertEquals(0, NoDirectStateWrite().lint(code).size)
    }

    @Test
    fun `accepts a mutableState outside a ViewModel`() {
        val code = """
            class Source {
                fun load() {
                    mutableState.value = State()
                }
            }
        """.trimIndent()

        assertEquals(0, NoDirectStateWrite().lint(code).size)
    }

    @Test
    fun `accepts the write inside the reducer function itself`() {
        val code = """
            abstract class Base : BaseViewModel<State>(State()) {
                protected fun updateState(reducer: State.() -> State) {
                    mutableState.update { it.reducer() }
                }
            }
        """.trimIndent()

        assertEquals(0, NoDirectStateWrite().lint(code).size)
    }

    @Test
    fun `accepts other flows and honours configured names`() {
        val code = """
            class Screen : BaseViewModel<State>(State()) {
                fun load() {
                    mutableRoute.value = Route.Home
                    events.tryEmit(Event.Saved)
                }
            }
        """.trimIndent()

        assertEquals(0, NoDirectStateWrite().lint(code).size)
        val configured = NoDirectStateWrite(TestConfig("stateNames" to listOf("mutableRoute")))
        assertEquals(1, configured.lint(code).size)
    }
}
