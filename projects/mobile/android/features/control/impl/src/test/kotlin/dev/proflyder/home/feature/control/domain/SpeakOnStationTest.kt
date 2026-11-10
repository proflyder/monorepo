package dev.proflyder.home.feature.control.domain

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.control.ControlRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SpeakOnStationTest {
    private class FakeRepository : ControlRepository {
        val speaks = mutableListOf<Pair<String, String>>()
        var result: Outcome<Unit> = Outcome.Success(Unit)

        override val isConfigured: Boolean = true

        override suspend fun catalog(): Outcome<ControlCatalog> =
            Outcome.Success(ControlCatalog(emptyList(), statesOk = true))

        override suspend fun send(entryId: String, action: ControlAction): Outcome<Unit> = result

        override suspend fun adjust(entryId: String, adjust: LightAdjust): Outcome<Unit> = result

        override suspend fun speak(entryId: String, text: String): Outcome<Unit> {
            speaks += entryId to text
            return result
        }

        override suspend fun setVolume(entryId: String, level: Int): Outcome<Unit> = result

        override suspend fun setVolumeMuted(entryId: String, muted: Boolean): Outcome<Unit> = result
    }

    private val speak = ControlEntry(
        id = "action.station_speak",
        kind = ControlKind.ACTION,
        title = "Сказать на колонке",
        room = "",
        icon = "",
        actions = setOf(ControlAction.SPEAK),
        power = null,
        available = true,
    )

    @Test
    fun forwardsTheEntryIdAndTextToTheRepository() = runBlocking {
        val repository = FakeRepository()
        val outcome = SpeakOnStation(repository)(speak, "Добрый вечер")
        assertEquals(Outcome.Success(Unit), outcome)
        assertEquals(listOf("action.station_speak" to "Добрый вечер"), repository.speaks)
    }

    @Test
    fun keepsTheFailureOfTheQueueCall() = runBlocking {
        val repository = FakeRepository().apply {
            result = Outcome.Failure(dev.proflyder.home.core.model.RemoteFailure.Unauthorized)
        }
        val outcome = SpeakOnStation(repository)(speak, "текст")
        assertTrue(outcome is Outcome.Failure)
    }
}
