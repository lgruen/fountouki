#![allow(dead_code)]
//! fountouki — preschool learning games, rendered entirely by macroquad so the
//! pixels are identical on iOS, Android, desktop and WASM. `--capture <path>`
//! renders a scene offscreen to a PNG (the golden path); otherwise it runs the
//! interactive app loop.
use macroquad::prelude::*;

mod anim;
mod draw;
mod games;
mod input;
mod layout;
mod palette;
mod parent;
mod scene;
mod sound;
mod store;
mod text;

use games::patterns::PatternsScene;
use games::phonics::PhonicsScene;
use games::picker::PickerScene;
use parent::{ParentPanel, PanelResult};
use input::Pointer;
use layout::{Frame, Insets};
use scene::{Ctx, Nav, Scene};
use sound::Audio;
use store::Db;
use text::Fonts;

fn window_conf() -> Conf {
    Conf {
        window_title: "fountouki".to_string(),
        window_width: 1024,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

fn now_ms() -> i64 {
    (macroquad::miniquad::date::now() * 1000.0) as i64
}

/// Build a synthetic "tap released here" pointer for scripted play-tests.
fn tap(pos: Vec2) -> Pointer {
    let mut p = Pointer::default();
    p.pos = pos;
    p.just_released = true;
    p.press_pos = pos;
    p
}

/// The game registry: route id → a fresh scene. Adding a game = one arm here
/// plus an entry in `games::picker::GAMES`.
fn build_game(id: &str, db: &Db, now: i64) -> Box<dyn Scene> {
    match id {
        "patterns" => Box::new(PatternsScene::new(db.clone(), now as u32 ^ 0x1234_5678, now)),
        _ => Box::new(PhonicsScene::new(db.clone(), now as u32 ^ 0x5bd1_e995, now)),
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

/// Read an offscreen render target back to CPU and save a PNG (rows flipped to
/// the top-down PNG convention).
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
        // iPad Pro 11 landscape CSS px by default; overridable for the golden
        // matrix (--capture <path> <scene> [w] [h]).
        let w = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1194u32);
        let h = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(834u32);
        let audio = Audio::silent();
        let db = Db::mem();
        let now = 1_717_000_000_000i64;
        let which = args.get(3).map(|s| s.as_str()).unwrap_or("phonics");
        let mut scene: Box<dyn Scene> = match which {
            "patterns" => {
                {
                    let mut kv = db.borrow_kv_mut();
                    let mut ps = fountouki_core::settings::PatternsSettings::default();
                    ps.theme_choice = "shapes".to_string();
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                let mut sc = PatternsScene::new(db.clone(), 7, now);
                sc.level = 3;
                sc.stars = 7;
                Box::new(sc)
            }
            "picker" => Box::new(PickerScene::new()),
            _ => {
                let mut sc = PhonicsScene::new(db.clone(), 7, now);
                sc.stars = 3; // mid-session for a representative shot
                Box::new(sc)
            }
        };
        let mut panel_opt: Option<ParentPanel> = match which {
            "parent-patterns" => Some(ParentPanel::open(db.clone(), "patterns", now, 99)),
            "parent-phonics" => Some(ParentPanel::open(db.clone(), "phonics", now, 99)),
            _ => None,
        };

        let rt = render_target(w, h);
        rt.texture.set_filter(FilterMode::Linear);
        let cam = capture_camera(&rt, w as f32, h as f32);
        let ptr = Pointer::default();
        set_camera(&cam);
        let ctx = Ctx {
            dt: 0.016,
            time: 0.4,
            now,
            pointer: &ptr,
            frame: Frame::new(w as f32, h as f32, Insets::default()),
            fonts: &fonts,
            audio: &audio,
        };
        scene.draw(&ctx);
        if let Some(p) = panel_opt.as_mut() {
            p.draw(&ctx);
        }
        set_default_camera();
        clear_background(palette::BG);
        next_frame().await;
        save_capture(&rt, w, h, &path);
        println!("CAPTURE_OK {path}");
        std::process::exit(0);
    }

    // Scripted play-tests: drive the real scenes with synthetic taps and assert
    // gameplay invariants. No rendering needed; exits non-zero on any failure.
    if args.get(1).map(|s| s == "--playtest").unwrap_or(false) {
        let audio = Audio::silent();
        let frame = Frame::new(1194.0, 834.0, Insets::default());
        let now = 1_717_000_000_000i64;
        let mut fails = 0;

        // phonics: 7 "got it" taps complete the rainbow.
        {
            let mut sc = PhonicsScene::new(Db::mem(), 7, now);
            for _ in 0..7 {
                let ptr = tap(sc.got_center(&frame));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            if sc.stars == 7 && sc.is_done() {
                println!("PASS phonics-session");
            } else {
                println!("FAIL phonics-session (stars={}, done={})", sc.stars, sc.is_done());
                fails += 1;
            }
        }
        // patterns: the correct choice scores a star.
        {
            let mut sc = PatternsScene::new(Db::mem(), 7, now);
            let ci = sc.correct_index();
            let s0 = sc.stars;
            let ptr = tap(sc.choice_center(&frame, ci));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if sc.stars == s0 + 1 {
                println!("PASS patterns-correct");
            } else {
                println!("FAIL patterns-correct (stars {}->{})", s0, sc.stars);
                fails += 1;
            }
        }
        // patterns: a wrong choice does NOT score (errorless).
        {
            let mut sc = PatternsScene::new(Db::mem(), 13, now);
            let ci = sc.correct_index();
            let n = sc.round().choices.len();
            let wrong = (ci + 1) % n;
            let s0 = sc.stars;
            let ptr = tap(sc.choice_center(&frame, wrong));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if wrong != ci && sc.stars == s0 {
                println!("PASS patterns-errorless");
            } else {
                println!("FAIL patterns-errorless (wrong={}, correct={}, stars {}->{})", wrong, ci, s0, sc.stars);
                fails += 1;
            }
        }
        println!("PLAYTEST done, {fails} failure(s)");
        std::process::exit(if fails == 0 { 0 } else { 1 });
    }

    // Interactive.
    let db = Db::mem();
    let muted = {
        let kv = db.borrow_kv();
        fountouki_core::settings::load_shared(&**kv).muted
    };
    let audio = Audio::load(muted).await;
    let mut scene: Box<dyn Scene> = Box::new(PickerScene::new());
    let mut current_game: Option<String> = None;
    let mut panel: Option<ParentPanel> = None;
    let mut ptr = Pointer::default();

    loop {
        let dt = get_frame_time();
        ptr = Pointer::poll(&ptr, dt);
        let frame = Frame::new(screen_width(), screen_height(), Insets::default());
        let ctx = Ctx {
            dt,
            time: get_time() as f32,
            now: now_ms(),
            pointer: &ptr,
            frame,
            fonts: &fonts,
            audio: &audio,
        };

        let mut close_rebuild: Option<bool> = None;
        if let Some(p) = panel.as_mut() {
            scene.draw(&ctx); // frozen scene as backdrop
            let res = p.update(&ctx);
            p.draw(&ctx);
            if let PanelResult::Close { rebuild } = res {
                close_rebuild = Some(rebuild || p.took_start_over());
            }
        } else {
            match scene.update(&ctx) {
                Nav::Home => {
                    scene = Box::new(PickerScene::new());
                    current_game = None;
                }
                Nav::Game(id) => {
                    current_game = Some(id.clone());
                    scene = build_game(&id, &db, now_ms());
                }
                Nav::OpenParent => {
                    let g = current_game.clone().unwrap_or_default();
                    panel = Some(ParentPanel::open(db.clone(), &g, now_ms(), now_ms() as u32));
                }
                Nav::Stay => {}
            }
            scene.draw(&ctx);
        }
        if let Some(rebuild) = close_rebuild {
            panel = None;
            if rebuild {
                if let Some(id) = &current_game {
                    scene = build_game(id, &db, now_ms());
                }
            }
        }

        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        next_frame().await;
    }
}
