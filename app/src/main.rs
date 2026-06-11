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
