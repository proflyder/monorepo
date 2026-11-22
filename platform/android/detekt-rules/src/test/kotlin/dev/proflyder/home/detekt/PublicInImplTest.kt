package dev.proflyder.home.detekt

import io.github.detekt.test.utils.compileContentForTest
import io.gitlab.arturbosch.detekt.test.lint
import org.junit.Assert.assertEquals
import org.junit.Test

class PublicInImplTest {
    @Test
    fun `reports public top-level declarations in an impl module`() {
        val file = compileContentForTest(
            """
                class Repository
                fun helper() = Unit
                val constant = 1
                typealias Id = String
            """.trimIndent(),
            "features/alarms/impl/src/main/kotlin/Repository.kt",
        )

        assertEquals(4, PublicInImpl().lint(file).size)
    }

    @Test
    fun `accepts internal and private declarations in an impl module`() {
        val file = compileContentForTest(
            """
                internal class Repository
                private fun helper() = Unit
                internal val constant = 1
            """.trimIndent(),
            "features/alarms/impl/src/main/kotlin/Repository.kt",
        )

        assertEquals(0, PublicInImpl().lint(file).size)
    }

    @Test
    fun `accepts public declarations in api and in impl tests`() {
        val api = compileContentForTest(
            "class Alarm",
            "features/alarms/api/src/main/kotlin/Alarm.kt",
        )
        val test = compileContentForTest(
            "class RepositoryTest",
            "features/alarms/impl/src/test/kotlin/RepositoryTest.kt",
        )

        assertEquals(0, PublicInImpl().lint(api).size)
        assertEquals(0, PublicInImpl().lint(test).size)
    }

    @Test
    fun `accepts public declarations that Metro must see from the graph module`() {
        val file = compileContentForTest(
            """
                @ContributesBinding(AppScope::class)
                @Inject
                class HttpClient
                @ContributesTo(AppScope::class)
                @BindingContainer
                abstract class Bindings
                class Helper
            """.trimIndent(),
            "core/network/impl/src/main/kotlin/HttpClient.kt",
        )

        assertEquals(1, PublicInImpl().lint(file).size)
    }

    @Test
    fun `accepts public graph contributions that the app graph must see`() {
        val file = compileContentForTest(
            """
                @ContributesIntoMap(AppScope::class)
                class ScreenViewModel
                @ContributesTo(AppScope::class)
                interface FeatureInjector
                class Helper
                class InjectedService {
                    @Inject
                    lateinit var controller: Controller
                }
            """.trimIndent(),
            "features/alarms/impl/src/main/kotlin/Screen.kt",
        )

        assertEquals(1, PublicInImpl().lint(file).size)
    }
}
