package dev.proflyder.home.core.designsystem

import android.content.Context
import android.content.res.TypedArray
import android.util.AttributeSet
import android.view.LayoutInflater
import android.widget.ImageView
import android.widget.TextView
import androidx.annotation.DrawableRes
import androidx.core.content.withStyledAttributes
import com.google.android.material.card.MaterialCardView

class DeviceTile @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : MaterialCardView(context, attrs, R.attr.homeDeviceTileStyle) {
    private val iconView: ImageView
    private val titleView: TextView
    private val statusView: TextView

    init {
        LayoutInflater.from(context).inflate(R.layout.ds_device_tile, this)
        iconView = findViewById(R.id.ds_tile_icon)
        titleView = findViewById(R.id.ds_tile_title)
        statusView = findViewById(R.id.ds_tile_status)
        context.withStyledAttributes(attrs, R.styleable.DeviceTile) { readAttributes(this) }
    }

    var on: Boolean = false
        set(value) {
            field = value
            DeviceStateViews.bindIcon(iconView, value)
            DeviceStateViews.bindStatus(statusView, value)
            DeviceStateViews.bindCard(this, value)
        }

    fun setIcon(@DrawableRes icon: Int) = iconView.setImageResource(icon)

    fun bind(title: CharSequence, status: CharSequence, on: Boolean) {
        titleView.text = title
        statusView.text = status
        this.on = on
    }

    private fun readAttributes(array: TypedArray) {
        iconView.setImageDrawable(array.getDrawable(R.styleable.DeviceTile_tileIcon))
        titleView.text = array.getText(R.styleable.DeviceTile_tileTitle)
        statusView.text = array.getText(R.styleable.DeviceTile_tileStatus)
        on = array.getBoolean(R.styleable.DeviceTile_tileOn, false)
    }
}
