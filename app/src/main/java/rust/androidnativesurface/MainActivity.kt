package rust.androidnativesurface

import android.app.Activity
import android.graphics.SurfaceTexture
import android.os.Bundle
import android.view.Surface
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
    }

    class NativeGL {
        private val mNative: Long = 0 // TODO: var?
        private external fun init(self: NativeGL)

        init {
            init(this)
        }

        // TODO: Add a destructor
    }

    open class NativeSurfaceWrapper(private val gl: NativeGL) {
        private var mNative: Long = 0

        private external fun setSurface(gl: NativeGL, self: NativeSurfaceWrapper, surface: Surface)
        private external fun removeSurface(self: NativeSurfaceWrapper)
        private external fun renderToSurface(gl: NativeGL, self: NativeSurfaceWrapper)

        fun setSurface(surface: Surface) {
            assert(mNative == 0L)
            setSurface(gl, this, surface)
            assert(mNative != 0L)
        }

        fun redraw() {
            assert(mNative != 0L)
            renderToSurface(gl, this)

        }

        fun removeSurface() {
            assert(mNative != 0L)
            removeSurface(this)
            assert(mNative == 0L)
        }
    }

    class SurfaceHolderWrapper(private val gl: NativeGL) : NativeSurfaceWrapper(gl),
        SurfaceHolder.Callback {
        override fun surfaceCreated(holder: SurfaceHolder) {
            println("SurfaceView created: ${holder.surface}")
            setSurface(holder.surface)
        }

        override fun surfaceChanged(holder: SurfaceHolder, p1: Int, p2: Int, p3: Int) {
            println("SurfaceView changed: ${holder.surface}")
            redraw()
        }

        override fun surfaceDestroyed(holder: SurfaceHolder) {
            println("SurfaceView destroyed: ${holder.surface}")
            removeSurface()
        }
    }

    class SurfaceTextureWrapper(private val gl: NativeGL) : NativeSurfaceWrapper(gl),
        TextureView.SurfaceTextureListener {

        override fun onSurfaceTextureAvailable(
            surfaceTexture: SurfaceTexture, p1: Int, p2: Int
        ) {
            Surface(surfaceTexture).let { surface ->
                println("Java TextureView created: $surfaceTexture, $surface")
                setSurface(surface)
                // No "changed" callback that always fires, so we have to draw immediately
                redraw()
            }
        }

        override fun onSurfaceTextureSizeChanged(
            surfaceTexture: SurfaceTexture, p1: Int, p2: Int
        ) {
            println("Java TextureView resized: $surfaceTexture")
            redraw()
        }

        override fun onSurfaceTextureDestroyed(surfaceTexture: SurfaceTexture): Boolean {
            println("Java TextureView removed: $surfaceTexture")
            removeSurface()
            return true
        }

        override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) {
            // Called after our app has swapped buffers to it
            println("Java TextureView $surfaceTexture updated at ${surfaceTexture.timestamp}")
        }
    }

    class NativeSurfaceTextureWrapper(private val gl: NativeGL) :
        TextureView.SurfaceTextureListener {
        private var mNative: Long = 0

        private external fun setSurfaceTexture(
            gl: NativeGL, self: NativeSurfaceTextureWrapper, surface: SurfaceTexture
        )

        private external fun removeSurfaceTexture(self: NativeSurfaceTextureWrapper)
        private external fun renderToSurfaceTexture(gl: NativeGL, self: NativeSurfaceTextureWrapper)

        override fun onSurfaceTextureAvailable(
            surfaceTexture: SurfaceTexture, p1: Int, p2: Int
        ) {
            println("Rust TextureView created: $surfaceTexture")
            assert(mNative == 0L)
            setSurfaceTexture(gl, this, surfaceTexture)
            assert(mNative != 0L)
            // No "changed" callback that always fires, so we have to draw immediately
            renderToSurfaceTexture(gl, this)
        }

        override fun onSurfaceTextureSizeChanged(
            surfaceTexture: SurfaceTexture, p1: Int, p2: Int
        ) {
            println("Rust TextureView resized: $surfaceTexture")
            assert(mNative != 0L)
            renderToSurfaceTexture(gl, this)
        }

        override fun onSurfaceTextureDestroyed(surfaceTexture: SurfaceTexture): Boolean {
            println("Rust TextureView destroyed: $surfaceTexture")
            assert(mNative != 0L)
            removeSurfaceTexture(this)
            assert(mNative == 0L)
            return true
        }

        override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) {
            // Called after our app has swapped buffers to it
            println("Rust TextureView $surfaceTexture updated at ${surfaceTexture.timestamp}")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val gl = NativeGL()

        val surfaceView: SurfaceView = findViewById(R.id.surface_view)
        println("SurfaceView: ${surfaceView.holder.surface}")
        surfaceView.holder.addCallback(SurfaceHolderWrapper(gl))

        val javaTextureView: TextureView = findViewById(R.id.java_texture_view)
        println("Java TextureView: ${javaTextureView.surfaceTexture}")
        // TODO: This breaks because the Surface() (producer-end) changes for the given SurfaceTexture() (consumer-end)
        javaTextureView.surfaceTextureListener = SurfaceTextureWrapper(gl)

        val rustTextureView: TextureView = findViewById(R.id.rust_texture_view)
        println("Rust TextureView: ${rustTextureView.surfaceTexture}")
        rustTextureView.surfaceTextureListener = NativeSurfaceTextureWrapper(gl)
    }
}
