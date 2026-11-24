package dev.proflyder.home.feature.secrets

import dev.proflyder.home.feature.secrets.presentation.StagingAliasState
import dev.proflyder.home.feature.secrets.presentation.canReuseStagedKey
import dev.proflyder.home.feature.secrets.presentation.samePublicKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class StagingAliasStateTest {
    @Test
    fun aStagedAliasSurvivesARecreatedScreen() {
        val context = RuntimeEnvironment.getApplication()
        StagingAliasState(context).stage("alias-1", "challenge-1")

        val recreated = StagingAliasState(context)

        assertEquals("alias-1", recreated.staged())
        assertEquals("challenge-1", recreated.stagedChallenge())
        assertTrue(canReuseStagedKey(recreated.staged(), recreated.stagedChallenge(), "challenge-1"))
        assertFalse(canReuseStagedKey(recreated.staged(), recreated.stagedChallenge(), "challenge-2"))
    }

    @Test
    fun clearingTheStagingStateLeavesNothingBehind() {
        val context = RuntimeEnvironment.getApplication()
        val state = StagingAliasState(context)
        state.stage("alias-1", "challenge-1")
        state.clear()

        assertNull(StagingAliasState(context).staged())
        assertNull(StagingAliasState(context).stagedChallenge())
    }

    @Test
    fun theServerKeyMustMatchTheStagedKeyToPromote() {
        assertTrue(samePublicKey("public-key", "public-key"))
        assertFalse(samePublicKey("public-key", "other"))
        assertFalse(samePublicKey(null, "public-key"))
        assertFalse(samePublicKey("public-key", null))
    }
}
