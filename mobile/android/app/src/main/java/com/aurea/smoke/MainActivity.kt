package com.aurea.smoke

import android.annotation.SuppressLint
import android.app.Activity
import android.os.Build
import android.os.Bundle
import android.view.Choreographer
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.WindowInsets
import android.view.WindowInsetsController

class MainActivity : Activity(), SurfaceHolder.Callback, Choreographer.FrameCallback {

    companion object {
        init {
            System.loadLibrary("android_demo")
        }

        // Static, so JNI finds them as Java_com_aurea_smoke_MainActivity_*.
        @JvmStatic external fun nativeInit(activity: Activity)
        @JvmStatic external fun nativeCreateApp()
        @JvmStatic external fun nativeOnPause()
        @JvmStatic external fun nativeOnResume()
        @JvmStatic external fun nativeOnDestroy()
        @JvmStatic external fun nativeOnSurfaceLost()
        @JvmStatic external fun nativeOnSurfaceRecreated()
        @JvmStatic external fun nativeSurfaceChanged(surface: Surface, width: Int, height: Int, density: Float)
        @JvmStatic external fun nativeSurfaceDestroyed()
        @JvmStatic external fun nativeTouch(action: Int, x: Float, y: Float)
        @JvmStatic external fun nativeInsets(left: Float, top: Float, right: Float, bottom: Float)
        @JvmStatic external fun nativeLightTheme(): Boolean
        @JvmStatic external fun nativeFrame(): Boolean
    }

    private var surfaceWasLost = false
    private var hasSurface = false
    private var frameQueued = false
    private var lightBars: Boolean? = null

    @SuppressLint("ClickableViewAccessibility")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        nativeInit(this)
        nativeCreateApp()
        val view = findViewById<SurfaceView>(R.id.surface)
        view.holder.addCallback(this)
        view.setOnApplyWindowInsetsListener { _, insets ->
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                nativeInsets(bars.left.toFloat(), bars.top.toFloat(), bars.right.toFloat(), bars.bottom.toFloat())
                requestFrame()
            }
            insets
        }
        view.setOnTouchListener { _, event ->
            val action = event.actionMasked
            if (action <= MotionEvent.ACTION_CANCEL) {
                nativeTouch(action, event.x, event.y)
                requestFrame()
            }
            true
        }
    }

    override fun onPause() {
        super.onPause()
        nativeOnPause()
    }

    override fun onResume() {
        super.onResume()
        nativeOnResume()
        requestFrame()
    }

    override fun onDestroy() {
        nativeOnDestroy()
        super.onDestroy()
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        if (surfaceWasLost) {
            nativeOnSurfaceRecreated()
            surfaceWasLost = false
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        nativeSurfaceChanged(holder.surface, width, height, resources.displayMetrics.density)
        hasSurface = true
        requestFrame()
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        hasSurface = false
        nativeSurfaceDestroyed()
        nativeOnSurfaceLost()
        surfaceWasLost = true
    }

    private fun requestFrame() {
        if (hasSurface && !frameQueued) {
            frameQueued = true
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    override fun doFrame(frameTimeNanos: Long) {
        frameQueued = false
        val more = hasSurface && nativeFrame()
        updateBars()
        if (more) {
            requestFrame()
        }
    }

    /** Dark status bar icons on a light theme. */
    private fun updateBars() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val light = nativeLightTheme()
        if (light == lightBars) return
        lightBars = light
        val mask = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
            WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
        window.insetsController?.setSystemBarsAppearance(if (light) mask else 0, mask)
    }
}
