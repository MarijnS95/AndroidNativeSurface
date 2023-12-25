use std::{
    ffi::{CStr, CString},
    fs::File,
    io::{self, BufRead, BufReader},
    thread,
    time::{Duration, Instant},
};

use glutin::{
    context::{ContextApi, ContextAttributesBuilder},
    prelude::*,
    surface::Rect,
};
use jni::{
    objects::{JClass, JObject},
    JNIEnv,
};
use log::{debug, info, LevelFilter};
use ndk::{
    data_space::DataSpace,
    hardware_buffer::HardwareBufferUsage,
    hardware_buffer_format::HardwareBufferFormat,
    media::image_reader::ImageReader,
    native_window::{NativeWindow, NativeWindowTransform},
    surface_control::{SurfaceControl, SurfaceTransaction},
    surface_texture::SurfaceTexture,
};
use raw_window_handle::{AndroidDisplayHandle, HasRawWindowHandle, RawDisplayHandle};

mod support;

fn render_to_native_window(og_window: NativeWindow) {
    dbg!(SurfaceControl::create_from_window(
        &og_window,
        CStr::from_bytes_with_nul(b"OG\0").unwrap(),
    ));
    dbg!(&og_window);
    // TODO: EGL can update the format of the window by choosing a different format,
    // but not if this producer (Surface/NativeWindow) comes from an ImageReader.
    dbg!(og_window.format());
    let format = HardwareBufferFormat::R10G10B10A2_UNORM;

    let sc = SurfaceControl::create_from_window(
        &og_window,
        CStr::from_bytes_with_nul(b"foo\0").unwrap(),
    );
    dbg!(&sc);
    let Some(sc) = sc else {
        return;
    };

    // TODO: Might have to wait until https://android.googlesource.com/platform/frameworks/av/+/master/media/ndk/NdkImageReader.cpp#743 AImageReader_newWithDataSpace() lands
    let i = ImageReader::new_with_data_space(
        og_window.width(),
        og_window.height(),
        HardwareBufferUsage::GPU_FRAMEBUFFER | HardwareBufferUsage::GPU_SAMPLED_IMAGE,
        // This is the format that we force EGL to select... Why does EGL not filter it on the config that we give it?
        4,
        format,
        DataSpace::Unknown,
    )
    .unwrap();
    let window = i.window().unwrap();
    {
        dbg!(SurfaceControl::create_from_window(
            &window,
            CStr::from_bytes_with_nul(b"foo\0").unwrap(),
        ));
    }

    // TODO: NDK should implement this!
    // let raw_display_handle = window.raw_display_handle();
    let raw_display_handle = RawDisplayHandle::Android(AndroidDisplayHandle::empty());
    let raw_window_handle = window.raw_window_handle();

    let gl_display = support::create_display(raw_display_handle);

    // Give the format to EGL otherwise it will return all configs, including formats that are not
    // supported by the fixed format of this ImageReader.
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

    // dbg!(i.acquire_next_image());
    // dbg!(unsafe { i.acquire_next_image_async() });

    let draw = Instant::now();
    renderer.draw();
    dbg!(draw.elapsed());

    let swap = Instant::now();
    gl_window
        .surface
        .swap_buffers(&gl_context)
        .expect("Cannot swap buffers");
    dbg!(swap.elapsed());

    // A buffer only becomes available after swapping
    let mut t = SurfaceTransaction::new().unwrap();
    // t.set_on_commit(Box::new(|stats| {
    //     dbg!(stats);
    // }));
    t.set_on_complete(Box::new(|stats| {
        dbg!(stats);
        // TODO: Implement IntoIterator for non-borrowed type
        for sc in &*stats.surface_controls() {
            dbg!(sc);
        }
    }));
    // t.set_visibility(&sc, ndk::surface_control::Visibility::Hide);
    let acquire = Instant::now();
    // let img = i.acquire_next_image().unwrap();
    // let fence = None;
    let (img, fence) = unsafe { i.acquire_next_image_async() }.unwrap();
    dbg!(acquire.elapsed());
    // let img = img.unwrap();
    dbg!(&img);
    dbg!(&fence);
    // t.set_buffer(&sc, &img.hardware_buffer().unwrap(), fence);
    let hw = img.hardware_buffer().unwrap();
    t.set_buffer(&sc, &hw, fence);

    let surfaces = (0..5)
        .map(|x| {
            SurfaceControl::create(&sc, &CString::new(format!("Nested {x}")).unwrap()).unwrap()
        })
        .collect::<Vec<_>>();

    for nested in &surfaces {
        t.set_buffer(nested, &hw, None);
        t.set_color(nested, 1.0, 1.0, 1.0, 1.0, DataSpace::Unknown);
    }
    t.apply();

    std::thread::spawn(move || {
        for dt in 0..10_000 {
            let crop = ndk::native_window::Rect {
                // left: 0,
                // top: 0,
                // right: 512 - dt / 10,
                // bottom: 512 - dt / 10,
                // left: dt,
                // top: dt,
                // right: 10000,
                // bottom: 10000,
                // left,
                // top,
                // right: left + dt,
                // bottom: top + dt,
                left: 0,
                top: 0,
                right: og_window.width() - dt,
                bottom: og_window.height() - dt,
            };
            for (i, nested) in surfaces.iter().enumerate() {
                let i = i as i32;
                let left = og_window.width() / 5 * i;
                let top = i * 100;
                if crop.right > crop.left && crop.bottom > crop.top {
                    t.set_crop(nested, &crop);
                }
                t.set_position(nested, left, top);
                t.set_scale(
                    nested,
                    512.0 / (og_window.width() as f32),
                    512.0 / (og_window.height() as f32),
                );
                // t.set_buffer_transform(nested, NativeWindowTransform::ROTATE_270);
                t.set_buffer_alpha(nested, i as f32 / 10.0 + 0.5);
                // t.set_visibility(nested, ndk::surface_control::Visibility::Show);
                t.apply();
            }

            std::thread::sleep(Duration::from_millis(1));
        }
    });
    // if let Some(fence) = fence {
    //     img.delete_async(fence);
    // } else {
    //     drop(img);
    // }

    let drop_ = Instant::now();
    drop(renderer);
    dbg!(drop_.elapsed());

    let not_current = Instant::now();
    gl_context
        .make_not_current()
        .expect("Cannot uncurrent GL context");
    dbg!(not_current.elapsed());
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
