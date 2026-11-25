package dev.proflyder.home.feature.chat

import dev.proflyder.home.feature.chat.data.ChatRepositoryTest
import dev.proflyder.home.feature.chat.data.ChatTimelineTest
import dev.proflyder.home.feature.chat.data.OutboxCodecTest
import dev.proflyder.home.feature.chat.presentation.ChatItemsTest
import dev.proflyder.home.feature.chat.presentation.ChatMarkdownTest
import dev.proflyder.home.feature.chat.presentation.ChatScreenshotTest
import dev.proflyder.home.feature.chat.presentation.ChatViewModelsTest
import dev.proflyder.home.feature.chat.presentation.ChatSessionsScreenshotTest
import dev.proflyder.home.feature.chat.presentation.ChatScrollTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ChatTimelineTest::class,
    OutboxCodecTest::class,
    ChatRepositoryTest::class,
    ChatItemsTest::class,
    ChatMarkdownTest::class,
    ChatScreenshotTest::class,
    ChatViewModelsTest::class,
    ChatSessionsScreenshotTest::class,
    ChatScrollTest::class,
)
class AllChatTests
