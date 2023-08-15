package rust.androidnativesurface

import android.app.Activity
import android.graphics.SurfaceTexture
import android.hardware.HardwareBuffer
import android.hardware.SyncFence
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.view.Surface
import android.view.SurfaceControl
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.TextureView
import java.io.FileDescriptor

data class RenderedHardwareBuffer(
    val hardware_buffer: HardwareBuffer,
    val fd: Int
)


class MainActivity : Activity() {
    companion object {
        init {
            System.loadLibrary("android_native_surface")
            init()
        }

        private external fun init()
        external fun renderToSurface(surface: Surface)
        external fun renderToSurfaceTexture(surfaceTexture: SurfaceTexture)

        external fun renderHardwareBuffer() // : RenderedHardwareBuffer
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val surfaceView: SurfaceView = findViewById(R.id.surface_view)
        println("SurfaceView: ${surfaceView.holder.surface}")


        renderHardwareBuffer()
//
//        val hwbuf = renderHardwareBuffer()
//        println("Have HardwareBuffer ${hwbuf.hardware_buffer}, wait for fd ${hwbuf.fd}")
//        val t = SurfaceControl.Transaction()
//
//        // TODO: This is @hide :(
//        val f = SyncFence.create(ParcelFileDescriptor.adoptFd(hwbuf.fd))
//        println("Fence $f")
//        t.setBuffer(
//            surfaceView.surfaceControl,
//            hwbuf.hardware_buffer,
//            f
//        )
        return

        surfaceView.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                println("SurfaceView created: ${holder.surface}")
//                holder.surface.attachAndQueueBufferWithColorSpace()
                renderToSurface(holder.surface)
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
