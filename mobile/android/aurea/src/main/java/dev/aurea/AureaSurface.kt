package dev.aurea

import android.content.Context
import android.text.InputType
import android.view.KeyEvent
import android.view.SurfaceView
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/**
 * The surface Aurea draws on, which is also the text editor the on-screen keyboard types into:
 * typed text, what an input method is still composing, and key presses go to Rust.
 */
internal class AureaSurface(context: Context, private val typed: Typed) : SurfaceView(context) {

    private companion object {
        const val MAX_DELETE = 256
    }

    /** Where typing goes. */
    interface Typed {
        fun key(code: Int, pressed: Boolean, meta: Int)
        fun text(text: String)
        fun preedit(text: String, cursor: Int)
    }

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_ACTION_NONE
        return Connection()
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        if (isSystemKey(keyCode)) return super.onKeyDown(keyCode, event)
        typed.key(keyCode, true, event.metaState)
        val ch = event.unicodeChar
        if (ch != 0 && !Character.isISOControl(ch)) typed.text(String(Character.toChars(ch)))
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (isSystemKey(keyCode)) return super.onKeyUp(keyCode, event)
        typed.key(keyCode, false, event.metaState)
        return true
    }

    /** Back, volume and the like stay with Android. */
    private fun isSystemKey(code: Int): Boolean = KeyEvent.isMediaSessionKey(code) ||
        code == KeyEvent.KEYCODE_BACK || code == KeyEvent.KEYCODE_VOLUME_UP ||
        code == KeyEvent.KEYCODE_VOLUME_DOWN || code == KeyEvent.KEYCODE_VOLUME_MUTE ||
        code == KeyEvent.KEYCODE_POWER || code == KeyEvent.KEYCODE_HOME

    /** Shows the on-screen keyboard, or hides it. */
    fun showKeyboard(show: Boolean) {
        val ime = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        if (show) {
            requestFocus()
            ime.showSoftInput(this, 0)
        } else {
            ime.hideSoftInputFromWindow(windowToken, 0)
        }
    }

    /** The keyboard's side: Aurea keeps the text, so this holds only what is still composing. */
    private inner class Connection : BaseInputConnection(this@AureaSurface, false) {
        private var composing = ""

        override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
            composing = ""
            typed.preedit("", -1)
            if (text.isNotEmpty()) typed.text(text.toString())
            return true
        }

        override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
            composing = text.toString()
            // Rust counts the cursor in UTF-8 bytes.
            typed.preedit(composing, composing.toByteArray(Charsets.UTF_8).size)
            return true
        }

        override fun finishComposingText(): Boolean {
            if (composing.isNotEmpty()) {
                typed.preedit("", -1)
                typed.text(composing)
                composing = ""
            }
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            // some keyboards ask for a lot; a field holds little of it.
            repeat(beforeLength.coerceAtMost(MAX_DELETE)) { tap(KeyEvent.KEYCODE_DEL) }
            repeat(afterLength.coerceAtMost(MAX_DELETE)) { tap(KeyEvent.KEYCODE_FORWARD_DEL) }
            return true
        }

        override fun sendKeyEvent(event: KeyEvent): Boolean {
            when (event.action) {
                KeyEvent.ACTION_DOWN -> onKeyDown(event.keyCode, event)
                KeyEvent.ACTION_UP -> onKeyUp(event.keyCode, event)
            }
            return true
        }

        override fun performEditorAction(actionCode: Int): Boolean {
            tap(KeyEvent.KEYCODE_ENTER)
            return true
        }

        private fun tap(code: Int) {
            typed.key(code, true, 0)
            typed.key(code, false, 0)
        }
    }
}
