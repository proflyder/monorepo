package dev.proflyder.home.core.navigation

import androidx.appcompat.app.AppCompatActivity

interface AppActivityContribution {
    val order: Int

    fun attach(activity: AppCompatActivity)
}
