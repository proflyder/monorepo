package dev.proflyder.home.core.presentation

import android.content.Context
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.ScreenSectionKey
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Multibinds
import dev.zacsweers.metro.SingleIn

interface ScreenSection {
    val key: ScreenSectionKey

    fun create(): Fragment
}

interface ScreenSectionsProvider {
    val screenSections: ScreenSections
}

@SingleIn(AppScope::class)
@Inject
class ScreenSections(entries: Set<ScreenSection>) {
    private val byId = entries.associateBy { it.key.id }

    fun create(key: ScreenSectionKey): Fragment? = byId[key.id]?.create()
}

@ContributesTo(AppScope::class)
@BindingContainer
interface ScreenSectionMultibindings {
    @Multibinds(allowEmpty = true)
    fun screenSections(): Set<ScreenSection>
}

fun Context.screenSections(): ScreenSections {
    val host = applicationContext
    check(host is ScreenSectionsProvider) { "Application must implement ScreenSectionsProvider" }
    return host.screenSections
}
