use jni::{
    objects::{JClass, JObject},
    JNIEnv,
};
use log::{debug, Level};
use ndk::native_window::NativeWindow;

mod support;

fn render_to_native_window(window: NativeWindow) {
    debug!("{:?}", window);

    let context = glutin::ContextBuilder::new()
        .build_windowed(&window)
        .unwrap();

    let context = unsafe { context.make_current() }.unwrap();

    let gl = support::load(&context);

    gl.draw_frame([1.0, 0.5, 0.7, 1.0]);

    context.swap_buffers().unwrap();
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_init(
    _env: JNIEnv,
    _class: JClass,
) {
    android_logger::init_once(android_logger::Config::default().with_min_level(Level::Trace));
}

#[no_mangle]
pub extern "system" fn Java_rust_androidnativesurface_MainActivity_00024Companion_renderToSurface(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
) {
    debug!("Java Surface: {:?}", surface);

    let window =
        unsafe { NativeWindow::from_surface(env.get_native_interface(), surface.into_inner()) }
            .unwrap();

    render_to_native_window(window)
}
