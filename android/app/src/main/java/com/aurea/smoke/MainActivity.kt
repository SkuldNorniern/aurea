package com.aurea.smoke

import android.os.Bundle
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity(), SurfaceHolder.Callback {

    companion object {
        init {
            System.loadLibrary("aurea")
        }

        external fun nativeInit(activity: android.app.Activity)
        external fun nativeOnPause()
        external fun nativeOnResume()
        external fun nativeOnDestroy()
        external fun nativeOnSurfaceLost()
        external fun nativeOnSurfaceRecreated()
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
