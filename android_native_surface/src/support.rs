//! Support module for the glutin examples.
//!
//! Copy-paste from https://github.com/rust-windowing/glutin/blob/master/glutin_examples/examples/support/mod.rs,
//! with `winit` support stripped out

use std::{
    ffi::{CStr, CString},
    num::NonZeroU32,
};

use glutin::{
    config::{ColorBufferType, Config, ConfigSurfaceTypes, ConfigTemplate, ConfigTemplateBuilder},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface, SurfaceAttributes, SurfaceAttributesBuilder, WindowSurface},
};
use ndk::{hardware_buffer_format::HardwareBufferFormat, native_window::NativeWindow};
use raw_window_handle::{DisplayHandle, HasWindowHandle as _};

pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

    pub use Gles2 as Gl;
}

/// Structure to hold winit window and gl surface.
#[derive(Debug)]
pub struct GlWindow {
    pub surface: Surface<WindowSurface>,
    pub window: NativeWindow,
}

impl GlWindow {
    pub fn from_existing(display: &Display, window: NativeWindow, config: &Config) -> Self {
        let attrs = surface_attributes(&window);
        let surface = unsafe { display.create_window_surface(config, &attrs).unwrap() };
        Self { window, surface }
    }
}

/// Create template to find OpenGL config, which is compatible with the given Android [`HardwareBufferFormat`]
pub fn config_template(format: HardwareBufferFormat) -> ConfigTemplate {
    // The default is RGBA8
    let builder = ConfigTemplateBuilder::new().with_surface_type(ConfigSurfaceTypes::WINDOW);

    let builder = match format {
        HardwareBufferFormat::R8G8B8A8_UNORM => builder,
        HardwareBufferFormat::R8G8B8X8_UNORM => builder,
        // TODO: 0 seems to behave like DONT_CARE (-1)
        HardwareBufferFormat::R8G8B8_UNORM => builder.with_alpha_size(0),
        HardwareBufferFormat::R5G6B5_UNORM => builder
            .with_buffer_type(ColorBufferType::Rgb {
                // TODO: EGL enumerates all config formates even if 565 is requested.
                // You will have to filder on this when enumerating configs.
                r_size: 5,
                g_size: 6,
                b_size: 5,
            })
            .with_alpha_size(0),
        HardwareBufferFormat::R16G16B16A16_FLOAT => builder
            .with_buffer_type(ColorBufferType::Rgb {
                r_size: 16,
                g_size: 16,
                b_size: 16,
            })
            .with_alpha_size(16)
            .with_float_pixels(true),
        HardwareBufferFormat::R10G10B10A2_UNORM => builder
            .with_buffer_type(ColorBufferType::Rgb {
                r_size: 10,
                g_size: 10,
                b_size: 10,
            })
            .with_alpha_size(2),
        HardwareBufferFormat::BLOB => todo!(),
        // TODO: Unset RGBA for all depth/stencil formats
        HardwareBufferFormat::D16_UNORM => builder.with_depth_size(16),
        HardwareBufferFormat::D24_UNORM => builder.with_depth_size(24),
        HardwareBufferFormat::D24_UNORM_S8_UINT => builder.with_depth_size(24).with_stencil_size(8),
        HardwareBufferFormat::D32_FLOAT => builder.with_depth_size(32).with_float_pixels(true),
        HardwareBufferFormat::D32_FLOAT_S8_UINT => builder
            .with_depth_size(32)
            .with_stencil_size(8)
            .with_float_pixels(true),
        HardwareBufferFormat::S8_UINT => builder.with_stencil_size(8),
        HardwareBufferFormat::Y8Cb8Cr8_420 => todo!(),
        HardwareBufferFormat::YCbCr_P010 => todo!(),
        HardwareBufferFormat::R8_UNORM => builder
            .with_buffer_type(ColorBufferType::Rgb {
                r_size: 8,
                g_size: 0,
                b_size: 0,
            })
            .with_alpha_size(0),
        x => todo!("{x:?}"),
    };
    builder.build()
}

/// Create surface attributes for window surface.
pub fn surface_attributes(window: &NativeWindow) -> SurfaceAttributes<WindowSurface> {
    let window_handle = window.window_handle().unwrap();
    SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window_handle.as_raw(),
        NonZeroU32::new(window.width().try_into().unwrap()).unwrap(),
        NonZeroU32::new(window.height().try_into().unwrap()).unwrap(),
    )
}

/// Create the display.
pub fn create_display(display: DisplayHandle<'_>) -> Display {
    let preference = DisplayApiPreference::Egl;

    // Create connection to underlying OpenGL client Api.
    unsafe { Display::new(display.as_raw(), preference).unwrap() }
}

pub struct Renderer {
    program: gl::types::GLuint,
    vao: gl::types::GLuint,
    vbo: gl::types::GLuint,
    gl: gl::Gl,
}

impl Renderer {
    // TODO: Api-wise this should take a CurrentContext on which we call .display()
    pub fn new(gl_display: &Display) -> Self {
        unsafe {
            let gl = gl::Gl::load_with(|symbol| {
                let symbol = CString::new(symbol).unwrap();
                gl_display.get_proc_address(symbol.as_c_str()).cast()
            });

            if let Some(renderer) = get_gl_string(&gl, gl::RENDERER) {
                println!("Running on {}", renderer.to_string_lossy());
            }
            if let Some(version) = get_gl_string(&gl, gl::VERSION) {
                println!("OpenGL Version {}", version.to_string_lossy());
            }

            if let Some(shaders_version) = get_gl_string(&gl, gl::SHADING_LANGUAGE_VERSION) {
                println!("Shaders version on {}", shaders_version.to_string_lossy());
            }

            let vertex_shader = create_shader(&gl, gl::VERTEX_SHADER, VERTEX_SHADER_SOURCE);
            let fragment_shader = create_shader(&gl, gl::FRAGMENT_SHADER, FRAGMENT_SHADER_SOURCE);

            let program = gl.CreateProgram();

            gl.AttachShader(program, vertex_shader);
            gl.AttachShader(program, fragment_shader);

            gl.LinkProgram(program);

            gl.UseProgram(program);

            gl.DeleteShader(vertex_shader);
            gl.DeleteShader(fragment_shader);

            let mut vao = std::mem::zeroed();
            gl.GenVertexArrays(1, &mut vao);
            gl.BindVertexArray(vao);

            let mut vbo = std::mem::zeroed();
            gl.GenBuffers(1, &mut vbo);
            gl.BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl.BufferData(
                gl::ARRAY_BUFFER,
                (VERTEX_DATA.len() * std::mem::size_of::<f32>()) as gl::types::GLsizeiptr,
                VERTEX_DATA.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            let pos_attrib = gl.GetAttribLocation(program, b"position\0".as_ptr() as *const _);
            let color_attrib = gl.GetAttribLocation(program, b"color\0".as_ptr() as *const _);
            gl.VertexAttribPointer(
                pos_attrib as gl::types::GLuint,
                2,
                gl::FLOAT,
                0,
                5 * std::mem::size_of::<f32>() as gl::types::GLsizei,
                std::ptr::null(),
            );
            gl.VertexAttribPointer(
                color_attrib as gl::types::GLuint,
                3,
                gl::FLOAT,
                0,
                5 * std::mem::size_of::<f32>() as gl::types::GLsizei,
                (2 * std::mem::size_of::<f32>()) as *const () as *const _,
            );
            gl.EnableVertexAttribArray(pos_attrib as gl::types::GLuint);
            gl.EnableVertexAttribArray(color_attrib as gl::types::GLuint);

            Self {
                program,
                vao,
                vbo,
                gl,
            }
        }
    }

    pub fn draw(&self) {
        unsafe {
            self.gl.UseProgram(self.program);

            self.gl.BindVertexArray(self.vao);
            self.gl.BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            self.gl.ClearColor(0.1, 0.1, 0.1, 0.9);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);
            self.gl.DrawArrays(gl::TRIANGLES, 0, 3);
        }
    }

    pub fn resize(&self, width: i32, height: i32) {
        unsafe {
            self.gl.Viewport(0, 0, width, height);
        }
    }
}

impl Drop for Renderer {
    // TODO: Note that this needs a "current" context with a surface!
    fn drop(&mut self) {
        unsafe {
            self.gl.DeleteProgram(self.program);
            self.gl.DeleteBuffers(1, &self.vbo);
            self.gl.DeleteVertexArrays(1, &self.vao);
        }
    }
}

unsafe fn create_shader(
    gl: &gl::Gl,
    shader: gl::types::GLenum,
    source: &[u8],
) -> gl::types::GLuint {
    let shader = gl.CreateShader(shader);
    gl.ShaderSource(
        shader,
        1,
        [source.as_ptr().cast()].as_ptr(),
        std::ptr::null(),
    );
    gl.CompileShader(shader);
    shader
}

fn get_gl_string(gl: &gl::Gl, variant: gl::types::GLenum) -> Option<&'static CStr> {
    unsafe {
        let s = gl.GetString(variant);
        (!s.is_null()).then(|| CStr::from_ptr(s.cast()))
    }
}

#[rustfmt::skip]
static VERTEX_DATA: [f32; 15] = [
    -0.5, -0.5,  1.0,  0.0,  0.0,
     0.0,  0.5,  0.0,  1.0,  0.0,
     0.5, -0.5,  0.0,  0.0,  1.0,
];

const VERTEX_SHADER_SOURCE: &[u8] = b"
#version 100
precision mediump float;

attribute vec2 position;
attribute vec3 color;

varying vec3 v_color;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    v_color = color;
}
\0";

const FRAGMENT_SHADER_SOURCE: &[u8] = b"
#version 100
precision mediump float;

varying vec3 v_color;

void main() {
    gl_FragColor = vec4(v_color, 1.0);
}
\0";
