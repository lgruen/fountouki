//! Home picker: large tiles, one per game, with drawn icon teasers (no emoji,
//! no acorn — the acorn is the launcher icon only). Tap a tile → open that game.
use crate::{
    draw, input,
    palette,
    scene::{Ctx, Nav, Scene},
    text,
};
use macroquad::prelude::*;

/// (route id, label). Order = display order. Adding a game = add an entry +
/// a match arm in `draw_icon` and `main::build_game`.
pub const GAMES: &[(&str, &str)] = &[
    ("patterns", "patterns"),
    ("phonics", "phonics"),
    ("tracing", "tracing"),
    ("singback", "sing back"),
];

pub struct PickerScene {
    db: crate::store::Db,
}

impl PickerScene {
    pub fn new(db: crate::store::Db) -> PickerScene {
        PickerScene { db }
    }
}

impl Scene for PickerScene {
    fn update(&mut self, ctx: &Ctx) -> Nav {
        let pt = ctx.pointer;
        if !pt.tapped() {
            return Nav::Stay;
        }
        let (m, mr) = mute_pos(&ctx.frame);
        if input::hit_circle(pt.pos, m.x, m.y, mr) {
            let muted = !ctx.audio.muted();
            ctx.audio.set_muted(muted);
            crate::store::persist_mute(&self.db, muted);
            return Nav::Stay;
        }
        for (i, (id, _)) in GAMES.iter().enumerate() {
            let r = tile_rect(&ctx.frame, i);
            if input::hit_rect(pt.pos, r.x, r.y, r.w, r.h) {
                return Nav::Game(id.to_string());
            }
        }
        Nav::Stay
    }

    fn draw(&mut self, ctx: &Ctx) {
        clear_background(palette::BG);
        let (m, mr) = mute_pos(&ctx.frame);
        draw::circle_btn(m.x, m.y, mr, palette::CARD);
        draw::speaker(m.x, m.y, mr * 0.9, palette::INK, ctx.audio.muted());

        for (i, (id, label)) in GAMES.iter().enumerate() {
            let r = tile_rect(&ctx.frame, i);
            draw::card(r.x, r.y, r.w, r.h, palette::CARD);
            draw_icon(id, r, ctx);
            text::ui_centered(label, r.x + r.w / 2.0, r.y + r.h * 0.84, (r.w * 0.12) as u16, palette::MUTED);
        }
    }
}

fn mute_pos(f: &crate::layout::Frame) -> (Vec2, f32) {
    let tb = f.topbar();
    let ir = f.icon_btn() / 2.0;
    (vec2(tb.x + tb.w - ir, tb.y + ir), ir)
}

fn tile_rect(f: &crate::layout::Frame, i: usize) -> Rect {
    let n = GAMES.len() as f32;
    let content = f.content();
    let gap = (f.w * 0.04).clamp(20.0, 60.0);
    // Shrink the tile so all N fit across the safe content width on one row
    // (never wrap, never clip the edge tiles) — mirrors patterns' choice-fit.
    let fit_w = (content.w - (n - 1.0) * gap) / n;
    let tw = (f.w * 0.24).clamp(120.0, 260.0).min(fit_w);
    let th = tw * 1.12;
    let total = n * tw + (n - 1.0) * gap;
    let x0 = f.w / 2.0 - total / 2.0;
    let y = f.h / 2.0 - th / 2.0;
    Rect::new(x0 + i as f32 * (tw + gap), y, tw, th)
}

/// Drawn icon teaser per game (mechanic preview without reading).
fn draw_icon(id: &str, r: Rect, ctx: &Ctx) {
    let cx = r.x + r.w / 2.0;
    let cy = r.y + r.h * 0.42;
    match id {
        "patterns" => {
            // mini sequence: circle, triangle, pink ?
            let s = r.w * 0.18;
            let gap = s * 0.5;
            let x0 = cx - (s + gap);
            draw_circle(x0, cy, s / 2.0, palette::RAINBOW[3]);
            draw_triangle(
                vec2(x0 + s + gap, cy - s / 2.0),
                vec2(x0 + s + gap - s / 2.0, cy + s / 2.0),
                vec2(x0 + s + gap + s / 2.0, cy + s / 2.0),
                palette::RAINBOW[1],
            );
            draw::rounded_rect(x0 + 2.0 * (s + gap) - s / 2.0, cy - s / 2.0, s, s, s * 0.2, palette::ACCENT_SOFT);
            text::draw_centered("?", x0 + 2.0 * (s + gap), cy, (s * 0.9) as u16, &ctx.fonts.cursive, palette::ACCENT);
        }
        "phonics" => {
            // mini rainbow swaying above the frog mascot
            let scale = r.w / 240.0 * 0.9;
            draw::rainbow(cx, cy + r.w * 0.1, scale, (8.0 * scale).max(5.0), 7);
            let fr = r.w * 0.12;
            let fy = cy + r.w * 0.20;
            draw::frog(cx, fy, fr, palette::RAINBOW[3], draw::FrogPose::default());
        }
        "tracing" => {
            // The mechanic (a cursive 'a' wearing the chart's start/end dots)
            // beside the reward (the build-a-house site, crane mid-build).
            use fountouki_core::tracing as tr;
            if let Some(g) = tr::glyph('a') {
                let font_px = (r.w * 0.52) as u16;
                let scale = font_px as f32 / tr::UPEM;
                let bb = tr::ink_bbox(g);
                let pen = vec2(
                    r.x + r.w * 0.27 - (bb.0 + bb.2) / 2.0 * scale,
                    cy + (bb.1 + bb.3) / 2.0 * scale + r.h * 0.04,
                );
                draw_text_ex(
                    "a",
                    pen.x,
                    pen.y,
                    TextParams {
                        font: Some(&ctx.fonts.cursive),
                        font_size: font_px,
                        color: palette::INK,
                        ..Default::default()
                    },
                );
                let to_px = |p: (f32, f32)| vec2(pen.x + p.0 * scale, pen.y - p.1 * scale);
                let start = to_px(g.strokes[0][0]);
                let end = to_px(*g.strokes[0].last().unwrap());
                let dr = r.w * 0.042;
                draw::disc(end.x, end.y, dr * 0.8, palette::RAINBOW[0]);
                draw::disc(start.x, start.y, dr, palette::OK_STRONG);
            }
            let pose = draw::HousePose { parts: 4, site: true, ..Default::default() };
            draw::house(r.x + r.w * 0.71, r.y + r.h * 0.70, r.w * 0.235, &pose);
        }
        "singback" => {
            // A choir preview: a row of four pad-colored dots, the 2nd "singing"
            // (a glow halo behind a popped, brighter dot) — the memory mechanic.
            // Colors match the game's pitch→color map (warm→cool).
            let cols = [palette::RAINBOW[0], palette::RAINBOW[2], palette::RAINBOW[3], palette::RAINBOW[6]];
            let s = r.w * 0.10;
            let gap = s * 1.6;
            let total = 3.0 * gap;
            let x0 = cx - total / 2.0;
            let lit = 1; // the glowing member
            for (i, &col) in cols.iter().enumerate() {
                let x = x0 + i as f32 * gap;
                if i == lit {
                    draw::disc(x, cy - s * 0.18, s * 1.7, Color::new(col.r, col.g, col.b, 0.4));
                    draw::disc(x, cy - s * 0.18, s * 1.15, col);
                } else {
                    draw::disc(x, cy, s, col);
                }
            }
        }
        _ => {}
    }
}
