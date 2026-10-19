package com.aurea.smoke

import android.os.Bundle
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.app.Activity

class MainActivity : Activity(), SurfaceHolder.Callback {

    companion object {
        init {
            System.loadLibrary("aurea")
        }

        // Static, so JNI looks for Java_com_aurea_smoke_MainActivity_*;
        // without it they live on the Companion class and none is found.
        @JvmStatic external fun nativeInit(activity: Activity)
        @JvmStatic external fun nativeOnPause()
        @JvmStatic external fun nativeOnResume()
        @JvmStatic external fun nativeOnDestroy()
        @JvmStatic external fun nativeOnSurfaceLost()
        @JvmStatic external fun nativeOnSurfaceRecreated()
    }

    private var surfaceWasLost = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        nativeInit(this)
        findViewById<SurfaceView>(R.id.surface).holder.addCallback(this)
    }

    override fun onPause() {
        super.onPause()
        nativeOnPause()
    }

    override fun onResume() {
        super.onResume()
        nativeOnResume()
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

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {}

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        nativeOnSurfaceLost()
        surfaceWasLost = true
    }
}
