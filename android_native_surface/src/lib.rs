use jni::{
    objects::{JClass, JObject},
    JNIEnv,
};
use log::{debug, Level};
use ndk::native_window::NativeWindow;

fn render_to_native_window(window: NativeWindow) {
    debug!("{:?}", window);
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
