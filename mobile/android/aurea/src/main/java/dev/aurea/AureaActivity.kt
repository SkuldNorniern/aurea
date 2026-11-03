package dev.aurea

import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlertDialog
import android.content.ActivityNotFoundException
import android.content.Intent
import android.hardware.input.InputManager
import android.net.Uri
import android.os.Build
import android.os.LocaleList
import android.os.Bundle
import android.os.SystemClock
import android.view.Choreographer
import android.view.InputDevice
import android.view.MotionEvent
import android.view.RoundedCorner
import android.view.Surface
import android.view.SurfaceHolder
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.window.OnBackInvokedDispatcher

/**
 * The Activity an Aurea app runs in. Subclass it, name the native library in [library], and
 * define `aurea_main` in Rust, which calls `aurea::android::run`.
 */
abstract class AureaActivity :
    Activity(),
    SurfaceHolder.Callback,
    Choreographer.FrameCallback,
    AureaSurface.Typed {

    /** `my_app` for `libmy_app.so`. */
    protected abstract val library: String

    private var hasSurface = false
    private var frameQueued = false
    private var lightBars: Boolean? = null
    private lateinit var surface: AureaSurface
    private var highFrameRate = false
    private lateinit var files: AureaFiles
    private val keyboards = object : InputManager.InputDeviceListener {
        override fun onInputDeviceAdded(id: Int) = sendKeyboard()
        override fun onInputDeviceRemoved(id: Int) = sendKeyboard()
        override fun onInputDeviceChanged(id: Int) = sendKeyboard()
    }

    private fun sendKeyboard() {
        nativeHardKeyboard(InputDevice.getDeviceIds().any { id ->
            val device = InputDevice.getDevice(id)
            device != null && !device.isVirtual &&
                device.keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC
        })
    }

    /** Gesture insets, corner radii, then each cutout as x, y, width and height. */
    @android.annotation.TargetApi(Build.VERSION_CODES.R)
    private fun screenShape(insets: WindowInsets): FloatArray {
        val gestures = insets.getInsets(WindowInsets.Type.systemGestures())
        val values = arrayListOf(
            gestures.left.toFloat(),
            gestures.top.toFloat(),
            gestures.right.toFloat(),
            gestures.bottom.toFloat(),
        )
        for (position in CORNERS) {
            val radius = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                insets.getRoundedCorner(position)?.radius ?: 0
            } else {
                0
            }
            values.add(radius.toFloat())
        }
        insets.displayCutout?.boundingRects?.forEach { rect ->
            values.add(rect.left.toFloat())
            values.add(rect.top.toFloat())
            values.add(rect.width().toFloat())
            values.add(rect.height().toFloat())
        }
        return values.toFloatArray()
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        System.loadLibrary(library)
        files = AureaFiles(this)
        val view = AureaSurface(this, this)
        surface = view
        setContentView(view)
        nativeCreate(this)
        getSystemService(InputManager::class.java).registerInputDeviceListener(keyboards, null)
        sendKeyboard()
        view.holder.addCallback(this)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) {
                if (!nativeBack()) moveTaskToBack(true)
            }
        }
        view.setOnApplyWindowInsetsListener { _, insets ->
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                nativeInsets(bars.left.toFloat(), bars.top.toFloat(), bars.right.toFloat(), bars.bottom.toFloat())
                nativeScreenShape(screenShape(insets))
                nativeKeyboard(insets.getInsets(WindowInsets.Type.ime()).bottom.toFloat())
                requestFrame()
            }
            insets
        }
        view.setOnTouchListener { _, event ->
            // pen samples arrive as they happen.
            if (event.actionMasked == MotionEvent.ACTION_DOWN && isPen(event)) {
                view.requestUnbufferedDispatch(event)
            }
            pointers(event)
            requestFrame()
            true
        }
        view.setOnGenericMotionListener { _, event ->
            val action = event.actionMasked
            if (action == MotionEvent.ACTION_BUTTON_PRESS && isPen(event) && event.actionButton and PEN_BUTTONS != 0) {
                nativePenTap()
                requestFrame()
                return@setOnGenericMotionListener true
            }
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

    private fun pointers(event: MotionEvent) {
        val index = event.actionIndex
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> pointer(DOWN, event, index, -1)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP ->
                pointer(if (isPalm(event)) CANCEL else UP, event, index, -1)
            MotionEvent.ACTION_MOVE -> {
                for (h in 0 until event.historySize) {
                    for (i in 0 until event.pointerCount) pointer(MOVE, event, i, h)
                }
                for (i in 0 until event.pointerCount) pointer(MOVE, event, i, -1)
            }
            MotionEvent.ACTION_CANCEL -> for (i in 0 until event.pointerCount) pointer(CANCEL, event, i, -1)
        }
    }

    private fun isPen(event: MotionEvent): Boolean {
        val tool = toolOf(event, event.actionIndex)
        return tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER
    }

    /** Some pens come as a finger from a stylus-only source. Touch screens can say both. */
    private fun toolOf(event: MotionEvent, i: Int): Int {
        val tool = event.getToolType(i)
        val finger = tool == MotionEvent.TOOL_TYPE_FINGER || tool == MotionEvent.TOOL_TYPE_UNKNOWN
        val stylus = event.isFromSource(InputDevice.SOURCE_STYLUS) &&
            !event.isFromSource(InputDevice.SOURCE_TOUCHSCREEN)
        return if (finger && stylus) MotionEvent.TOOL_TYPE_STYLUS else tool
    }

    /** Lifted pointer the system took for a palm. */
    private fun isPalm(event: MotionEvent): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            (event.flags and MotionEvent.FLAG_CANCELED) != 0

    /** `h` < 0 is the current reading. */
    private fun pointer(phase: Int, event: MotionEvent, i: Int, h: Int) {
        val now = SystemClock.uptimeMillis()
        val current = h < 0
        val axis = { a: Int -> if (current) event.getAxisValue(a, i) else event.getHistoricalAxisValue(a, i, h) }
        nativePointer(
            phase,
            event.getPointerId(i),
            toolOf(event, i),
            if (current) event.getX(i) else event.getHistoricalX(i, h),
            if (current) event.getY(i) else event.getHistoricalY(i, h),
            if (current) event.getPressure(i) else event.getHistoricalPressure(i, h),
            axis(MotionEvent.AXIS_TILT),
            axis(MotionEvent.AXIS_ORIENTATION),
            (event.buttonState and PEN_BUTTONS) != 0,
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

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        nativeTrimMemory(level)
    }

    override fun onDestroy() {
        getSystemService(InputManager::class.java).unregisterInputDeviceListener(keyboards)
        nativeDestroy(isFinishing)
        super.onDestroy()
    }

    override fun surfaceCreated(holder: SurfaceHolder) {}

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        nativeSurfaceChanged(holder.surface, width, height, resources.displayMetrics.density)
        hasSurface = true
        applyFrameRate()
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

    /** From any thread. */
    @Suppress("unused")
    fun showKeyboardFromNative(show: Boolean) {
        runOnUiThread { surface.showKeyboard(show) }
    }

    /** Hertz. */
    @Suppress("unused", "DEPRECATION")
    fun refreshRateFromNative(): Float =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display?.refreshRate ?: 0f
        else windowManager.defaultDisplay.refreshRate

    /** Top rate while [high], system pacing otherwise. */
    @Suppress("unused")
    fun setHighFrameRateFromNative(high: Boolean) {
        runOnUiThread {
            highFrameRate = high
            applyFrameRate()
        }
    }

    private fun applyFrameRate() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val top = display?.supportedModes?.maxOfOrNull { it.refreshRate } ?: 0f
        val target = surface.holder.surface
        if (target.isValid) {
            target.setFrameRate(if (highFrameRate) top else 0f, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
        }
    }

    @Suppress("unused")
    fun pickFilesFromNative(extensions: Array<String>, multiple: Boolean, id: Long): Boolean =
        files.pick(extensions, multiple, id)

    @Suppress("unused")
    fun exportFilesFromNative(paths: Array<String>, id: Long): Boolean = files.export(paths, id)

    @Suppress("unused")
    fun shareFilesFromNative(paths: Array<String>): Boolean = files.share(paths)

    @Suppress("unused")
    fun clipboardTextFromNative(): String? = files.clipboardText()

    @Suppress("unused")
    fun setClipboardTextFromNative(text: String) = files.setClipboardText(text)

    @Suppress("unused")
    fun clipboardPngFromNative(max: Long): ByteArray? = files.clipboardPng(max)

    @Suppress("unused")
    fun setClipboardPngFromNative(png: ByteArray): Boolean = files.setClipboardPng(png)

    /**
     * Buttons: 0 OK, 1 OK and Cancel, 2 Yes and No, 3 Save, Discard and Cancel. Each answers with
     * the code of `MessageChoice` in Rust. Not dismissable, so one button always answers.
     */
    @Suppress("unused")
    fun alertFromNative(title: String, message: String, buttons: Int, id: Long): Boolean {
        if (isFinishing) return false
        val ok = getString(android.R.string.ok) to 0
        val cancel = getString(android.R.string.cancel) to 1
        // the platform has no shared words for save and discard.
        val (positive, neutral, negative) = when (buttons) {
            1 -> listOf(ok, null, cancel)
            2 -> listOf(getString(android.R.string.yes) to 2, null, getString(android.R.string.no) to 3)
            3 -> listOf("Save" to 4, "Discard" to 5, cancel)
            else -> listOf(ok, null, null)
        }
        val dialog = AlertDialog.Builder(this)
            .setTitle(title)
            .setMessage(message)
            .setCancelable(false)
        positive?.let { (label, code) -> dialog.setPositiveButton(label) { _, _ -> nativeAlertDone(id, code) } }
        neutral?.let { (label, code) -> dialog.setNeutralButton(label) { _, _ -> nativeAlertDone(id, code) } }
        negative?.let { (label, code) -> dialog.setNegativeButton(label) { _, _ -> nativeAlertDone(id, code) } }
        dialog.show()
        return true
    }

    @Deprecated("Android 13 and later use the callback from onCreate")
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        if (!nativeBack()) super.onBackPressed()
    }

    @Deprecated("Activity has no replacement without androidx")
    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (!files.result(requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data)
        }
    }

    /** From any thread, in device pixels. */
    @Suppress("unused")
    fun setTextAreaFromNative(x: Float, y: Float, width: Float, height: Float) {
        runOnUiThread { surface.setTextArea(x, y, width, height) }
    }

    /** The system's languages, most preferred first, as comma-separated BCP 47 tags. */
    @Suppress("unused")
    fun localesFromNative(): String = LocaleList.getDefault().toLanguageTags()

    /** False when no app handles it. */
    @Suppress("unused")
    fun openUrlFromNative(url: String): Boolean = try {
        startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        true
    } catch (_: ActivityNotFoundException) {
        false
    }

    fun requestFrame() {
        if (hasSurface && !frameQueued) {
            frameQueued = true
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    /** From any thread. */
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
        val CORNERS = intArrayOf(
            RoundedCorner.POSITION_TOP_LEFT,
            RoundedCorner.POSITION_TOP_RIGHT,
            RoundedCorner.POSITION_BOTTOM_RIGHT,
            RoundedCorner.POSITION_BOTTOM_LEFT,
        )

        // Rust reads these.
        // pens differ in which button their side switch reports.
        const val PEN_BUTTONS = MotionEvent.BUTTON_STYLUS_PRIMARY or
            MotionEvent.BUTTON_STYLUS_SECONDARY or MotionEvent.BUTTON_SECONDARY

        const val DOWN = 0
        const val UP = 1
        const val MOVE = 2
        const val CANCEL = 3
        const val HOVER = 4
        const val EXIT = 5

        // Rust reads these.
        const val FOCUSED = 0
        const val UNFOCUSED = 1
        const val MINIMIZED = 2
        const val RESTORED = 3

        @JvmStatic external fun nativeCreate(activity: Activity)
        @JvmStatic external fun nativeDestroy(finishing: Boolean)
        @JvmStatic external fun nativeLifecycle(state: Int)
        @JvmStatic external fun nativeTrimMemory(level: Int)
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
        @JvmStatic external fun nativePenTap()
        @JvmStatic external fun nativeInsets(left: Float, top: Float, right: Float, bottom: Float)
        @JvmStatic external fun nativeScreenShape(values: FloatArray)
        @JvmStatic external fun nativeBack(): Boolean
        @JvmStatic external fun nativeHardKeyboard(connected: Boolean)
        @JvmStatic external fun nativeKeyboard(bottom: Float)
        @JvmStatic external fun nativeLightTheme(): Boolean
        @JvmStatic external fun nativeFrame(): Boolean
        @JvmStatic external fun nativeKey(code: Int, pressed: Boolean, meta: Int)
        @JvmStatic external fun nativeText(text: String)
        @JvmStatic external fun nativePreedit(text: String, cursor: Int)
        @JvmStatic external fun nativeAlertDone(id: Long, answer: Int)
    }
}
