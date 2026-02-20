package com.aurea.smoke

import android.os.Bundle
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.appcompat.app.AppCompatActivity

/**
 * Minimal Android smoke app for Aurea.
 *
 * To integrate Aurea native + Rust:
 * 1. Add [lib] crate-type = ["cdylib"] to aurea for Android target
 * 2. Run: cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build
 * 3. Load libaurea.so in static block and call init
 *
 * This placeholder shows a blank surface; the Rust/native bridge
 * would receive the Surface via SurfaceHolder.Callback.surfaceCreated.
 */
class MainActivity : AppCompatActivity(), SurfaceHolder.Callback {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        findViewById<SurfaceView>(R.id.surface).holder.addCallback(this)
    }

    override fun surfaceCreated(holder: SurfaceHolder) {}

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {}

    override fun surfaceDestroyed(holder: SurfaceHolder) {}
}
