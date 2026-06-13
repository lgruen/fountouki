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
                // Mid-trace of 'c' (pinned — the SRS queue is shuffled): guides,
                // faded glyph, laid ink, breadcrumbs, pen marker, red end dot.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('c');
                sc.skip_watch();
                for i in 0..=20 {
                    let ptr = drag(sc.stroke_point_px(&frame, 0.45 * i as f32 / 20.0));
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
                // 'x' with stroke 1 traced: shows the numbered "2" start dot.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('x');
                sc.skip_watch();
                for i in 0..=40 {
                    let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 40.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Lift at the end dot — strokes complete on the release.
                let idle = Pointer::default();
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                Box::new(sc)
            }
            "tracing-grade" => {
                // A finished 'c' awaiting the parent's ✓/✗ under the card.
                let frame = Frame::new(w as f32, h as f32, Insets::default());
                let mut sc = TracingScene::new(db.clone(), 7, now);
                sc.debug_set_letter('c');
                sc.skip_watch();
                for i in 0..=40 {
                    let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 40.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Lift to complete the letter, then settle through the reward
                // beat into the grade phase.
                let idle = Pointer::default();
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ctx = Ctx { dt: 1.5, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
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
                    let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 40.0));
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                }
                // Lift to complete the letter, settle through the reward beat
                // into the grade row, tap ✓, then catch the excavator mid-dig
                // on the foundation stage.
                let idle = Pointer::default();
                let ctx = Ctx { dt: 0.05, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                let ctx = Ctx { dt: 1.5, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
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
            let skipped = watched && !sc.in_watch();

            // Errorless: a finger far off the path lays no ink.
            let far = sc.stroke_point_px(&frame, 0.5) + vec2(200.0, 200.0);
            let ptr = drag(far);
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let errorless = sc.stroke_index() == 0 && sc.stars == 0;

            // Trace all SESSION_GOAL letters stroke by stroke, like a finger,
            // with the parent tapping ✓ after each reward beat.
            let goal = fountouki_core::tracing::SESSION_GOAL as u32;
            let mut graded_each_time = true;
            'session: for _ in 0..goal {
                sc.skip_watch();
                let mut guard = 0;
                while !sc.awaiting_advance() && !sc.is_done() && guard < 12 {
                    for i in 0..=30 {
                        let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 30.0));
                        let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                        sc.update(&ctx);
                    }
                    // Lift at the end dot — the stroke completes on release.
                    let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                    sc.update(&ctx);
                    guard += 1;
                }
                // Settle through the reward beat into the parent grade.
                let ctx = Ctx { dt: 1.5, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
                graded_each_time &= sc.in_grade();
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
            // stays), a window lamp lights and STAYS lit (monotonic).
            let ptr = tap(sc.door_center(&frame));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let door_ok = sc.is_done() && sc.door_taps() == 1;
            let ptr = tap(sc.window_center(&frame, 1));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            // A party-guest frog reacts to its tap too.
            let ptr = tap(sc.friend_center(&frame, 0));
            let ctx = Ctx { dt: 0.1, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let friend_ok = sc.friend_taps() == 1;
            let idle2 = Pointer::default();
            let ctx = Ctx { dt: 2.0, time: 0.0, now, pointer: &idle2, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if door_ok && friend_ok && sc.is_done() && sc.window_lit(1) && !sc.window_lit(0) {
                println!("PASS tracing-housewarming");
            } else {
                println!(
                    "FAIL tracing-housewarming (door_ok={door_ok}, friend_ok={friend_ok}, done={}, lit1={}, lit0={})",
                    sc.is_done(),
                    sc.window_lit(1),
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
        // tracing: the dot letters — i's body then its dot (a tap, not a drag).
        {
            let mut sc = TracingScene::new(Db::mem(), 7, now);
            sc.debug_set_letter('i');
            sc.skip_watch();
            // Trace the body and lift to complete it; then the dot (a tap
            // target, not a drag) finishes the letter.
            for i in 0..=30 {
                let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 30.0));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let idle = Pointer::default();
            let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if !sc.awaiting_advance() {
                let ptr = drag(sc.stroke_point_px(&frame, 0.0));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            if sc.awaiting_advance() && sc.current_letter() == 'i' {
                println!("PASS tracing-dot-letter");
            } else {
                println!(
                    "FAIL tracing-dot-letter (finished={}, letter={})",
                    sc.awaiting_advance(),
                    sc.current_letter()
                );
                fails += 1;
            }
        }
        // tracing: the stroke gates — a finger dropped ON the path mid-stroke
        // (inside the corridor, but never at the green start dot) lays no
        // progress; dragging dot-to-dot doesn't finish while the finger is
        // still down (no mid-drag snap); the LIFT at the red end dot does.
        {
            let mut sc = TracingScene::new(Db::mem(), 7, now);
            sc.debug_set_letter('c');
            sc.skip_watch();
            for _ in 0..5 {
                let ptr = drag(sc.stroke_point_px(&frame, 0.4));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            let gated = sc.stroke_index() == 0 && sc.stroke_progress() <= 0.0;
            for i in 0..=40 {
                let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 40.0));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            // Finger held down at the end dot: the stroke must still be live.
            let held = !sc.awaiting_advance() && sc.stroke_index() == 0;
            let idle = Pointer::default();
            let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            if gated && held && sc.awaiting_advance() {
                println!("PASS tracing-stroke-gates");
            } else {
                println!(
                    "FAIL tracing-stroke-gates (gated={gated}, held={held}, finished={})",
                    sc.awaiting_advance()
                );
                fails += 1;
            }
        }
        // tracing: the parent's ✗ holds the letter's Leitner box down and the
        // house does NOT gain a part (only ✓ builds, like phonics' rainbow);
        // the session moves on to a different letter.
        {
            let mut sc = TracingScene::new(Db::mem(), 7, now);
            let idle = Pointer::default();
            sc.debug_set_letter('c');
            sc.skip_watch();
            for i in 0..=40 {
                let ptr = drag(sc.stroke_point_px(&frame, i as f32 / 40.0));
                let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &ptr, frame, fonts: &fonts, audio: &audio };
                sc.update(&ctx);
            }
            // Lift to complete, then settle through the reward beat.
            let ctx = Ctx { dt: 0.02, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let ctx = Ctx { dt: 1.5, time: 0.0, now, pointer: &idle, frame, fonts: &fonts, audio: &audio };
            sc.update(&ctx);
            let graded = sc.in_grade();
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
