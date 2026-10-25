package io.github.naighes.scout4all

import androidx.work.testing.WorkManagerTestInitHelper

/** The app, with a WorkManager that sends nothing: Robolectric creates the
 * app before the provider that would set it up. */
class TestApplication : Scout4AllApplication() {
    override fun onCreate() {
        WorkManagerTestInitHelper.initializeTestWorkManager(this)
        super.onCreate()
    }
}
