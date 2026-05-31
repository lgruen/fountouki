#![allow(dead_code)]
//! fountouki — preschool learning games, rendered entirely by macroquad so the
//! pixels are identical on iOS, Android, desktop and WASM. This is the rewrite
//! entrypoint; `--capture` renders a scene offscreen to a PNG (the golden path).
use macroquad::prelude::*;

mod anim;
mod draw;
mod input;
mod layout;
mod palette;
mod scene;
mod sound;
mod text;

use text::Fonts;

fn window_conf() -> Conf {
    Conf {
        window_title: "fountouki".to_string(),
        window_width: 720,
        window_height: 500,
        high_dpi: false,
        ..Default::default()
    }
}

fn capture_camera(rt: &RenderTarget, w: f32, h: f32) -> Camera2D {
    Camera2D {
        zoom: vec2(2.0 / w, 2.0 / h),
        target: vec2(w / 2.0, h / 2.0),
        render_target: Some(rt.clone()),
        ..Default::default()
    }
}

/// Read an offscreen render target back to CPU and save a PNG. Render targets
/// are bottom-up, so flip rows to match the top-down PNG convention.
fn save_capture(rt: &RenderTarget, w: u32, h: u32, path: &str) {
    let img = rt.texture.get_texture_data();
    let mut flipped = img.clone();
    let stride = (w * 4) as usize;
    for y in 0..h as usize {
        let src = &img.bytes[y * stride..(y + 1) * stride];
        let dy = h as usize - 1 - y;
        flipped.bytes[dy * stride..(dy + 1) * stride].copy_from_slice(src);
    }
    flipped.export_png(path);
}

#[macroquad::main(window_conf)]
async fn main() {
    let fonts = Fonts::load();
    let args: Vec<String> = std::env::args().collect();

    if args.get(1).map(|s| s == "--capture").unwrap_or(false) {
        let path = args
            .get(2)
            .cloned()
            .unwrap_or_else(|| "/Users/leo/fountouki/app/out.png".to_string());
        let (w, h) = (1194u32, 834u32); // iPad Pro 11 landscape CSS px
        let rt = render_target(w, h);
        rt.texture.set_filter(FilterMode::Linear);
        let cam = capture_camera(&rt, w as f32, h as f32);
        set_camera(&cam);
        clear_background(palette::BG);
        draw::phonics_card_preview(&fonts, w as f32, h as f32);
        set_default_camera();
        clear_background(palette::BG);
        next_frame().await;
        save_capture(&rt, w, h, &path);
        println!("CAPTURE_OK {}", path);
        std::process::exit(0);
    }

    loop {
        clear_background(palette::BG);
        draw::phonics_card_preview(&fonts, screen_width(), screen_height());
        if is_key_pressed(KeyCode::Escape) {
            std::process::exit(0);
        }
        next_frame().await;
    }
}
