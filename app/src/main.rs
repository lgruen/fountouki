#![allow(dead_code)]
//! fountouki — preschool learning games, rendered entirely by macroquad so the
//! pixels are identical on iOS, Android, desktop and WASM. `--capture <path>`
//! renders a scene offscreen to a PNG (the golden path); otherwise it runs the
//! interactive app loop.
use macroquad::prelude::*;

mod anim;
mod chrome;
mod confetti;
mod draw;
mod emoji;
mod games;
mod input;
mod kb;
mod layout;
mod net;
mod palette;
mod parent;
mod scene;
mod sound;
mod store;
mod text;

use games::patterns::PatternsScene;
use games::phonics::PhonicsScene;
use games::picker::PickerScene;
use games::singback::SingbackScene;
use games::tracing::TracingScene;
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

/// Build a synthetic "finger held down here" pointer (tracing drags).
fn drag(pos: Vec2) -> Pointer {
    let mut p = Pointer::default();
    p.pos = pos;
    p.down = true;
    p.press_pos = pos;
    p
}

/// The game registry: route id → a fresh scene. Adding a game = one arm here
/// plus an entry in `games::picker::GAMES`.
fn build_game(id: &str, db: &Db, now: i64) -> Box<dyn Scene> {
    match id {
        "patterns" => Box::new(PatternsScene::new(db.clone(), now as u32 ^ 0x1234_5678, now)),
        "tracing" => Box::new(TracingScene::new(db.clone(), now as u32 ^ 0x7e11_e77a, now)),
        "singback" => Box::new(SingbackScene::new(db.clone(), now as u32 ^ 0x5126_acc0, now)),
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
    emoji::init();
    text::init_ui();
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
                    let ps = fountouki_core::settings::PatternsSettings {
                        theme_choice: "shapes".to_string(),
                        ..Default::default()
                    };
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                let mut sc = PatternsScene::new(db.clone(), 7, now);
                sc.level = 3;
                sc.stars = 7;
                Box::new(sc)
            }
            "patterns-unit" => {
                {
                    let mut kv = db.borrow_kv_mut();
                    let ps = fountouki_core::settings::PatternsSettings {
                        theme_choice: "shapes".to_string(),
                        mode: "unit".to_string(),
                        ..Default::default()
                    };
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = PatternsScene::new(db.clone(), 7, now);
                sc.level = 2;
                let ulen = sc.round().unit_len.min(sc.round().visible.len());
                for i in 0..ulen {
                    let ptr = tap(sc.cell_center(&frame, i));
                    let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "patterns-levelup" => {
                // A clean streak of 4 fires the level-up drive-by; settle ~1.2 s
                // so the golden catches the mini train mid-crossing.
                {
                    let mut kv = db.borrow_kv_mut();
                    let ps = fountouki_core::settings::PatternsSettings {
                        theme_choice: "shapes".to_string(),
                        ..Default::default()
                    };
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = PatternsScene::new(db.clone(), 7, now);
                let idle = Pointer::default();
                for i in 0..4 {
                    let ptr = tap(sc.choice_center(&frame, sc.correct_index()));
                    let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                    if i < 3 {
                        let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                        sc.update(&ctx);
                    }
                }
                for _ in 0..12 {
                    let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "patterns-emoji" => {
                {
                    let mut kv = db.borrow_kv_mut();
                    let ps = fountouki_core::settings::PatternsSettings {
                        theme_choice: "emoji-animals".to_string(),
                        ..Default::default()
                    };
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                Box::new(PatternsScene::new(db.clone(), 5, now))
            }
            "patterns-hard" => {
                // Hard pins 4 choices (correct + unit-mate + pool distractors) —
                // exercises the single-row choice layout. emoji-animals has a big
                // pool so the count is always the full 4.
                {
                    let mut kv = db.borrow_kv_mut();
                    let ps = fountouki_core::settings::PatternsSettings {
                        theme_choice: "emoji-animals".to_string(),
                        difficulty: "hard".to_string(),
                        ..Default::default()
                    };
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                Box::new(PatternsScene::new(db.clone(), 5, now))
            }
            "patterns-done" => {
                // Master the final level (a clean streak of 4 at MAX_LEVEL) to
                // reach the Pattern Train finale, then settle the entrance so the
                // golden shows the train parked + celebrating at the flag.
                {
                    let mut kv = db.borrow_kv_mut();
                    let ps = fountouki_core::settings::PatternsSettings {
                        theme_choice: "shapes".to_string(),
                        ..Default::default()
                    };
                    fountouki_core::settings::save_patterns(&mut **kv, &ps);
                }
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = PatternsScene::new(db.clone(), 7, now);
                sc.level = fountouki_core::patterns::MAX_LEVEL;
                let dptr = Pointer::default();
                let mut guard = 0;
                while !sc.in_finale() && guard < 40 {
                    let ptr = tap(sc.choice_center(&frame, sc.correct_index()));
                    let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                    if sc.in_finale() {
                        break;
                    }
                    let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &dptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                    guard += 1;
                }
                for _ in 0..26 {
                    let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &dptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "phonics-miss" => {
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = PhonicsScene::new(db.clone(), 7, now);
                let ptr = tap(sc.miss_center(&frame));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                Box::new(sc)
            }
            "phonics-miss-igloo" => {
                // The 'i' miss-reveal exercises the drawn (vector) igloo.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = PhonicsScene::new(db.clone(), 7, now);
                sc.debug_set_letter('i');
                let ptr = tap(sc.miss_center(&frame));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                Box::new(sc)
            }
            "picker" => Box::new(PickerScene::new(db.clone())),
            "tracing" => {
                // Mid-trace of 'c' (pinned — the SRS queue is shuffled): the big
                // card, the high-contrast guide glyph, the kid's free-drawn ink,
                // tiny start/end dots, and the always-offered redo / ✗ / ✓ row.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('c');
                sc.skip_watch();
                for i in 0..=20 {
                    let ptr = drag(sc.stroke_point_px(&frame, 0, 0.55 * i as f32 / 20.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "tracing-watch" => {
                // The animated stroke-order demo, caught mid-stroke.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('c');
                let idle = Pointer::default();
                for _ in 0..12 {
                    let ctx = Ctx { dt: 0.075, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "tracing-two-stroke" => {
                // 'x' fully traced (both strokes' free-drawn ink) with the small
                // numbered start dots.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('x');
                sc.skip_watch();
                let idle = Pointer::default();
                for si in 0..sc.stroke_count() {
                    for i in 0..=20 {
                        let ptr = drag(sc.stroke_point_px(&frame, si, i as f32 / 20.0));
                        let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                        sc.update(&ctx);
                    }
                    // Lift between strokes so the polylines stay separate.
                    let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "tracing-grade" => {
                // A fully-traced 'c' with the always-present redo / ✗ / ✓ row.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('c');
                sc.skip_watch();
                for i in 0..=40 {
                    let ptr = drag(sc.stroke_point_px(&frame, 0, i as f32 / 40.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Lift — the ink stays up over the guide; the row is always there.
                let idle = Pointer::default();
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                Box::new(sc)
            }
            "tracing-reward" => {
                // The post-✓ install: the excavator digging the freshly earned
                // foundation trench while the next letter's card waits (its demo
                // holds off until the build finishes).
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('c');
                sc.skip_watch();
                for i in 0..=40 {
                    let ptr = drag(sc.stroke_point_px(&frame, 0, i as f32 / 40.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Lift, tap ✓, then catch the excavator mid-dig on the
                // foundation stage.
                let idle = Pointer::default();
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ptr = tap(sc.got_center(&frame));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                for _ in 0..8 {
                    let ctx = Ctx { dt: 0.15, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "tracing-build" => {
                // Mid-session: walls up on the slab, the tower crane carrying
                // the roof truss down toward them (lift slings + trolley out).
                // BUILD_STARS / BUILD_T env overrides let a dev inspect any
                // stage mid-animation; goldens use the defaults.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('d');
                let stars: u32 = std::env::var("BUILD_STARS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
                let tt: f32 = std::env::var("BUILD_T").ok().and_then(|v| v.parse().ok()).unwrap_or(1.36);
                sc.debug_set_build(stars, 0.0);
                let idle = Pointer::default();
                let steps = (tt / 0.17).ceil() as usize;
                for _ in 0..steps {
                    let ctx = Ctx { dt: tt / steps as f32, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "tracing-done" => {
                // The house-warming finale, settled (flags up, smoke going),
                // with one window lamp tapped on.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_finish_session();
                let idle = Pointer::default();
                for _ in 0..4 {
                    let ctx = Ctx { dt: 0.2, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                let ptr = tap(sc.window_center(&frame, 0));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                for _ in 0..4 {
                    let ctx = Ctx { dt: 0.15, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "tracing-housewarming" => {
                // The finale mid-play, to show off the touchables: both window
                // lamps switched on, the door ringing open, the sun bursting
                // into rays and the chimney coughing a puff.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_finish_session();
                let idle = Pointer::default();
                for _ in 0..4 {
                    let ctx = Ctx { dt: 0.2, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Flick both lamps on, then let them warm up to full glow.
                for i in 0..2 {
                    let ptr = tap(sc.window_center(&frame, i));
                    let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                for _ in 0..6 {
                    let ctx = Ctx { dt: 0.06, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Ring the door, tap the sun (rays), poke the chimney (puff).
                let ptr = tap(sc.door_center(&frame));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ptr = tap(sc.sun_center(&frame));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ptr = tap(sc.chimney_center(&frame));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                Box::new(sc)
            }
            "phonics-done" => {
                // Play 7 correct rounds to reach the rainbow-done garden scene.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = PhonicsScene::new(db.clone(), 7, now);
                let idle = Pointer::default();
                for _ in 0..7 {
                    let ptr = tap(sc.got_center(&frame));
                    let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                    // Settle past the post-star reward beat so the next ✓ lands.
                    let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                Box::new(sc)
            }
            "singback" | "singback-ready" | "singback-input" | "singback-miss"
            | "singback-reward" | "singback-finale" => {
                use games::singback::CaptureState;
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let idle = Pointer::default();
                let ctx0 = Ctx { dt: 0.016, time: 0.4, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                let cap = match which {
                    "singback-ready" => CaptureState::Ready,
                    "singback-input" => CaptureState::Input,
                    "singback-miss" => CaptureState::Miss,
                    "singback-reward" => CaptureState::Reward,
                    "singback-finale" => CaptureState::Finale,
                    _ => CaptureState::Show, // "singback"
                };
                Box::new(SingbackScene::capture(db.clone(), 99, now, cap, &ctx0))
            }
            _ => {
                let mut sc = PhonicsScene::new(db.clone(), 7, now);
                sc.stars = 3; // mid-session for a representative shot
                Box::new(sc)
            }
        };
        let mut panel_opt: Option<ParentPanel> = match which {
            "parent-patterns" => Some(ParentPanel::open(db.clone(), "patterns", now, 99)),
            "parent-phonics" => Some(ParentPanel::open(db.clone(), "phonics", now, 99)),
            "parent-tracing" => Some(ParentPanel::open(db.clone(), "tracing", now, 99)),
            "parent-singback" => Some(ParentPanel::open(db.clone(), "singback", now, 99)),
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

        // phonics: 7 "got it" taps complete the rainbow. A second ✓ tap during
        // the post-star reward beat must be ignored (settle between taps).
        {
            let mut sc = PhonicsScene::new(Db::mem(), 7, now);
            let idle = Pointer::default();
            for i in 0..7 {
                let ptr = tap(sc.got_center(&frame));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                if i == 0 {
                    // Rapid re-tap inside the beat: must not double-grade.
                    let ptr = tap(sc.got_center(&frame));
                    let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            if sc.stars == 7 && sc.is_done() {
                println!("PASS phonics-session");
            } else {
                println!("FAIL phonics-session (stars={}, done={})", sc.stars, sc.is_done());
                fails += 1;
            }
            // Done scene: tapping the frog plays a reaction (frog_taps++), and
            // the scene stays on the celebration (not navigated away).
            let before = sc.frog_taps();
            let ptr = tap(sc.frog_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if sc.is_done() && sc.frog_taps() == before + 1 {
                println!("PASS phonics-frog-react");
            } else {
                println!("FAIL phonics-frog-react (done={}, taps {}->{})", sc.is_done(), before, sc.frog_taps());
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
        // phonics: a miss reveals the exemplar (errorless hint), scores nothing,
        // and the → arrow advances back to a fresh card. Then the mute speaker
        // toggles + persists the shared mute without touching gameplay.
        {
            let db = Db::mem();
            let mut sc = PhonicsScene::new(db.clone(), 11, now);
            let ptr = tap(sc.miss_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let revealed = sc.is_miss() && sc.stars == 0;
            let ptr = tap(sc.advance_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if revealed && !sc.is_miss() && !sc.is_done() {
                println!("PASS phonics-miss-reveal");
            } else {
                println!("FAIL phonics-miss-reveal (revealed={}, miss={}, stars={})", revealed, sc.is_miss(), sc.stars);
                fails += 1;
            }
            let tb = chrome::topbar(&frame);
            let was = audio.muted();
            let ptr = tap(tb.mute.0);
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let persisted = {
                let kv = db.borrow_kv();
                fountouki_core::settings::load_shared(&**kv).muted
            };
            if audio.muted() != was && persisted == audio.muted() {
                println!("PASS chrome-mute-toggle");
            } else {
                println!("FAIL chrome-mute-toggle (muted {}->{}, persisted={})", was, audio.muted(), persisted);
                fails += 1;
            }
            audio.set_muted(was); // restore for later scenarios
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
        // patterns: a rapid second tap while the correct answer is animating out
        // is ignored — no double star, no skipped round (the advance_in lock).
        {
            let mut sc = PatternsScene::new(Db::mem(), 17, now);
            let ci = sc.correct_index();
            let pos = sc.choice_center(&frame, ci);
            for _ in 0..2 {
                let ptr = tap(pos);
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            if sc.stars == 1 {
                println!("PASS patterns-double-tap");
            } else {
                println!("FAIL patterns-double-tap (stars={})", sc.stars);
                fails += 1;
            }
        }
        // patterns unit mode: selecting a run of exactly unit_len cells and
        // submitting scores; a wrong-length run clears errorlessly (no star).
        {
            let db = Db::mem();
            {
                let mut kv = db.borrow_kv_mut();
                let ps = fountouki_core::settings::PatternsSettings {
                    theme_choice: "shapes".to_string(),
                    mode: "unit".to_string(),
                    ..Default::default()
                };
                fountouki_core::settings::save_patterns(&mut **kv, &ps);
            }
            let mut sc = PatternsScene::new(db, 7, now);
            let ulen = sc.round().unit_len;
            let nvis = sc.round().visible.len();
            // Wrong length first (when possible): select one cell, submit.
            let mut errorless_ok = true;
            if ulen > 1 {
                let ptr = tap(sc.cell_center(&frame, 0));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ptr = tap(sc.fab_center(&frame));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                errorless_ok = sc.stars == 0 && sc.unit_selection().is_none();
            }
            // Then the real thing: a contiguous run of unit_len cells + submit.
            for i in 0..ulen.min(nvis) {
                let ptr = tap(sc.cell_center(&frame, i));
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let sel_ok = sc.unit_selection() == Some((0, ulen.min(nvis)));
            let ptr = tap(sc.fab_center(&frame));
            let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if errorless_ok && sel_ok && sc.stars == 1 {
                println!("PASS patterns-unit-mode");
            } else {
                println!("FAIL patterns-unit-mode (errorless={}, sel={}, stars={})", errorless_ok, sel_ok, sc.stars);
                fails += 1;
            }
        }
        // patterns: a level only advances on a CLEAN streak of LEVEL_UP_STREAK
        // correct in a row. A wrong answer breaks the streak, so
        // mistake-then-correct must NOT bump the level (stars still climb
        // monotonically). Regression guard: the old code counted cumulative
        // correct answers, so a mistake mid-run still leveled up.
        {
            let mut sc = PatternsScene::new(Db::mem(), 21, now);
            // Tap the correct choice, then settle past the advance animation
            // (and, on a level-up, the full drive-by hold) so the next round is
            // generated.
            let play_correct = |sc: &mut PatternsScene| {
                let ci = sc.correct_index();
                let ptr = tap(sc.choice_center(&frame, ci));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let idle = Pointer::default();
                let ctx = Ctx { dt: 4.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            };
            // Tap a wrong choice, then settle past the retry delay (errorless).
            let play_wrong = |sc: &mut PatternsScene| {
                let ci = sc.correct_index();
                let n = sc.round().choices.len();
                let wrong = (ci + 1) % n;
                let ptr = tap(sc.choice_center(&frame, wrong));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let idle = Pointer::default();
                let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            };
            // Three clean correct: one short of a level-up.
            for _ in 0..3 { play_correct(&mut sc); }
            let lvl = sc.level;
            // A mistake breaks the run, so the very next correct must NOT advance.
            play_wrong(&mut sc);
            play_correct(&mut sc);
            let held = sc.level == lvl;
            // A fresh clean run of LEVEL_UP_STREAK then does advance.
            for _ in 0..4 { play_correct(&mut sc); }
            if held && sc.level == lvl + 1 && sc.stars == 8 {
                println!("PASS patterns-level-streak");
            } else {
                println!("FAIL patterns-level-streak (held={}, level {}->{}, stars={})", held, lvl, sc.level, sc.stars);
                fails += 1;
            }
        }
        // patterns: a clean streak of LEVEL_UP_STREAK fires the level-up
        // drive-by (and holds the next round); it parks again afterwards.
        {
            let mut sc = PatternsScene::new(Db::mem(), 31, now);
            let idle = Pointer::default();
            for i in 0..4 {
                let ptr = tap(sc.choice_center(&frame, sc.correct_index()));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                if i < 3 {
                    let ctx = Ctx { dt: 4.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
            }
            let fired = sc.drive_active();
            for _ in 0..5 {
                let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            if fired && !sc.drive_active() && sc.level == 2 {
                println!("PASS patterns-levelup-driveby");
            } else {
                println!("FAIL patterns-levelup-driveby (fired={}, parked={}, level={})", fired, !sc.drive_active(), sc.level);
                fails += 1;
            }
        }
        // patterns: mastering the FINAL level (a clean streak at MAX_LEVEL) fires
        // the train finale. The engine is then re-tappable (plays a reaction,
        // stays on the celebration) and Replay returns to a fresh game.
        {
            let mut sc = PatternsScene::new(Db::mem(), 7, now);
            sc.level = fountouki_core::patterns::MAX_LEVEL; // start at the top
            let idle = Pointer::default();
            let mut guard = 0;
            while !sc.in_finale() && guard < 40 {
                let ptr = tap(sc.choice_center(&frame, sc.correct_index()));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                if sc.in_finale() {
                    break;
                }
                let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                guard += 1;
            }
            if sc.in_finale() {
                println!("PASS patterns-finale-trigger");
            } else {
                println!("FAIL patterns-finale-trigger (guard={guard}, level={})", sc.level);
                fails += 1;
            }
            // Settle the entrance, then tap the engine.
            for _ in 0..20 {
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let before = sc.engine_taps();
            let ptr = tap(sc.engine_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if sc.in_finale() && sc.engine_taps() == before + 1 {
                println!("PASS patterns-engine-react");
            } else {
                println!("FAIL patterns-engine-react (finale={}, taps {}->{})", sc.in_finale(), before, sc.engine_taps());
                fails += 1;
            }
            // Replay returns to a fresh game at level 1 (stars reset).
            let ptr = tap(sc.replay_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if !sc.in_finale() && sc.level == 1 && sc.stars == 0 {
                println!("PASS patterns-replay");
            } else {
                println!("FAIL patterns-replay (finale={}, level={}, stars={})", sc.in_finale(), sc.level, sc.stars);
                fails += 1;
            }
        }
        // tracing: drag along every stroke of every session letter and grade
        // each one ✓; the session completes, stars climb monotonically, and the
        // Leitner state persists (every ✓-graded letter promoted to box >= 1).
        {
            let db = Db::mem();
            let mut sc = TracingScene::new(db.clone(), 7, now);
            let idle = Pointer::default();
            // The demo can be skipped with a tap (impatient-kid path).
            let watched = sc.in_watch();
            let ptr = tap(vec2(frame.w / 2.0, frame.h / 2.0));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let skipped = watched && sc.in_trace();

            // Free-draw / errorless: a finger off the card lays no ink.
            let ptr = drag(vec2(frame.w + 50.0, frame.h + 50.0));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let errorless = !sc.has_ink() && sc.stars == 0;

            // Free-draw every SESSION_GOAL letter (all its strokes), like a
            // finger, then the parent taps ✓ — the row is always offered.
            let goal = fountouki_core::tracing::SESSION_GOAL as u32;
            let mut graded_each_time = true;
            'session: for _ in 0..goal {
                sc.skip_watch();
                let in_trace = sc.in_trace();
                for si in 0..sc.stroke_count() {
                    for i in 0..=20 {
                        let ptr = drag(sc.stroke_point_px(&frame, si, i as f32 / 20.0));
                        let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                        sc.update(&ctx);
                    }
                    // Lift between strokes.
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // The redo / ✗ / ✓ row must be live with the kid's ink showing.
                graded_each_time &= in_trace && sc.has_ink();
                let ptr = tap(sc.got_center(&frame));
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                if sc.is_done() {
                    break 'session;
                }
            }
            // The final ✓ must NOT hard-cut to the finale: the door's install
            // (the topping-out beat) plays on-site first…
            let topping = sc.stars == goal && !sc.is_done();
            for _ in 0..4 {
                let ctx = Ctx { dt: 1.0, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            // …and the house-warming follows once the door lands.
            let persisted = {
                let kv = db.borrow_kv();
                let st = fountouki_core::tracing::load(&**kv, now);
                st.letters.values().filter(|ls| ls.box_ >= 1).count() as u32
            };
            if skipped
                && errorless
                && graded_each_time
                && topping
                && sc.is_done()
                && sc.stars == goal
                && persisted == goal
            {
                println!("PASS tracing-session");
            } else {
                println!(
                    "FAIL tracing-session (skipped={skipped}, errorless={errorless}, graded={graded_each_time}, topping={topping}, done={}, stars={}, persisted={persisted})",
                    sc.is_done(),
                    sc.stars
                );
                fails += 1;
            }
            // House-warming finale: the door rings + swings (and the scene
            // stays).
            let ptr = tap(sc.door_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let door_ok = sc.is_done() && sc.door_taps() == 1;
            // The window lamp is a switch: a tap lights it, another tap turns it
            // OFF again (no longer a one-way star).
            let ptr = tap(sc.window_center(&frame, 1));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let lit_on = sc.window_lit(1);
            let ptr = tap(sc.window_center(&frame, 1));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let lit_off = !sc.window_lit(1);
            // The sky is touchable too: tap the sun, poke the chimney.
            let ptr = tap(sc.sun_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let ptr = tap(sc.chimney_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let sky_ok = sc.sun_taps() == 1 && sc.chimney_taps() == 1;
            // A party-guest frog reacts to its tap too.
            let ptr = tap(sc.friend_center(&frame, 0));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let friend_ok = sc.friend_taps() == 1;
            let idle2 = Pointer::default();
            let ctx = Ctx { dt: 2.0, time: 0.0, now, pointer: &idle2, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if door_ok && friend_ok && lit_on && lit_off && sky_ok && sc.is_done() && !sc.window_lit(0) {
                println!("PASS tracing-housewarming");
            } else {
                println!(
                    "FAIL tracing-housewarming (door_ok={door_ok}, friend_ok={friend_ok}, lit_on={lit_on}, lit_off={lit_off}, sky_ok={sky_ok}, done={}, lit0={})",
                    sc.is_done(),
                    sc.window_lit(0)
                );
                fails += 1;
            }
            // Replay resets the session (stars back to 0, tracing resumes).
            let (replay, _home, br) = chrome::corner_buttons(&frame);
            let _ = br;
            let ptr = tap(replay);
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if !sc.is_done() && sc.stars == 0 && sc.in_watch() {
                println!("PASS tracing-replay");
            } else {
                println!("FAIL tracing-replay (done={}, stars={}, watch={})", sc.is_done(), sc.stars, sc.in_watch());
                fails += 1;
            }
        }
        // tracing: a multi-stroke letter ('i' = body + dot) free-draws and
        // grades ✓ — promoted to box >= 1, the kid gets the star.
        {
            let mut sc = TracingScene::new(Db::mem(), 7, now);
            sc.debug_set_letter('i');
            sc.skip_watch();
            let idle = Pointer::default();
            for si in 0..sc.stroke_count() {
                for i in 0..=20 {
                    let ptr = drag(sc.stroke_point_px(&frame, si, i as f32 / 20.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let drew = sc.has_ink();
            let ptr = tap(sc.got_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if drew && sc.stars == 1 && sc.letter_box('i') >= 1 {
                println!("PASS tracing-dot-letter");
            } else {
                println!(
                    "FAIL tracing-dot-letter (drew={drew}, stars={}, box={})",
                    sc.stars,
                    sc.letter_box('i')
                );
                fails += 1;
            }
        }
        // tracing: the parent's ✗ holds the letter's Leitner box down and the
        // house does NOT gain a part (only ✓ builds, like phonics' rainbow);
        // the session moves on to a different letter, no confetti.
        {
            let mut sc = TracingScene::new(Db::mem(), 7, now);
            let idle = Pointer::default();
            sc.debug_set_letter('c');
            sc.skip_watch();
            for i in 0..=20 {
                let ptr = drag(sc.stroke_point_px(&frame, 0, i as f32 / 20.0));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            // Lift; the ✗/✓ row is offered while tracing (no reward beat).
            let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let graded = sc.in_trace();
            let ptr = tap(sc.miss_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let moved_on = !sc.is_done() && sc.in_watch() && sc.current_letter() != 'c';
            if graded && sc.stars == 0 && sc.letter_box('c') == 0 && moved_on {
                println!("PASS tracing-grade-miss");
            } else {
                println!(
                    "FAIL tracing-grade-miss (graded={graded}, stars={}, box={}, letter={}, watch={})",
                    sc.stars,
                    sc.letter_box('c'),
                    sc.current_letter(),
                    sc.in_watch()
                );
                fails += 1;
            }
        }
        // singback: tapping back the WHOLE sequence completes a round — the best
        // span records, the sequence GROWS by one (Simon-style, never shortens),
        // and across rounds best_span is MONOTONIC and equals the longest round
        // completed. Play two full rounds and check the whole chain.
        {
            let mut sc = SingbackScene::new(Db::mem(), 99, now);
            let idle = Pointer::default();
            let mut ok = true;
            let mut best_prev = sc.best_span(); // starts at 0 on a fresh Db::mem()
            let mut longest = 0u32;
            // Advancing game clock: each tap is a distinct physical press well
            // past the tap-debounce window, so the debounce passes every one.
            let mut clk = 0.0f32;
            for round in 0..2u32 {
                sc.skip_to_input(); // skip the watch playback
                let seq: Vec<u8> = sc.sequence().to_vec();
                let len = seq.len();
                for &p in &seq {
                    clk += 0.3;
                    let ptr = tap(sc.pad_center(&frame, p as usize));
                    let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                let rewarded = sc.in_reward();
                let best = sc.best_span();
                longest = longest.max(len as u32);
                // Monotonic: best never drops, and equals the longest completed.
                ok &= rewarded && best >= best_prev && best == longest;
                best_prev = best;
                // Settle past the reward beat: the next round appends + replays.
                for _ in 0..40 {
                    let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Growth is monotonic up: the sequence is exactly one longer now.
                ok &= sc.sequence().len() == len + 1;
                if round == 0 && (!rewarded || best != len as u32) {
                    ok = false; // first round must set best to its own length
                }
            }
            if ok && sc.best_span() == longest {
                println!("PASS singback-round");
            } else {
                println!("FAIL singback-round (best={}, longest={longest}, len={})", sc.best_span(), sc.sequence().len());
                fails += 1;
            }
        }
        // singback: a parent "start over" resets best_span back to 0. Earn a best,
        // then apply core::singback::start_over and reload — a freshly-mounted
        // scene must read best_span 0 (the reset out-versions the earned blob).
        {
            let db = Db::mem();
            // Earn a best by completing one round.
            let mut sc = SingbackScene::new(db.clone(), 99, now);
            sc.skip_to_input();
            let seq: Vec<u8> = sc.sequence().to_vec();
            let mut clk = 0.0f32;
            for &p in &seq {
                clk += 0.3; // advance past the tap-debounce window per tap
                let ptr = tap(sc.pad_center(&frame, p as usize));
                let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let earned = sc.best_span();
            // Parent start-over: reset + persist (mirrors parent.rs's singback arm).
            {
                let mut kv = db.borrow_kv_mut();
                let cur = fountouki_core::singback::load(&**kv, now);
                fountouki_core::singback::save(&mut **kv, &fountouki_core::singback::start_over(&cur, now));
            }
            let fresh = SingbackScene::new(db.clone(), 7, now);
            if earned > 0 && fresh.best_span() == 0 {
                println!("PASS singback-start-over");
            } else {
                println!("FAIL singback-start-over (earned={earned}, after_reset={})", fresh.best_span());
                fails += 1;
            }
        }
        // singback: a wrong tap is errorless — it enters Miss (the correct pad
        // teaches), never scores, and the sequence keeps the SAME length after
        // the teaching beat replays. Then the replay button re-shows from Input.
        {
            let mut sc = SingbackScene::new(Db::mem(), 99, now);
            let idle = Pointer::default();
            sc.skip_to_input();
            let seq: Vec<u8> = sc.sequence().to_vec();
            let len0 = seq.len();
            // Advancing game clock so each distinct tap clears the debounce.
            let mut clk = 0.0f32;
            // Tap a pad that is NOT the first step.
            let wrong = (0..4u8).find(|&p| p != seq[0]).unwrap();
            clk += 0.3;
            let ptr = tap(sc.pad_center(&frame, wrong as usize));
            let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let missed = sc.in_miss();
            // Settle past the Miss beat → it replays the same sequence.
            for _ in 0..20 {
                let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let same_len = sc.sequence().len() == len0;
            let never_scored = sc.best_span() == 0;
            // Replay button: advance got>0 first, then hit replay — it must reset
            // got to 0 and re-Show WITHOUT shortening the sequence (length held).
            sc.skip_to_input();
            clk += 0.3;
            let ptr = tap(sc.pad_center(&frame, seq[0] as usize)); // one correct tap
            let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let progressed = sc.got() == 1;
            clk += 0.3;
            let ptr = tap(sc.replay_center(&frame));
            let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let replayed = !sc.in_input() && sc.got() == 0 && sc.sequence().len() == len0;
            if missed && same_len && never_scored && progressed && replayed {
                println!("PASS singback-errorless");
            } else {
                println!("FAIL singback-errorless (missed={missed}, same_len={same_len}, never_scored={never_scored}, progressed={progressed}, replayed={replayed})");
                fails += 1;
            }
        }
        // singback: the sequence NEVER starts cold — a fresh scene opens in the
        // Ready count-in lead-in, and only reaches Show after the lead-in, then
        // Input after the whole sequence plays. Advance frames and confirm the
        // phase order Ready → Show → Input.
        {
            let mut sc = SingbackScene::new(Db::mem(), 99, now);
            let idle = Pointer::default();
            let starts_ready = sc.in_ready();
            // Run a couple seconds of frames; we should pass through Show and end
            // in Input (nobody tapping). Track that Show was actually entered
            // before Input (no instant cold start into the sequence).
            let mut saw_show = false;
            let mut clk = 0.0f32;
            for _ in 0..200 {
                clk += 0.03;
                let ctx = Ctx { dt: 0.03, time: clk, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                if !sc.in_ready() && !sc.in_input() {
                    saw_show = true; // the only remaining reachable phase here
                }
                if sc.in_input() {
                    break;
                }
            }
            if starts_ready && saw_show && sc.in_input() {
                println!("PASS singback-lead-in");
            } else {
                println!("FAIL singback-lead-in (starts_ready={starts_ready}, saw_show={saw_show}, in_input={})", sc.in_input());
                fails += 1;
            }
        }
        // singback: no same-critter-twice-in-a-row in the easy stage. Grow the
        // sequence (by completing rounds) across many seeds and assert that while
        // the length is in the easy stage, no two ADJACENT pads are equal.
        {
            use games::singback::EASY_NO_REPEAT_LEN;
            let mut ok = true;
            for seed in 0..40u32 {
                let mut sc = SingbackScene::new(Db::mem(), seed * 7 + 1, now);
                // Check the freshly-built sequence first.
                for win in sc.sequence().windows(2) {
                    if sc.sequence().len() < EASY_NO_REPEAT_LEN && win[0] == win[1] {
                        ok = false;
                    }
                }
                // Complete a few rounds to grow it, re-checking each easy-stage seq.
                let mut clk = 0.0f32;
                for _ in 0..4 {
                    sc.skip_to_input();
                    let seq: Vec<u8> = sc.sequence().to_vec();
                    if seq.len() < EASY_NO_REPEAT_LEN {
                        for win in seq.windows(2) {
                            if win[0] == win[1] {
                                ok = false;
                            }
                        }
                    }
                    for &p in &seq {
                        clk += 0.3;
                        let ptr = tap(sc.pad_center(&frame, p as usize));
                        let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                        sc.update(&ctx);
                    }
                    // Settle past Reward so the next pad appends (it deduped).
                    for _ in 0..40 {
                        clk += 0.1;
                        let idle = Pointer::default();
                        let ctx = Ctx { dt: 0.1, time: clk, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                        sc.update(&ctx);
                    }
                }
            }
            if ok {
                println!("PASS singback-no-repeat");
            } else {
                println!("FAIL singback-no-repeat (adjacent equal pads found in easy stage)");
                fails += 1;
            }
        }
        // singback: completing a round of FINALE_SPAN (6) enters the Finale; then
        // tapping the corner REPLAY restarts the session at the difficulty's
        // start length (best_span stays — monotonic). Drive rounds until len 6.
        {
            let mut sc = SingbackScene::new(Db::mem(), 99, now);
            let idle = Pointer::default();
            let mut clk = 0.0f32;
            let mut reached_finale = false;
            // Up to a generous number of rounds; each completes + grows by one.
            for _ in 0..12 {
                sc.skip_to_input();
                let seq: Vec<u8> = sc.sequence().to_vec();
                for &p in &seq {
                    clk += 0.3;
                    let ptr = tap(sc.pad_center(&frame, p as usize));
                    let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                if sc.in_finale() {
                    reached_finale = true;
                    break;
                }
                // Settle past Reward → append + back to Ready.
                for _ in 0..40 {
                    clk += 0.1;
                    let ctx = Ctx { dt: 0.1, time: clk, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
            }
            let best_at_finale = sc.best_span();
            // The invisible topbar must be DEAD during the Finale: a tap on the
            // top-LEFT (where ← Home / long-press parent would live) must NOT
            // navigate (no visible control there) and must leave us in Finale.
            let tb = chrome::topbar(&frame);
            clk += 0.3;
            let ptr = tap(tb.home.0);
            let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            let nav = sc.update(&ctx);
            let topbar_dead = matches!(nav, Nav::Stay) && sc.in_finale();
            // Corner replay restarts the session. Find the corner replay center.
            let (rc, _home, _br) = chrome::corner_buttons(&frame);
            clk += 0.3;
            let ptr = tap(rc);
            let ctx = Ctx { dt: 0.05, time: clk, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            // Restarted: short sequence again, count-in begins, best unchanged.
            let restarted_short = sc.sequence().len() <= 3; // normal start_len = 2
            let best_kept = sc.best_span() == best_at_finale && best_at_finale >= 6;
            let restarted_ready = sc.in_ready();
            if reached_finale && topbar_dead && restarted_short && best_kept && restarted_ready {
                println!("PASS singback-finale");
            } else {
                println!("FAIL singback-finale (reached={reached_finale}, topbar_dead={topbar_dead}, short={restarted_short}, best_kept={best_kept} (best={}), ready={restarted_ready})", sc.best_span());
                fails += 1;
            }
        }
        // singback: a rapid double-tap on the SAME pad registers ONCE (the tap
        // debounce swallows the second edge of one physical press). With the
        // game clock barely advancing, two taps on the first step's pad must
        // leave `got` at 1, not 2 (the second never lands).
        {
            let mut sc = SingbackScene::new(Db::mem(), 99, now);
            sc.skip_to_input();
            let first = sc.sequence()[0];
            // Two taps within the debounce window (clock barely moves).
            let ptr = tap(sc.pad_center(&frame, first as usize));
            let ctx = Ctx { dt: 0.0, time: 5.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let after_one = sc.got();
            // Second edge: only +0.05s of clock (< TAP_DEBOUNCE_S = 0.15).
            let ptr = tap(sc.pad_center(&frame, first as usize));
            let ctx = Ctx { dt: 0.0, time: 5.05, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let after_two = sc.got();
            // got advanced exactly once (1), the second tap was swallowed.
            if after_one == 1 && after_two == 1 {
                println!("PASS singback-debounce");
            } else {
                println!("FAIL singback-debounce (after_one={after_one}, after_two={after_two})");
                fails += 1;
            }
        }
        // singback: the debounce is PER-TARGET — a fast tap on a DIFFERENT pad is
        // NOT a bounce, so two correct taps on distinct pads in quick succession
        // (clock inside the debounce window) BOTH land — completing the 2-pad
        // start round. This guards the real failure mode: eating a legitimate
        // fast distinct-pad tap.
        {
            let mut sc = SingbackScene::new(Db::mem(), 99, now);
            sc.skip_to_input();
            let seq: Vec<u8> = sc.sequence().to_vec();
            // The easy-stage dedupe guarantees seq[0] != seq[1] (distinct pads).
            let p0 = seq[0];
            let p1 = seq[1];
            let ptr = tap(sc.pad_center(&frame, p0 as usize));
            let ctx = Ctx { dt: 0.0, time: 8.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let after_first = sc.got();
            // Second tap, a DIFFERENT pad, only +0.05s (< TAP_DEBOUNCE_S = 0.15).
            let ptr = tap(sc.pad_center(&frame, p1 as usize));
            let ctx = Ctx { dt: 0.0, time: 8.05, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            // Both correct taps landed despite the tiny gap (distinct ≠ bounce):
            // the first advanced got, the second completed the 2-pad start round
            // into Reward — NOT Miss (a swallowed 2nd tap would leave us in Input,
            // so reaching Reward proves the distinct-pad tap was accepted).
            let completed = sc.in_reward();
            if p0 != p1 && after_first == 1 && completed {
                println!("PASS singback-debounce-distinct");
            } else {
                println!("FAIL singback-debounce-distinct (p0={p0}, p1={p1}, after_first={after_first}, in_reward={}, in_input={})", sc.in_reward(), sc.in_input());
                fails += 1;
            }
        }
        println!("PLAYTEST done, {fails} failure(s)");
        std::process::exit(if fails == 0 { 0 } else { 1 });
    }

    // Interactive (the real app): persistent store so the sync token, mute, and
    // local progress survive a reload. Capture/playtest above stay in-memory.
    let db = Db::persistent();
    let muted = {
        let kv = db.borrow_kv();
        fountouki_core::settings::load_shared(&**kv).muted
    };
    let audio = Audio::load(muted).await;
    let mut scene: Box<dyn Scene> = Box::new(PickerScene::new(db.clone()));
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
                    scene = Box::new(PickerScene::new(db.clone()));
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
