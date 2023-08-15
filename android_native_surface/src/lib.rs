use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    os::fd::{AsFd, FromRawFd, RawFd},
    thread,
    time::Instant,
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
use ndk::{
    hardware_buffer::HardwareBufferUsage,
    media::image_reader::{ImageFormat, ImageReader},
    native_window::NativeWindow,
    surface_texture::SurfaceTexture,
    sync::SyncFileInfo,
};
use raw_window_handle::{AndroidDisplayHandle, HasRawWindowHandle, RawDisplayHandle};
use rustix::event::{PollFd, PollFlags};

mod support;

fn render_to_native_window(window: NativeWindow) {
    dbg!(&window);

    // TODO: NDK should implement this!
    // let raw_display_handle = window.raw_display_handle();
    let raw_display_handle = RawDisplayHandle::Android(AndroidDisplayHandle::empty());
    let raw_window_handle = window.raw_window_handle();

    let gl_display = support::create_display(raw_display_handle);

    let template = support::config_template(raw_window_handle);
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

    println!("Picked a config with {} samples", config.num_samples());
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

    gl_window.surface.swap_buffers(&gl_context).unwrap();
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

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_renderHardwareBuffer(
    mut env: JNIEnv,
    _class: JClass,
) {
    debug!("SurfaceControl hack");

    let cls = env
        .find_class("rust/androidnativesurface/RenderedHardwareBuffer")
        .unwrap();
    dbg!(cls);
    let image_reader = ImageReader::new(100, 100, ImageFormat::RGBA_8888, 10).expect("ImageReader");
    let window = image_reader.window().expect("NativeWindow");
    dbg!(&image_reader, &window);

    debug!(
        "Acquire before {:?}",
        image_reader.acquire_latest_image_async()
    );

    render_to_native_window(window);

    let (image, fd) = image_reader.acquire_latest_image_async().unwrap();
    dbg!(&image, &fd);

    if let Some(fd) = &fd {
        let sync_file_info = SyncFileInfo::new(fd.as_fd()).expect("SyncFileInfo");
        dbg!(&sync_file_info);
    }

    let hwbuf = image.hardware_buffer().unwrap();
    dbg!(&hwbuf);
    dbg!(hwbuf.describe());

    if let Some(fd) = &fd {
        let x = Instant::now();
        let mut pfd = PollFd::new(fd, PollFlags::all());
        rustix::event::poll(std::slice::from_mut(&mut pfd), -1).unwrap();
        debug!("Polling on fd took {:.3?}: {pfd:?}", x.elapsed());
    }
    let x = Instant::now();
    // let map = hwbuf.lock(HardwareBufferUsage::CPU_READ_OFTEN, None, None);
    let map = hwbuf.lock(HardwareBufferUsage::CPU_READ_OFTEN, fd, None);
    debug!("Locking with fd took {:.3?}", x.elapsed());
    debug!("map {:?}", map);
    debug!("unlock async {:?}", hwbuf.unlock_async());

    // let obj = env.alloc_object(cls).unwrap();
    // dbg!(obj);
    // env.new_object(cls, ctor_sig, ctor_args)
    // hwbuf.to_jni(env)
}
