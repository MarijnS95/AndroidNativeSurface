package rust.androidnativesurface

import android.app.Activity
import android.graphics.SurfaceTexture
import android.hardware.DataSpace
import android.hardware.display.DisplayManager
import android.os.Bundle
import android.view.Display.HdrCapabilities
import android.view.Surface
import android.view.SurfaceControl
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.TextureView
import android.view.WindowManager

class MainActivity : Activity() {
    companion object {
        init {
            System.loadLibrary("android_native_surface")
            init()
        }

        private external fun init()
        external fun renderToSurface(surface: Surface)
        external fun renderToSurfaceTexture(surfaceTexture: SurfaceTexture)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val display = display!!
        println("Display $display")
        println("Display ID ${display.displayId}")
        println("isHdr ${display.isHdr}")
        println("isWideColorGamut ${display.isWideColorGamut}")
        println("preferredWideGamutColorSpace ${display.preferredWideGamutColorSpace}")
//        println("hdrSdrRatio ${display.hdrSdrRatio}")
//        println("isHdrSdrRatioAvailable ${display.isHdrSdrRatioAvailable}")
        println("hdrCapabilities ${display.hdrCapabilities}")
        println("desiredMaxAverageLuminance ${display.hdrCapabilities.desiredMaxAverageLuminance}")
        println("desiredMinLuminance ${display.hdrCapabilities.desiredMinLuminance}")
        println("desiredMaxLuminance ${display.hdrCapabilities.desiredMaxLuminance}")
        display.hdrCapabilities.supportedHdrTypes.forEach {
            val name = if (it == HdrCapabilities.HDR_TYPE_DOLBY_VISION) {
                "HDR_TYPE_DOLBY_VISION"
            } else if (it == HdrCapabilities.HDR_TYPE_HDR10) {
                "HDR_TYPE_HDR10"
            } else if (it == HdrCapabilities.HDR_TYPE_HDR10_PLUS) {
                "HDR_TYPE_HDR10_PLUS"
            } else if (it == HdrCapabilities.HDR_TYPE_HLG) {
                "HDR_TYPE_HLG"
            } else if (it == HdrCapabilities.HDR_TYPE_INVALID) {
                "HDR_TYPE_INVALID"
            } else {
            }

            println(name)
        }

//        println("supportedHdrTypes ${display.mode.supportedHdrTypes}")
        var dm = getSystemService(DisplayManager::class.java)
//        println(dm.hdrConversionMode)

        val surfaceView: SurfaceView = findViewById(R.id.surface_view)
        println("SurfaceView: ${surfaceView.holder.surface}")
        surfaceView.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                println("SurfaceView created: ${holder.surface}")

                SurfaceControl.Transaction()
//        .setExtendedRangeBrightness(surfaceView.surfaceControl, 10.0f, 10.0f)
                    .setDataSpace(surfaceView.surfaceControl, DataSpace.DATASPACE_DISPLAY_P3)
                    .apply()

                renderToSurface(holder.surface)

                SurfaceControl.Transaction()
//        .setExtendedRangeBrightness(surfaceView.surfaceControl, 10.0f, 10.0f)
                    .setDataSpace(surfaceView.surfaceControl, DataSpace.DATASPACE_DISPLAY_P3)
                    .apply()
            }

            override fun surfaceChanged(holder: SurfaceHolder, p1: Int, p2: Int, p3: Int) {
                println("SurfaceView changed: ${holder.surface}")
                renderToSurface(holder.surface)
            }

            override fun surfaceDestroyed(p0: SurfaceHolder) {
//                    TODO("Not yet implemented")
            }
        })

        val javaTextureView: TextureView = findViewById(R.id.java_texture_view)
        println("Java TextureView: ${javaTextureView.surfaceTexture}")
        javaTextureView.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(
                surfaceTexture: SurfaceTexture,
                p1: Int,
                p2: Int
            ) {
                Surface(surfaceTexture).let { surface ->
                    println("Java TextureView created: $surfaceTexture, $surface")
                    renderToSurface(surface)
                }
            }

            override fun onSurfaceTextureSizeChanged(
                surfaceTexture: SurfaceTexture,
                p1: Int,
                p2: Int
            ) {
                Surface(surfaceTexture).let { surface ->
                    println("Java TextureView resized: $surfaceTexture, $surface")
                    renderToSurface(surface)
                }
            }

            override fun onSurfaceTextureDestroyed(p0: SurfaceTexture): Boolean {
//                TODO("Not yet implemented")
                return true
            }

            override fun onSurfaceTextureUpdated(p0: SurfaceTexture) {
//                TODO("Not yet implemented")
            }
        }

        val rustTextureView: TextureView = findViewById(R.id.rust_texture_view)
        println("Rust TextureView: ${rustTextureView.surfaceTexture}")
        rustTextureView.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(
                surfaceTexture: SurfaceTexture,
                p1: Int,
                p2: Int
            ) {
                println("Rust TextureView created: $surfaceTexture")
                renderToSurfaceTexture(surfaceTexture)
            }

            override fun onSurfaceTextureSizeChanged(
                surfaceTexture: SurfaceTexture,
                p1: Int,
                p2: Int
            ) {
                println("Rust TextureView resized: $surfaceTexture")
                renderToSurfaceTexture(surfaceTexture)
            }

            override fun onSurfaceTextureDestroyed(p0: SurfaceTexture): Boolean {
//                TODO("Not yet implemented")
                return true
            }

            override fun onSurfaceTextureUpdated(p0: SurfaceTexture) {
//                TODO("Not yet implemented")
            }
        }
    }
}
