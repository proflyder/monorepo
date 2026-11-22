package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.testing.TestFixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ControlTaskPayloadsTest {
    @Test
    fun buildsAdjustPayload() {
        assertEquals(
            TestFixtures.read("control_set_payload.json"),
            ControlTaskPayloads.adjustPayload("light.strip", LightAdjust(brightnessPct = 55, hue = 120, saturation = 80)),
        )
    }

    @Test
    fun buildsVolumeAndMutePayloads() {
        assertEquals(
            TestFixtures.read("control_volume_payload.json"),
            ControlTaskPayloads.volumePayload("volume.station", 45),
        )
        assertEquals(
            TestFixtures.read("control_mute_payload.json"),
            ControlTaskPayloads.mutePayload("volume.station", muted = true),
        )
    }

    @Test
    fun buildsSpeakPayloadWithTheTextOnlyInTheTask() {
        val payload = ControlTaskPayloads.speakPayload("action.station_speak", "Добрый вечер")
        assertEquals(TestFixtures.read("control_speak_payload.json"), payload)
        assertTrue(payload.contains("Добрый вечер"))
    }
}
