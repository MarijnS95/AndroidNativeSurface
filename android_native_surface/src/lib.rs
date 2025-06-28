use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader},
    thread,
};

use android_logger::FilterBuilder;
use glutin::{
    config::Config,
    context::{ContextApi, ContextAttributesBuilder, NotCurrentContext},
    display::Display,
    prelude::*,
};
use jni::{
    objects::{JClass, JObject},
    JNIEnv,
};
use log::{debug, info, LevelFilter};
use ndk::{
    hardware_buffer_format::HardwareBufferFormat, native_window::NativeWindow,
    surface_texture::SurfaceTexture, trace::Section,
};
use raw_window_handle::DisplayHandle;

mod support;

struct NativeGL {
    gl_display: Display,
    // TODO: HardwareBufferFormat does not derive Hash?
    gl_contexts: HashMap</*HardwareBufferFormat*/ i32, (Option<NotCurrentContext>, Config)>,
    /// Lazy-initialized when the first context+surface is made current
    renderer: Option<support::Renderer>,
}

impl NativeGL {
    fn new() -> Self {
        let _t = Section::new("Gl::new()").unwrap();

        // TODO: EGL can update the format of the window by choosing a different format,
        // but not if this producer (Surface/NativeWindow) comes from an ImageReader.
        // let format = dbg!(window.format());
        let format = HardwareBufferFormat::R8G8B8X8_UNORM;

        let display_handle = DisplayHandle::android();

        let gl_display = support::create_display(display_handle);

        let template = support::config_template(format);
        let gl_config = unsafe {
            gl_display
                .find_configs(template)
                .unwrap()
                .reduce(|accum, config| {
                    // Find the config with the maximum number of samples.
                    //
                    // In general if you're not sure what you want in template you can request or
                    // don't want to require multisampling for example, you can search for a
                    // specific option you want afterwards.
                    //
                    // XXX however on macOS you can request only one config, so you should do
                    // a search with the help of `find_configs` and adjusting your template.
                    if config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        };

        println!(
            "Picked a config with {} samples, {:?}, alpha: {}",
            gl_config.num_samples(),
            gl_config.color_buffer_type(),
            gl_config.alpha_size()
        );

        // The context creation part. It can be created before surface and that's how
        // it's expected in multithreaded + multiwindow operation mode, since you
        // can send NotCurrentContext, but not Surface.
        let context_attributes = ContextAttributesBuilder::new().build(None);

        // Since glutin by default tries to create OpenGL core context, which may not be
        // present we should try gles.
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(None);
        let gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        };

        Self {
            gl_display,
            gl_contexts: std::iter::once((format.into(), (Some(gl_context), gl_config))).collect(),
            renderer: None,
        }
    }

    fn create_gl_window(&mut self, window: NativeWindow) -> support::GlWindow {
        debug!("Add window {window:?}");
        let _t = Section::new("Gl::add_window()").unwrap();

        // TODO: Query format from NativeWindow
        // (even though the config implicitly overwrites it)
        let format = HardwareBufferFormat::R8G8B8X8_UNORM;
        let (_gl_context_rc, gl_config) = self
            .gl_contexts
            .get_mut(&format.into())
            .expect("No context/config for format");

        // Create a wrapper for GL window and surface.
        support::GlWindow::from_existing(&self.gl_display, window, gl_config)
    }

    fn render_to_gl_window(&mut self, gl_window: &support::GlWindow) {
        debug!("Render to window {gl_window:?}");
        let _t = Section::new("Gl::render_to_window()").unwrap();

        let (gl_context_rc, gl_context, gl_window, renderer) = {
            let _t = Section::new("Preparation").unwrap();

            // TODO: Lazy-init more configs!
            // let format = window.format();
            let format = HardwareBufferFormat::R8G8B8X8_UNORM;
            let (gl_context_rc, _gl_config) = self
                .gl_contexts
                .get_mut(&format.into())
                .expect("No context/config for format");
            let gl_context = gl_context_rc.take().expect("Didn't put back");

            // Make it current and load symbols.
            let gl_context = gl_context.make_current(&gl_window.surface).unwrap();

            let renderer = self.renderer.get_or_insert_with(|| {
                let _t = Section::new("Renderer setup").unwrap();
                support::Renderer::new(&self.gl_display)
            });

            (gl_context_rc, gl_context, gl_window, renderer)
        };

        {
            let _t = Section::new("resize").unwrap();
            renderer.resize(gl_window.window.width(), gl_window.window.height());
        }

        {
            let _t = Section::new("draw").unwrap();
            renderer.draw();
        }

        {
            let _t = Section::new("swap_buffers").unwrap();
            gl_window
                .surface
                .swap_buffers(&gl_context)
                .expect("Cannot swap buffers");
        }

        {
            let _t = Section::new("make_not_current").unwrap();
            *gl_context_rc = Some(
                gl_context
                    .make_not_current()
                    .expect("Cannot uncurrent GL context"),
            );
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_init(
    _env: JNIEnv,
    _class: JClass,
) {
    let _t = Section::new("init").unwrap();
    android_logger::init_once(
        android_logger::Config::default()
            .with_filter(
                FilterBuilder::new()
                    .filter_level(LevelFilter::Trace)
                    // Disable Trace-level messages on JNI crate (specifically around accessing Rust fields)
                    .filter_module("jni", LevelFilter::Debug)
                    .build(),
            )
            // android_logger erroneously doesn't set log::set_max_level() to the highest
            // that the filter could match on, hence we have to set it again manually:
            // https://github.com/rust-mobile/android_logger-rs/issues/80
            .with_max_level(LevelFilter::Trace),
    );

    let file = {
        let (read, write) = rustix::pipe::pipe().unwrap();
        rustix::stdio::dup2_stdout(&write).unwrap();
        rustix::stdio::dup2_stderr(&write).unwrap();

        File::from(read)
    };

    thread::spawn(move || -> io::Result<()> {
        let mut reader = BufReader::new(file);
        let mut buffer = String::new();
        loop {
            buffer.clear();
            let len = reader.read_line(&mut buffer)?;
            if len == 0 {
                break Ok(());
            } else {
                info!(target: "RustStdoutStderr", "{buffer}");
            }
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeGL_init(
    mut env: JNIEnv,
    _class: JClass,
    native_gl: JObject,
) {
    let gl = NativeGL::new();
    unsafe { env.set_rust_field(native_gl, "mNative", gl) }.unwrap();
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeSurfaceWrapper_setSurface(
    mut env: JNIEnv,
    _class: JClass,
    native_gl: JObject,
    native_surface_wrapper: JObject,
    surface: JObject,
) {
    let _t = Section::new("setSurface").unwrap();
    debug!("Add Java Surface {surface:?} to {native_surface_wrapper:?}");

    let window =
        unsafe { NativeWindow::from_surface(env.get_native_interface(), surface.into_raw()) }
            .unwrap();
    let mut native_gl =
        unsafe { env.get_rust_field::<_, _, NativeGL>(native_gl, "mNative") }.unwrap();
    let gl_window = native_gl.create_gl_window(window);
    drop(native_gl);
    unsafe { env.set_rust_field(native_surface_wrapper, "mNative", gl_window) }.unwrap();
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeSurfaceWrapper_removeSurface(
    mut env: JNIEnv,
    _class: JClass,
    native_surface_wrapper: JObject,
) {
    let _t = Section::new("removeSurface").unwrap();
    debug!("Remove Java Surface from {native_surface_wrapper:?}");

    let gl_window: support::GlWindow =
        unsafe { env.take_rust_field(native_surface_wrapper, "mNative") }.unwrap();

    debug!("Removed surface was {gl_window:?}");
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeSurfaceWrapper_renderToSurface(
    mut env: JNIEnv,
    _class: JClass,
    native_gl: JObject,
    native_surface_wrapper: JObject,
) {
    let _t = Section::new("renderToSurface").unwrap();
    debug!("Render to Java Surface via {native_surface_wrapper:?}");

    // SAFETY: TODO
    let mut env2 = unsafe { env.unsafe_clone() };

    let gl_window =
        unsafe { env.get_rust_field::<_, _, support::GlWindow>(native_surface_wrapper, "mNative") }
            .unwrap();
    debug!("Java Surface is {gl_window:?}");

    let mut native_gl =
        unsafe { env2.get_rust_field::<_, _, NativeGL>(native_gl, "mNative") }.unwrap();

    native_gl.render_to_gl_window(&gl_window)
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeSurfaceTextureWrapper_setSurfaceTexture(
    mut env: JNIEnv,
    _class: JClass,
    native_gl: JObject,
    native_surface_texture_wrapper: JObject,
    surface_texture: JObject,
) {
    let _t = Section::new("setSurfaceTexture").unwrap();
    debug!("Add Java SurfaceTexture {surface_texture:?} to {native_surface_texture_wrapper:?}");

    // SAFETY: The handle is valid and we're not storing this SurfaceTexture anywhere.  The lifetime
    // on the Java side is guiding (and a Surface/NativeWindow can exist independently from it).
    let surface_texture = unsafe {
        SurfaceTexture::from_surface_texture(env.get_native_interface(), surface_texture.into_raw())
            .unwrap()
    };
    let window = surface_texture.acquire_native_window().unwrap();
    let mut native_gl =
        unsafe { env.get_rust_field::<_, _, NativeGL>(native_gl, "mNative") }.unwrap();
    let gl_window = native_gl.create_gl_window(window);
    drop(native_gl);
    unsafe { env.set_rust_field(native_surface_texture_wrapper, "mNative", gl_window) }.unwrap();
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeSurfaceTextureWrapper_removeSurfaceTexture(
    mut env: JNIEnv,
    _class: JClass,
    native_surface_texture_wrapper: JObject,
) {
    let _t = Section::new("removeSurfaceTexture").unwrap();
    debug!("Remove Java Surface from {native_surface_texture_wrapper:?}");

    let gl_window: support::GlWindow =
        unsafe { env.take_rust_field(native_surface_texture_wrapper, "mNative") }.unwrap();

    debug!("Removed surface was {gl_window:?}");
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024NativeSurfaceTextureWrapper_renderToSurfaceTexture(
    mut env: JNIEnv,
    _class: JClass,
    native_gl: JObject,
    native_surface_texture_wrapper: JObject,
) {
    let _t = Section::new("renderToSurfaceTexture").unwrap();
    debug!("Render to Java Surface via {native_surface_texture_wrapper:?}");

    // SAFETY: TODO
    let mut env2 = unsafe { env.unsafe_clone() };

    let gl_window = unsafe {
        env.get_rust_field::<_, _, support::GlWindow>(native_surface_texture_wrapper, "mNative")
    }
    .unwrap();
    debug!("Java Surface is {gl_window:?}");

    let mut native_gl =
        unsafe { env2.get_rust_field::<_, _, NativeGL>(native_gl, "mNative") }.unwrap();

    native_gl.render_to_gl_window(&gl_window)
}
