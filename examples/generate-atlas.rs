/* Any copyright is dedicated to the Public Domain.
 * http://creativecommons.org/publicdomain/zero/1.0/ */

extern crate compute_shader;
extern crate euclid;
extern crate gl;
extern crate glfw;
extern crate lord_drawquaad;
extern crate memmap;
extern crate pathfinder;

use compute_shader::buffer;
use compute_shader::image::{ExternalImage, Format};
use compute_shader::instance::Instance;
use euclid::{Point2D, Rect, Size2D};
use gl::types::{GLint, GLuint};
use glfw::{Action, Context, Key, OpenGlProfileHint, WindowEvent, WindowHint, WindowMode};
use memmap::{Mmap, Protection};
use pathfinder::atlas::AtlasBuilder;
use pathfinder::charmap::CodepointRange;
use pathfinder::coverage::CoverageBuffer;
use pathfinder::outline::OutlineBuilder;
use pathfinder::otf::Font;
use pathfinder::rasterizer::{Rasterizer, RasterizerOptions};
use std::env;
use std::os::raw::c_void;

const DEFAULT_POINT_SIZE: f32 = 24.0;
const WIDTH: u32 = 512;
const HEIGHT: u32 = 384;

fn main() {
    let mut glfw = glfw::init(glfw::LOG_ERRORS).unwrap();
    glfw.window_hint(WindowHint::ContextVersion(3, 3));
    glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
    glfw.window_hint(WindowHint::OpenGlProfile(OpenGlProfileHint::Core));
    let context = glfw.create_window(WIDTH, HEIGHT, "generate-atlas", WindowMode::Windowed);

    let (mut window, events) = context.expect("Couldn't create a window!");
    window.make_current();
    gl::load_with(|symbol| window.get_proc_address(symbol) as *const c_void);
    let (device_pixel_width, device_pixel_height) = window.get_framebuffer_size();

    let instance = Instance::new().unwrap();
    let device = instance.open_device().unwrap();
    let queue = device.create_queue().unwrap();

    let rasterizer_options = RasterizerOptions::from_env().unwrap();
    let rasterizer = Rasterizer::new(&instance, device, queue, rasterizer_options).unwrap();

    let file = Mmap::open_path(env::args().nth(1).unwrap(), Protection::Read).unwrap();

    let point_size = match env::args().nth(2) {
        None => DEFAULT_POINT_SIZE,
        Some(point_size) => point_size.parse().unwrap()
    };

    // FIXME(pcwalton)
    let shelf_height = (point_size * 2.0).ceil() as u32;

    let mut outline_builder = OutlineBuilder::new();
    let mut atlas_builder = AtlasBuilder::new(device_pixel_width as GLuint, shelf_height);

    unsafe {
        let font = Font::new(file.as_slice()).unwrap();
        let codepoint_ranges = [CodepointRange::new(' ' as u32, '~' as u32)];

        let glyph_ranges = font.glyph_ranges_for_codepoint_ranges(&codepoint_ranges).unwrap();
        for (glyph_index, glyph_id) in glyph_ranges.iter().enumerate() {
            outline_builder.add_glyph(&font, glyph_id).unwrap();
            atlas_builder.pack_glyph(&outline_builder, glyph_index as u32, point_size).unwrap()
        }
    }

    let outline_buffers = outline_builder.create_buffers().unwrap();
    let atlas = atlas_builder.create_atlas(&outline_builder).unwrap();

    let atlas_size = Size2D::new(device_pixel_width as GLuint, device_pixel_height as GLuint);
    let coverage_buffer = CoverageBuffer::new(&rasterizer.device, &atlas_size).unwrap();

    let image = rasterizer.device
                          .create_image(Format::R8, buffer::Protection::WriteOnly, &atlas_size)
                          .unwrap();

    let rect = Rect::new(Point2D::new(0, 0), atlas_size);

    rasterizer.draw_atlas(&image, &rect, &atlas, &outline_buffers, &coverage_buffer).unwrap();
    rasterizer.queue.flush().unwrap();

    let draw_context = lord_drawquaad::Context::new();

    let mut gl_texture = 0;
    unsafe {
        gl::GenTextures(1, &mut gl_texture);
        image.bind_to(&ExternalImage::GlTexture(gl_texture)).unwrap();

        gl::BindTexture(gl::TEXTURE_RECTANGLE, gl_texture);
        gl::TexParameteri(gl::TEXTURE_RECTANGLE, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
        gl::TexParameteri(gl::TEXTURE_RECTANGLE, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
        gl::TexParameteri(gl::TEXTURE_RECTANGLE, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as GLint);
        gl::TexParameteri(gl::TEXTURE_RECTANGLE, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as GLint);

        gl::Viewport(0, 0, device_pixel_width, device_pixel_height);
        gl::ClearColor(1.0, 1.0, 1.0, 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT);
    }

    draw_context.draw(gl_texture);
    window.swap_buffers();

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    window.set_should_close(true)
                }
                _ => {}
            }
        }
    }
}

