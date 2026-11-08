package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.testing.TestFixtures
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ScenarioValidationTest {
    private fun valid(): ScenarioDraft = ScenarioDraft(
        label = "Evening",
        triggers = listOf(
            ScenarioTrigger.Time(TimeTrigger("Asia/Almaty", WallTime(19, 0), RepeatKind.DAILY)),
        ),
        conditions = listOf(ScenarioCondition.EntityState("light.kitchen", "on")),
        steps = listOf(
            ScenarioStep.Catalog("light.kitchen", "set", TestFixtures.read("brightness_params.json")),
            ScenarioStep.Speak("Good evening"),
            ScenarioStep.Delay(60),
        ),
    )

    private fun failure(draft: ScenarioDraft): RemoteFailure? = ScenarioValidation.validate(draft)

    @Test
    fun validDefinitionPasses() {
        assertNull(failure(valid()))
        assertNull(failure(valid().copy(triggers = emptyList(), conditions = emptyList(), steps = emptyList())))
    }

    @Test
    fun labelUsesServerByteLimit() {
        assertEquals(RemoteFailure.Validation(Field.LABEL), failure(valid().copy(label = " ")))
        assertEquals(RemoteFailure.Validation(Field.LABEL), failure(valid().copy(label = "a".repeat(81))))
        assertNull(failure(valid().copy(label = "я".repeat(40))))
        assertEquals(RemoteFailure.Validation(Field.LABEL), failure(valid().copy(label = "я".repeat(41))))
    }

    @Test
    fun descriptionUsesServerByteLimit() {
        assertEquals(
            RemoteFailure.Validation(Field.DESCRIPTION),
            failure(valid().copy(description = "a".repeat(513))),
        )
        assertNull(failure(valid().copy(description = "a".repeat(512))))
    }

    @Test
    fun collectionLimitsMatchTheServer() {
        val trigger = ScenarioTrigger.Time(TimeTrigger("UTC", WallTime(7, 0), RepeatKind.DAILY))
        assertEquals(
            RemoteFailure.Validation(Field.TRIGGERS),
            failure(valid().copy(triggers = List(9) { trigger })),
        )
        assertNull(failure(valid().copy(triggers = List(8) { trigger })))

        val condition = ScenarioCondition.EntityState("light.kitchen", "on")
        assertEquals(
            RemoteFailure.Validation(Field.CONDITIONS),
            failure(valid().copy(conditions = List(9) { condition })),
        )
        assertNull(failure(valid().copy(conditions = List(8) { condition })))

        val step = ScenarioStep.Delay(5)
        assertEquals(
            RemoteFailure.Validation(Field.STEPS),
            failure(valid().copy(steps = List(33) { step })),
        )
        assertNull(failure(valid().copy(steps = List(32) { step })))
    }

    @Test
    fun speakAndDelayLimitsAreMirrored() {
        assertEquals(
            RemoteFailure.Validation(Field.SPEAK),
            failure(valid().copy(steps = listOf(ScenarioStep.Speak("a".repeat(501))))),
        )
        assertNull(failure(valid().copy(steps = listOf(ScenarioStep.Speak("a".repeat(500))))))
        assertEquals(
            RemoteFailure.Validation(Field.DURATION),
            failure(valid().copy(steps = listOf(ScenarioStep.Delay(0)))),
        )
        assertEquals(
            RemoteFailure.Validation(Field.DURATION),
            failure(valid().copy(steps = listOf(ScenarioStep.Delay(3601)))),
        )
    }

    @Test
    fun unknownOneofPartsBlockSaving() {
        val draft = valid().copy(
            triggers = listOf(ScenarioTrigger.Unknown(TestFixtures.read("unknown_empty.json"))),
            conditions = listOf(ScenarioCondition.Unknown(TestFixtures.read("unknown_empty.json"))),
            steps = listOf(ScenarioStep.Unknown(TestFixtures.read("unknown_empty.json"))),
        )
        assertEquals(RemoteFailure.Validation(Field.UNSUPPORTED), failure(draft))
    }

    @Test
    fun timeTriggerRules() {
        assertEquals(
            RemoteFailure.Validation(Field.TIMEZONE),
            failure(valid().copy(triggers = listOf(
                ScenarioTrigger.Time(TimeTrigger("Not A Zone", WallTime(7, 0), RepeatKind.DAILY)),
            ))),
        )
        assertEquals(
            RemoteFailure.Validation(Field.WEEKDAYS),
            failure(valid().copy(triggers = listOf(
                ScenarioTrigger.Time(TimeTrigger("UTC", WallTime(7, 0), RepeatKind.CUSTOM, weekdays = emptyList())),
            ))),
        )
        assertEquals(
            RemoteFailure.Validation(Field.DATE),
            failure(valid().copy(triggers = listOf(
                ScenarioTrigger.Time(
                    TimeTrigger("UTC", WallTime(7, 0), RepeatKind.ONCE, onceDate = "2000-01-01"),
                ),
            ))),
        )
        assertNull(
            failure(valid().copy(triggers = listOf(
                ScenarioTrigger.Time(
                    TimeTrigger(
                        "UTC",
                        WallTime(7, 0),
                        RepeatKind.ONCE,
                        onceDate = LocalDate.now().plusDays(1).toString(),
                    ),
                ),
            ))),
        )
    }

    @Test
    fun conditionsAndCatalogStepsMatchTheServerShape() {
        assertEquals(
            RemoteFailure.Validation(Field.ENTITY),
            failure(valid().copy(conditions = listOf(ScenarioCondition.EntityState("not an id", "on")))),
        )
        assertEquals(
            RemoteFailure.Validation(Field.ENTITY),
            failure(valid().copy(conditions = listOf(ScenarioCondition.EntityState("light.kitchen", "")))),
        )
        assertEquals(
            RemoteFailure.Validation(Field.ENTITY),
            failure(valid().copy(steps = listOf(ScenarioStep.Catalog("", "run", "{}")))),
        )
        assertEquals(
            RemoteFailure.Validation(Field.STEPS),
            failure(valid().copy(steps = listOf(ScenarioStep.Catalog("light.kitchen", "set", "{not json")))),
        )
        assertNull(
            failure(valid().copy(conditions = listOf(
                ScenarioCondition.TimeWindow(WallTime(8, 0), WallTime(22, 0), emptyList()),
            ))),
        )
    }
}
