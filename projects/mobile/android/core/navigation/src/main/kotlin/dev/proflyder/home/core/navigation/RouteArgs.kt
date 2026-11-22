package dev.proflyder.home.core.navigation

import android.os.Bundle
import android.os.Parcel
import android.os.Parcelable
import androidx.core.os.BundleCompat
import androidx.fragment.app.Fragment
import androidx.lifecycle.SavedStateHandle

class RouteArgs(val route: String) : Parcelable {
    override fun describeContents(): Int = 0

    override fun writeToParcel(dest: Parcel, flags: Int) {
        dest.writeString(route)
    }

    fun <D : Destination> decode(codec: RouteCodec<D>): D? = RoutePath.of(route)?.let(codec::decode)

    companion object {
        const val KEY: String = "dev.proflyder.home.navigation.route"

        @JvmField
        val CREATOR: Parcelable.Creator<RouteArgs> = object : Parcelable.Creator<RouteArgs> {
            override fun createFromParcel(source: Parcel): RouteArgs = RouteArgs(source.readString().orEmpty())

            override fun newArray(size: Int): Array<RouteArgs?> = arrayOfNulls(size)
        }

        fun bundle(destination: Destination): Bundle = Bundle().apply { putParcelable(KEY, RouteArgs(destination.route)) }
    }
}

fun <D : Destination> Fragment.destination(codec: RouteCodec<D>): Lazy<D?> = lazy {
    arguments?.let { BundleCompat.getParcelable(it, RouteArgs.KEY, RouteArgs::class.java) }?.decode(codec)
}

fun <D : Destination> SavedStateHandle.destination(codec: RouteCodec<D>): D? =
    get<RouteArgs>(RouteArgs.KEY)?.decode(codec)
