package rust.androidnativesurface

import android.app.Activity
import android.graphics.SurfaceTexture
import android.os.Bundle
import android.view.Surface
import android.view.SurfaceControl
import android.view.SurfaceControlViewHost
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.TextureView

class MainActivity : Activity() {
    companion object {
        init {
            System.loadLibrary("android_native_surface")
            init()
        }

        private external fun init()
        external fun renderToSurface(surface: Surface, control: SurfaceControl?)
        external fun renderToSurfaceTexture(surfaceTexture: SurfaceTexture)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        println("Root SC ${window.rootSurfaceControl}")
        println("VIEW ${window.decorView}")
        val control = SurfaceControl.Builder()
        control.setHidden(false)
        control.setName("Foobarbaz")
//        control.setBufferSize(512, 512) // Only set by default with setParent, but we cannot parent to the root surface
        val sc = control.build()
        val surface = Surface(sc)
        window.takeSurface(object : SurfaceHolder.Callback2 {
            override fun surfaceCreated(holder: SurfaceHolder) {
                println("Surface for holder created: ${holder.surface}")
//                renderToSurface(holder.surface, null)
                println("Root SC ${window.rootSurfaceControl}")
//                println("VIEW ${window.decorView}")
//                TODO: No constructor for a parent Surface, only a SurfaceControl
//                control.setParent(holder.surface)
                val t = window.rootSurfaceControl?.buildReparentTransaction(sc)!!
                val frame = holder.surfaceFrame
                t.setBufferSize(sc, frame.width(), frame.height())
                println("t: $t")
//                t.apply()
                window.rootSurfaceControl?.applyTransactionOnDraw(t)
                renderToSurface(surface, sc)
            }

            override fun surfaceChanged(
                holder: SurfaceHolder,
                format: Int,
                width: Int,
                height: Int
            ) {
                println("Surface changed: ${holder.surface}")
//                renderToSurface(surface, sc)
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                //                    TODO("Not yet implemented")
            }

            override fun surfaceRedrawNeeded(holder: SurfaceHolder) {
//                println("VIEW ${window.decorView}")
                println("Surface needs redraw: ${holder.surface}")
//                renderToSurface(holder.surface)
                val t = SurfaceControl.Transaction();
                val frame = holder.surfaceFrame
                t.setBufferSize(sc, frame.width(), frame.height())
                t.apply()
                renderToSurface(surface, sc)

            }
        })

        return
        setContentView(R.layout.activity_main)

        val surfaceView: SurfaceView = findViewById(R.id.surface_view)
        println("SurfaceView: ${surfaceView.holder.surface}")
        surfaceView.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                println("SurfaceView created: ${holder.surface}")
//                renderToSurface(holder.surface)
            }

            override fun surfaceChanged(holder: SurfaceHolder, p1: Int, p2: Int, p3: Int) {
                println("SurfaceView changed: ${holder.surface}")
//                renderToSurface(holder.surface)
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
//                    renderToSurface(surface)
                }
            }

            override fun onSurfaceTextureSizeChanged(
                surfaceTexture: SurfaceTexture,
                p1: Int,
                p2: Int
            ) {
                Surface(surfaceTexture).let { surface ->
                    println("Java TextureView resized: $surfaceTexture, $surface")
//                    renderToSurface(surface)
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
