use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    thread,
};

use glutin::{
    context::{ContextApi, ContextAttributesBuilder},
    prelude::*,
};
use jni::{
    objects::{JClass, JObject},
    JNIEnv,
};
use log::{debug, info, LevelFilter};
use ndk::{native_window::NativeWindow, surface_texture::SurfaceTexture};
use raw_window_handle::{AndroidDisplayHandle, HasRawWindowHandle, RawDisplayHandle};

mod support;

fn render_to_native_window(window: NativeWindow) {
    dbg!(&window);
    // TODO: EGL can update the format of the window by choosing a different format,
    // but not if this producer (Surface/NativeWindow) comes from an ImageReader.
    let format = dbg!(window.format());

    // TODO: NDK should implement this!
    // let raw_display_handle = window.raw_display_handle();
    let raw_display_handle = RawDisplayHandle::Android(AndroidDisplayHandle::empty());
    let raw_window_handle = window.raw_window_handle();

    let gl_display = support::create_display(raw_display_handle);

    let template = support::config_template(raw_window_handle, format);
    let config = unsafe {
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
        config.num_samples(),
        config.color_buffer_type(),
        config.alpha_size()
    );

    // Create a wrapper for GL window and surface.
    let gl_window = support::GlWindow::from_existing(&gl_display, window, &config);

    // The context creation part. It can be created before surface and that's how
    // it's expected in multithreaded + multiwindow operation mode, since you
    // can send NotCurrentContext, but not Surface.
    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

    // Since glutin by default tries to create OpenGL core context, which may not be
    // present we should try gles.
    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));
    let gl_context = unsafe {
        gl_display
            .create_context(&config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_display
                    .create_context(&config, &fallback_context_attributes)
                    .expect("failed to create context")
            })
    };

    // Make it current and load symbols.
    let gl_context = gl_context.make_current(&gl_window.surface).unwrap();

    let renderer = support::Renderer::new(&gl_display);
    renderer.resize(gl_window.window.width(), gl_window.window.height());

    renderer.draw();

    gl_window
        .surface
        .swap_buffers(&gl_context)
        .expect("Cannot swap buffers");

    drop(renderer);

    gl_context
        .make_not_current()
        .expect("Cannot uncurrent GL context");
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_init(
    _env: JNIEnv,
    _class: JClass,
) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));

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
                info!(target: "RustStdoutStderr", "{}", buffer);
            }
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_renderToSurface(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
) {
    debug!("Java Surface: {:?}", surface);

    let window =
        unsafe { NativeWindow::from_surface(env.get_native_interface(), surface.into_raw()) }
            .unwrap();

    render_to_native_window(window)
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_renderToSurfaceTexture(
    env: JNIEnv,
    _class: JClass,
    surface_texture: JObject,
) {
    debug!("Java SurfaceTexture: {:?}", surface_texture);

    let surface_texture = unsafe {
        SurfaceTexture::from_surface_texture(env.get_native_interface(), surface_texture.into_raw())
            .unwrap()
    };

    let window = surface_texture.acquire_native_window().unwrap();

    render_to_native_window(window)
}
