package dev.proflyder.home.feature.chat.presentation

import android.app.Activity
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.widget.ImageView
import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.feature.chat.data.LocalAttachment
import dev.proflyder.home.feature.chat.data.decodeThumbnail
import android.view.LayoutInflater
import android.view.View
import androidx.recyclerview.widget.LinearLayoutManager
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppThemes
import dev.proflyder.home.core.ui.pinAboveKeyboard
import dev.proflyder.home.feature.chat.ChatFixtures
import dev.proflyder.home.feature.chat.data.ChatTimeline
import dev.proflyder.home.feature.chat.data.Outgoing
import dev.proflyder.home.feature.chat.impl.databinding.FragmentChatBinding
import java.io.File
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import androidx.appcompat.view.ContextThemeWrapper as ThemeWrapper
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ChatScreenshotTest {
    private val streaming = ChatFixtures.message("m7", 7, ChatRole.ASSISTANT, "Перезапускаю `scheduler`", status = ChatStatus.STREAMING)
    private val queued = Outgoing("c9", "Спасибо!", ChatQuote("m4", ChatRole.ASSISTANT, "Всё в норме"), ChatFixtures.NOON + 900_000)

    private fun themed(theme: AppTheme): Context {
        val base = ThemeWrapper(RuntimeEnvironment.getApplication(), MaterialR.style.Theme_Material3_DayNight_NoActionBar)
        return AppThemes.wrap(base, theme)
    }

    private class LocalImages : ChatAdapterCallbacks() {
        override fun bindImage(media: BubbleMedia, target: ImageView) {
            media.path?.let { target.setImageBitmap(decodeThumbnail(File(it), PHOTO_WIDTH)) }
        }

        override fun bindWave(media: BubbleMedia, target: VoiceWaveView) {
            target.setLevels(FloatArray(VoiceWaveView.BARS) { 0.2f + 0.8f * kotlin.math.abs(kotlin.math.sin(it * 0.45f)) })
        }
    }

    private fun render(
        name: String,
        theme: AppTheme,
        timeline: ChatTimeline,
        playback: VoicePlayback? = null,
        prepare: (FragmentChatBinding) -> Unit = ::showReply,
    ): FragmentChatBinding {
        val context = themed(theme)
        val binding = FragmentChatBinding.inflate(LayoutInflater.from(context))
        val adapter = ChatAdapter(LocalImages())
        binding.chatList.layoutManager = LinearLayoutManager(context).apply { stackFromEnd = true }
        binding.chatList.adapter = adapter
        binding.root.setBackgroundColor(com.google.android.material.color.MaterialColors.getColor(binding.root, MaterialR.attr.colorSurface))
        adapter.submitList(ChatItems.build(timeline, ZoneOffset.ofHours(5), playback))
        prepare(binding)
        Robolectric.buildActivity(Activity::class.java).setup().get().setContentView(binding.root)
        binding.chatPanel.pinAboveKeyboard(binding.chatList)
        layout(binding.root)
        shadowOf(android.os.Looper.getMainLooper()).idle()
        layout(binding.root)
        binding.chatList.scrollToPosition(adapter.itemCount - 1)
        shadowOf(android.os.Looper.getMainLooper()).idle()
        save(name, binding.root)
        return binding
    }

    private fun photo(): File {
        val bitmap = Bitmap.createBitmap(PHOTO_WIDTH, PHOTO_HEIGHT, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.drawColor(Color.rgb(46, 125, 160))
        canvas.drawCircle(PHOTO_WIDTH * 0.7f, PHOTO_HEIGHT * 0.35f, PHOTO_HEIGHT * 0.2f, Paint().apply { color = Color.rgb(255, 213, 79) })
        val file = File.createTempFile("photo", ".jpg")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, PNG_QUALITY, it) }
        return file
    }

    private fun media(): ChatTimeline {
        val image = photo()
        val outgoing = Outgoing(
            "c13",
            ChatFixtures.conversation[1].text,
            null,
            ChatFixtures.NOON + 13 * 60_000,
            attachments = listOf(LocalAttachment(image.path, ChatAttachmentKind.IMAGE, "image/jpeg", image.name, image.length())),
        )
        val answer = ChatFixtures.message("m11", 11, ChatRole.ASSISTANT, ChatFixtures.conversation[1].text)
        return ChatTimeline(
            messages = listOf(ChatFixtures.voice, answer, ChatFixtures.log),
            outbox = listOf(outgoing),
            loaded = true,
        )
    }

    private fun recording(binding: FragmentChatBinding) {
        binding.chatRecording.visibility = View.VISIBLE
        binding.chatCompose.visibility = View.GONE
        binding.chatRecordingTime.text = android.text.format.DateUtils.formatElapsedTime(RECORDED_SECONDS)
    }

    private fun alerts(): ChatTimeline = ChatTimeline(
        messages = listOf(
            ChatFixtures.alert("m20", 20, "critical", resolved = false),
            ChatFixtures.alert("m21", 21, "warning", resolved = false),
            ChatFixtures.alert("m22", 22, "critical", resolved = true),
        ),
        loaded = true,
    )

    @Test
    fun alertsInCurrent() {
        val binding = render("chat-alerts-current", AppTheme.CURRENT, alerts(), prepare = {})
        assertEquals(4, binding.chatList.adapter?.itemCount)
    }

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun alertsInNeonDark() {
        render("chat-alerts-neon-dark", AppTheme.NEON, alerts(), prepare = {})
    }

    @Test
    fun mediaInCurrent() {
        val playback = VoicePlayback("a-f_v", 2_600, 7_400, playing = true)
        val binding = render("chat-media-current", AppTheme.CURRENT, media(), playback, prepare = {})
        assertEquals(5, binding.chatList.adapter?.itemCount)
    }

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun mediaInNeonDark() {
        render("chat-media-neon-dark", AppTheme.NEON, media(), prepare = ::recording)
    }

    private fun showReply(binding: FragmentChatBinding) {
        binding.chatReplyBar.visibility = View.VISIBLE
        binding.chatReplyAuthor.setText(chatAuthor(ChatRole.ASSISTANT))
        binding.chatReplyText.text = ChatFixtures.conversation[3].text.lines().first()
        binding.chatInput.setText(ChatFixtures.conversation[4].text)
        binding.chatSend.isEnabled = true
    }

    private fun layout(view: View) {
        val width = (WIDTH_DP * view.resources.displayMetrics.density).toInt()
        val height = (HEIGHT_DP * view.resources.displayMetrics.density).toInt()
        view.measure(exactly(width), exactly(height))
        view.layout(0, 0, width, height)
    }

    private fun save(name: String, view: View) {
        layout(view)
        val width = view.width
        val height = view.height
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private fun conversation(): ChatTimeline =
        ChatTimeline(messages = ChatFixtures.conversation + streaming, outbox = listOf(queued), loaded = true)

    @Test
    fun conversationInCurrent() {
        val binding = render("chat-current", AppTheme.CURRENT, conversation())
        assertEquals(9, binding.chatList.adapter?.itemCount)
    }

    @Test
    fun conversationInNeon() {
        val binding = render("chat-neon", AppTheme.NEON, conversation())
        assertEquals(9, binding.chatList.adapter?.itemCount)
    }

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun conversationInCurrentDark() {
        render("chat-current-dark", AppTheme.CURRENT, conversation())
    }

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun conversationInNeonDark() {
        render("chat-neon-dark", AppTheme.NEON, conversation())
    }

    @Test
    @Config(qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
    fun typingInNeonDark() {
        val pending = ChatFixtures.message("m8", 8, ChatRole.ASSISTANT, "", status = ChatStatus.PENDING)
        val binding = render("chat-neon-typing", AppTheme.NEON, ChatTimeline(messages = ChatFixtures.conversation.take(1) + pending))
        assertEquals(ChatItem.Typing, (binding.chatList.adapter as ChatAdapter).currentList.last())
    }

    private companion object {
        const val WIDTH_DP = 411
        const val HEIGHT_DP = 891
        const val PNG_QUALITY = 100
        const val PHOTO_WIDTH = 640
        const val PHOTO_HEIGHT = 480
        const val RECORDED_SECONDS = 4L
    }
}
