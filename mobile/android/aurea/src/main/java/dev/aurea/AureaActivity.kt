package dev.aurea

import android.annotation.SuppressLint
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ComponentCallbacks2
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.Choreographer
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.WindowInsets
import android.view.WindowInsetsController

/**
 * The Activity an Aurea app runs in: a full-screen surface Aurea draws, with touches, the pen, the
 * system bars and the app's lifecycle passed to it. Subclass it, name the app's native library in
 * [library], and define `aurea_main` in Rust, which calls `aurea::android::run`.
 */
abstract class AureaActivity :
    Activity(),
    SurfaceHolder.Callback,
    Choreographer.FrameCallback,
    AureaSurface.Typed {

    /** The app's native library, as `System.loadLibrary` takes it: `my_app` for `libmy_app.so`. */
    protected abstract val library: String

    private var hasSurface = false
    private var frameQueued = false
    private var lightBars: Boolean? = null
    private lateinit var surface: AureaSurface

    @SuppressLint("ClickableViewAccessibility")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        System.loadLibrary(library)
        val view = AureaSurface(this, this)
        surface = view
        setContentView(view)
        nativeCreate(this)
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
            pointers(event)
            requestFrame()
            true
        }
        view.setOnGenericMotionListener { _, event ->
            val action = event.actionMasked
            val hover = action == MotionEvent.ACTION_HOVER_ENTER ||
                action == MotionEvent.ACTION_HOVER_MOVE ||
                action == MotionEvent.ACTION_HOVER_EXIT
            if (hover) {
                val phase = if (action == MotionEvent.ACTION_HOVER_EXIT) EXIT else HOVER
                for (i in 0 until event.pointerCount) pointer(phase, event, i, -1)
                requestFrame()
            }
            hover
        }
    }

    /** Every pointer change in `event`, with the samples Android batched since the last one. */
    private fun pointers(event: MotionEvent) {
        val index = event.actionIndex
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> pointer(DOWN, event, index, -1)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> pointer(UP, event, index, -1)
            MotionEvent.ACTION_MOVE -> {
                for (h in 0 until event.historySize) {
                    for (i in 0 until event.pointerCount) pointer(MOVE, event, i, h)
                }
                for (i in 0 until event.pointerCount) pointer(MOVE, event, i, -1)
            }
            MotionEvent.ACTION_CANCEL -> for (i in 0 until event.pointerCount) pointer(CANCEL, event, i, -1)
        }
    }

    /** Pointer `i` of `event`: its current reading, or historical sample `h`. */
    private fun pointer(phase: Int, event: MotionEvent, i: Int, h: Int) {
        val now = SystemClock.uptimeMillis()
        val current = h < 0
        val axis = { a: Int -> if (current) event.getAxisValue(a, i) else event.getHistoricalAxisValue(a, i, h) }
        nativePointer(
            phase,
            event.getPointerId(i),
            event.getToolType(i),
            if (current) event.getX(i) else event.getHistoricalX(i, h),
            if (current) event.getY(i) else event.getHistoricalY(i, h),
            if (current) event.getPressure(i) else event.getHistoricalPressure(i, h),
            axis(MotionEvent.AXIS_TILT),
            axis(MotionEvent.AXIS_ORIENTATION),
            (event.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY) != 0,
            (now - if (current) event.eventTime else event.getHistoricalEventTime(h)) / 1000.0,
        )
    }

    override fun onStart() {
        super.onStart()
        nativeLifecycle(RESTORED)
    }

    override fun onResume() {
        super.onResume()
        nativeLifecycle(FOCUSED)
        requestFrame()
    }

    override fun onPause() {
        nativeLifecycle(UNFOCUSED)
        super.onPause()
    }

    override fun onStop() {
        nativeLifecycle(MINIMIZED)
        super.onStop()
    }

    @Suppress("DEPRECATION")
    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) nativeLifecycle(MEMORY_WARNING)
    }

    override fun onDestroy() {
        nativeDestroy(isFinishing)
        super.onDestroy()
    }

    override fun surfaceCreated(holder: SurfaceHolder) {}

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        nativeSurfaceChanged(holder.surface, width, height, resources.displayMetrics.density)
        hasSurface = true
        requestFrame()
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        hasSurface = false
        nativeSurfaceDestroyed()
    }

    override fun key(code: Int, pressed: Boolean, meta: Int) {
        nativeKey(code, pressed, meta)
        requestFrame()
    }

    override fun text(text: String) {
        nativeText(text)
        requestFrame()
    }

    override fun preedit(text: String, cursor: Int) {
        nativePreedit(text, cursor)
        requestFrame()
    }

    /** Shows or hides the on-screen keyboard from any thread; Rust calls it. */
    @Suppress("unused")
    fun showKeyboardFromNative(show: Boolean) {
        runOnUiThread { surface.showKeyboard(show) }
    }

    /** Opens a URL Rust has checked in the app that handles it; false when none does. */
    @Suppress("unused")
    fun openUrlFromNative(url: String): Boolean = try {
        startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        true
    } catch (_: ActivityNotFoundException) {
        false
    }

    /** Draws a frame on the next vsync, if one is not already coming. */
    fun requestFrame() {
        if (hasSurface && !frameQueued) {
            frameQueued = true
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    /** [requestFrame] from any thread; Rust calls it. */
    @Suppress("unused")
    fun requestFrameFromNative() {
        runOnUiThread { requestFrame() }
    }

    override fun doFrame(frameTimeNanos: Long) {
        frameQueued = false
        val more = hasSurface && nativeFrame()
        updateBars()
        if (more) requestFrame()
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

    private companion object {
        // Pointer phases, as Rust reads them.
        const val DOWN = 0
        const val UP = 1
        const val MOVE = 2
        const val CANCEL = 3
        const val HOVER = 4
        const val EXIT = 5

        // Lifecycle states, as Rust reads them.
        const val FOCUSED = 0
        const val UNFOCUSED = 1
        const val MINIMIZED = 2
        const val RESTORED = 3
        const val MEMORY_WARNING = 4

        @JvmStatic external fun nativeCreate(activity: Activity)
        @JvmStatic external fun nativeDestroy(finishing: Boolean)
        @JvmStatic external fun nativeLifecycle(state: Int)
        @JvmStatic external fun nativeSurfaceChanged(surface: Surface, width: Int, height: Int, density: Float)
        @JvmStatic external fun nativeSurfaceDestroyed()
        @JvmStatic external fun nativePointer(
            phase: Int,
            id: Int,
            tool: Int,
            x: Float,
            y: Float,
            pressure: Float,
            tilt: Float,
            orientation: Float,
            barrel: Boolean,
            age: Double,
        )
        @JvmStatic external fun nativeInsets(left: Float, top: Float, right: Float, bottom: Float)
        @JvmStatic external fun nativeLightTheme(): Boolean
        @JvmStatic external fun nativeFrame(): Boolean
        @JvmStatic external fun nativeKey(code: Int, pressed: Boolean, meta: Int)
        @JvmStatic external fun nativeText(text: String)
        @JvmStatic external fun nativePreedit(text: String, cursor: Int)
    }
}
