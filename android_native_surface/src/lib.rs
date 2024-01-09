use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader},
    sync::Arc,
    thread,
    time::Duration,
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
    choreographer::Choreographer,
    hardware_buffer::{HardwareBufferUsage, Rect},
    hardware_buffer_format::HardwareBufferFormat,
    looper::ThreadLooper,
    media::image_reader::{AcquireResult, ImageFormat, ImageReader},
    native_window::{NativeWindow, NativeWindowTransform},
    surface_control::{SurfaceControl, SurfaceTransaction},
    surface_texture::SurfaceTexture,
    trace::Section,
};
use raw_window_handle::DisplayHandle;

mod support;

/// Wraps a [`NativeWindow`], its [`SurfaceControl`], a specially-crafted [`ImageReader`]
/// and a [`support::GlWindow`] renderer into that `ImageReader`.
///
/// The resulting buffers from the [`ImageReader`] have to be manually presented to the
/// [`SurfaceControl`].  This is just a playground for experimenting with [`SurfaceControl`] as one
/// would otherwise let the [`support::GlWindow`] present into the [`NativeWindow`] directly.
///
/// Alternatively the buffer(s) could be independently allocated and presented, voiding the need for
/// an intermediary "EGL swapchain" and the [`Surface::swap_buffers()`] machinery with it.
#[derive(Debug)]
struct Window {
    #[allow(dead_code, reason = "Keepalive")]
    original_window: NativeWindow,
    surface_control: SurfaceControl,
    image_reader: ImageReader,
    render_window: support::GlWindow,
}

unsafe impl Send for Window {} // TODO: ImageReader?

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
        // XXX: The X8 format doesn't seem to work, as we cannot make it to match on a GL format
        // (I'd expect to get it with the default `alpha: 8, transparency: false` selectors...).
        let format = HardwareBufferFormat::R8G8B8A8_UNORM;

        // TODO: Looking that the window format instead may be interesting, but afaik it's "just"
        // a preference and used for the internal buffer-producer-consumer which (the dequeue
        // operation) is "inaccessible" from the public API.  We can present any arbitrary buffer
        // format to it.

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

    fn create_gl_window(&mut self, original_window: NativeWindow) -> Window {
        debug!("Add window {original_window:?}");
        let _t = Section::new("Gl::add_window()").unwrap();

        // TODO: Query format from NativeWindow
        // (even though the config implicitly overwrites it)
        // let format = dbg!(og_window.format());
        // TODO: EGL can update the format of the window by choosing a different format,
        // but not if this producer (Surface/NativeWindow) comes from an ImageReader.
        let format = HardwareBufferFormat::R8G8B8A8_UNORM;
        let (_gl_context_rc, gl_config) = self
            .gl_contexts
            .get_mut(&format.into())
            .expect("No context/config for format");

        dbg!(&original_window);

        let surface_control = SurfaceControl::create_from_window(&original_window, c"foo")
            .expect("Failed to create SC on NativeWindow, which requires a bugfix in Android 15");

        let image_format = match format {
            HardwareBufferFormat::R8G8B8X8_UNORM => ImageFormat::RGBX_8888,
            HardwareBufferFormat::R8G8B8A8_UNORM => ImageFormat::RGBA_8888,
            HardwareBufferFormat::R5G6B5_UNORM => ImageFormat::RGB_565,
            x => todo!("{x:?}"),
        };
        let image_reader = ImageReader::new_with_usage(
            original_window.width(),
            original_window.height(),
            image_format,
            // AImageReader_newWithUsage: format 43 is not supported with usage 0x300 by AImageReader
            HardwareBufferUsage::GPU_FRAMEBUFFER | HardwareBufferUsage::GPU_SAMPLED_IMAGE,
            // TODO: Might have to wait until https://android.googlesource.com/platform/frameworks/av/+/master/media/ndk/NdkImageReader.cpp#743 AImageReader_newWithDataSpace() lands
            4,
        )
        .unwrap();
        let window = image_reader.window().unwrap();

        // Create a wrapper for GL window and surface.
        let target_window = support::GlWindow::from_existing(&self.gl_display, window, gl_config);

        Window {
            original_window,
            surface_control,
            image_reader,
            render_window: target_window,
        }
    }

    fn render_to_gl_window(&mut self, window: &Window) {
        debug!("Render to window {window:?}");
        let _t = Section::new("Gl::render_to_window()").unwrap();

        let (gl_context_rc, gl_context, window, renderer) = {
            let _t = Section::new("Preparation").unwrap();

            // TODO: Lazy-init more configs!
            // let format = window.format();
            let format = HardwareBufferFormat::R8G8B8A8_UNORM;
            let (gl_context_rc, _gl_config) = self
                .gl_contexts
                .get_mut(&format.into())
                .expect("No context/config for format");
            let gl_context = gl_context_rc.take().expect("Didn't put back");

            // Make it current and load symbols.
            let gl_context = gl_context
                .make_current(&window.render_window.surface)
                .unwrap();

            let renderer = self.renderer.get_or_insert_with(|| {
                let _t = Section::new("Renderer setup").unwrap();
                support::Renderer::new(&self.gl_display)
            });

            (gl_context_rc, gl_context, window, renderer)
        };

        {
            let _t = Section::new("resize").unwrap();
            // Should be the same as window.original_window
            renderer.resize(
                window.render_window.window.width(),
                window.render_window.window.height(),
            );
        }

        let i = &window.image_reader;

        dbg!(i.acquire_next_image());
        dbg!(unsafe { i.acquire_next_image_async() });

        {
            let _t = Section::new("draw").unwrap();
            renderer.draw();
        }

        {
            let _t = Section::new("swap_buffers").unwrap();
            window
                .render_window
                .surface
                .swap_buffers(&gl_context)
                .expect("Cannot swap buffers");
        }

        let (img, fence) = {
            let _t = Section::new("acquire_image").unwrap();
            // let img = i.acquire_next_image().unwrap();
            // let fence = None;
            // A buffer only becomes available after swapping
            let AcquireResult::Image(img_fence) = unsafe { i.acquire_next_image_async() }.unwrap()
            else {
                panic!()
            };
            img_fence
        };

        {
            let _t = Section::new("set").unwrap();
            let mut t = SurfaceTransaction::new();
            // t.set_on_commit(Box::new(|stats| {
            //     dbg!(stats);
            // }));
            // t.set_on_complete(Box::new(|stats| {
            //     dbg!(stats);
            // }));
            // t.set_visibility(&sc, ndk::surface_control::Visibility::Hide);
            dbg!(&img);
            dbg!(&fence);
            t.set_buffer(
                &window.surface_control,
                &img.hardware_buffer().unwrap(),
                fence,
            );
            t.apply();

            let c = Choreographer::instance().unwrap();
            // let mut latest = Arc::new(Mutex::new(None));
            dbg!(&c);
            // c.post_frame_callback(Box::new(|d| {
            //     dbg!(d);
            // }));
            // let set_latest = latest.clone();
            let mut bladiebla = 1;

            let x = 1;
            // : *const u32 = std::ptr::null();
            let x = Arc::new(x);

            let l = ThreadLooper::for_thread();
            dbg!(&l);

            dbg!(std::thread::current().id());
            c.post_vsync_callback(Box::new(move |vsync: &'_ _| {
                bladiebla += 1;
                dbg!(std::thread::current().id());
                // dbg!(std::backtrace::Backtrace::force_capture());
                dbg!(bladiebla);
                dbg!(unsafe { *x });
                // dbg!(vsync);
                // let mut g = set_latest.lock().unwrap();
                // let len = vsync.frame_timelines_length();
                // *g = Some((
                //     vsync.frame_timeline_vsync_id(len - 1),
                //     vsync.frame_timeline_expected_presentation_time(len - 1),
                // ));
            }));
            let _live = c.register_refresh_rate_callback(Box::new(|refresh_rate| {
                dbg!(std::thread::current().id());
                dbg!(refresh_rate);
            }));
            std::mem::forget(_live);

            dbg!(bladiebla);

            let og_window = window.original_window.clone();
            let sc = window.surface_control.clone();

            std::thread::spawn(move || {
                dbg!(std::thread::current().id());

                // dbg!(ThreadLooper::for_thread());
                // dbg!(Choreographer::instance());
                // // let l = ThreadLooper::prepare();
                // // let c = Choreographer::instance().unwrap();
                // dbg!(&c);
                // c.post_frame_callback(Box::new(|c| {
                //     dbg!(c);
                //     dbg!(std::thread::current().id());
                // }));
                // std::mem::forget(c.register_refresh_rate_callback(Box::new(|x| {
                //     dbg!(x);
                //     dbg!(std::thread::current().id());
                // })));

                // loop {
                //     let x = l.poll_once().unwrap();
                //     if matches!(x, ndk::looper::Poll::Callback) {
                //         dbg!(&x);
                //     }
                //     // l.poll_all();
                //     // std::thread::sleep(Duration::from_millis(100));
                // }

                std::thread::sleep(Duration::from_millis(100));
                for i in 0..20 {
                    t.set_on_complete(Box::new(|stats| {
                        // dbg!(std::thread::current().id());
                        // dbg!(stats);
                    }));
                    // if let Some((vsync_id, latest)) = latest.lock().unwrap().as_ref() {
                    //     dbg!(vsync_id, latest);
                    //     t.set_frame_timeline(*vsync_id + 100 * i);
                    //     t.set_desired_present_time(*latest + i as u32 * Duration::from_millis(100));
                    // }
                    // t.set_on_commit(Box::new(|stats| {
                    //     dbg!(stats);
                    // }));
                    t.set_geometry(
                        &sc,
                        &Rect {
                            left: 0,
                            top: 0,
                            right: og_window.width(),
                            bottom: og_window.height(),
                        },
                        &Rect {
                            left: i as i32 * 10,
                            top: 0,
                            right: og_window.width() / 2,
                            bottom: og_window.height(),
                        },
                        NativeWindowTransform::empty(),
                    );
                    t.apply();
                }
                drop(t);
            });
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

    let gl_window: Window =
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
        unsafe { env.get_rust_field::<_, _, Window>(native_surface_wrapper, "mNative") }.unwrap();
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

    let gl_window: Window =
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

    let gl_window =
        unsafe { env.get_rust_field::<_, _, Window>(native_surface_texture_wrapper, "mNative") }
            .unwrap();
    debug!("Java Surface is {gl_window:?}");

    let mut native_gl =
        unsafe { env2.get_rust_field::<_, _, NativeGL>(native_gl, "mNative") }.unwrap();

    native_gl.render_to_gl_window(&gl_window)
}
