package dev.proflyder.home.core.designsystem

import android.content.Context

fun Context.joinMeta(parts: Iterable<CharSequence>): String = parts.joinToString(getString(R.string.ds_meta_separator))
