//! `mmk-play`'s SMM2 front end (`mmk_front`): ties the platform-neutral screens to the shell's
//! window loop, renderer and sound: keyboard / pads → menu buttons, a fixed 60 Hz tick, the
//! layout draw lists through [`UiLayer`] (one per UI import) with the course view drawn by the
//! game [`Renderer`] where the front end puts it, the UI sounds into the output stream, and the
//! front end's messages as a small text box.

use std::collections::VecDeque;
use std::sync::Arc;

use mmk_front::{Buttons, DrawItem, Front, FrontInput, UiKind};
use mmk_ui::{
    BLEND_NORMAL, COMBINE_NONE, Quad, SHIFT_COMBINE_ALPHA, SHIFT_COMBINE_COLOR, TEXTURE_NONE,
};
use winit::keyboard::KeyCode;

// the host: files, threads and the course server natively; the browser's storage on the web
#[cfg(not(target_arch = "wasm32"))]
use crate::front_host::{HostData, ShellHost};
use crate::game::Game;
use crate::input::Keyboard;
use crate::render::{Gpu, Renderer};
use crate::session::FixedStep;
use crate::ui_layer::UiLayer;
use crate::ui_sound::UiMixer;
#[cfg(target_arch = "wasm32")]
use crate::web_front::{HostData, ShellHost};

pub struct FrontApp {
    pub front: Front,
    pub data: HostData,
    layers: Vec<(UiKind, UiLayer)>,
    step: FixedStep,
    pub ui_audio: Option<Arc<UiMixer>>,
    click: Option<[f32; 2]>,
    /// The mouse in layout units (None: outside the view), its left / right buttons held.
    pointer: Option<[f32; 2]>,
    pointer_down: bool,
    pointer_alt: bool,
    /// The message shown and its texture in the SMM2 import's cache.
    toast: Option<(String, Option<mmk_ui::TexRef>)>,
    toasts: u32,
    /// Keys typed into an open text field, one handed to the front end per tick.
    typed: VecDeque<char>,
    /// Keeps the sound playing.
    pub audio_stream: Option<crate::ui_sound::FrontAudio>,
    /// On-screen controls to draw this frame (`TouchPad::shapes`; empty: none).
    pub touch_shapes: Vec<([f32; 2], f32, bool)>,
}

/// Menu buttons, pause and skip from the keyboard and the pads' engine buttons, as in the Swift
/// app: arrows / WASD move, Return / Z / Space confirm, Esc / P / Backspace go back (a key that
/// does both confirms); pads: the d-pad, jump confirms, run and Start go back. Skip: − or Tab.
pub fn menu_input(keys: &Keyboard, pad: u32) -> FrontInput {
    use mmk_sim::input as b;
    let k = |codes: &[KeyCode]| codes.iter().any(|&c| keys.held(c));
    let mut m = Buttons::NONE;
    let mut add = |on: bool, x: Buttons| {
        if on {
            m = m | x;
        }
    };
    add(
        k(&[KeyCode::ArrowUp, KeyCode::KeyW]) || pad & b::UP != 0,
        Buttons::UP,
    );
    add(
        k(&[KeyCode::ArrowDown, KeyCode::KeyS]) || pad & b::DOWN != 0,
        Buttons::DOWN,
    );
    add(
        k(&[KeyCode::ArrowLeft, KeyCode::KeyA]) || pad & b::LEFT != 0,
        Buttons::LEFT,
    );
    add(
        k(&[KeyCode::ArrowRight, KeyCode::KeyD]) || pad & b::RIGHT != 0,
        Buttons::RIGHT,
    );
    let ctrl = k(&[
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    // (Ctrl+Z is the maker's undo, not a confirm)
    let confirm = k(&[KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
        || (keys.held(KeyCode::KeyZ) && !ctrl)
        || pad & b::JUMP != 0;
    let back =
        k(&[KeyCode::Escape, KeyCode::KeyP, KeyCode::Backspace]) || pad & (b::RUN | b::START) != 0;
    add(confirm, Buttons::CONFIRM);
    add(back && !confirm, Buttons::BACK);
    FrontInput {
        menu: m,
        start: keys.buttons() & b::START != 0 || pad & b::START != 0,
        skip: k(&[KeyCode::Minus, KeyCode::Tab]),
        click: None,
        text: None,
        pointer: None,
        pointer_down: false,
        pointer_alt: false,
        keys: maker_keys(keys, ctrl),
    }
}

/// The Course Maker's keys: U / Ctrl+Z undo, Y / Ctrl+Shift+Z redo, Q the part window, [ ]
/// the palette slots, T test play from the cursor, E the eraser, G the other area, M held
/// multigrab, C copy the selection.
pub fn maker_keys(keys: &Keyboard, ctrl: bool) -> u16 {
    use mmk_front::maker::keys as m;
    let shift = keys.held(KeyCode::ShiftLeft) || keys.held(KeyCode::ShiftRight);
    let z = keys.held(KeyCode::KeyZ) && ctrl;
    [
        (keys.held(KeyCode::KeyU) || (z && !shift), m::UNDO),
        (keys.held(KeyCode::KeyY) || (z && shift), m::REDO),
        (keys.held(KeyCode::KeyQ), m::PARTS),
        (keys.held(KeyCode::BracketRight), m::NEXT),
        (keys.held(KeyCode::BracketLeft), m::PREV),
        (keys.held(KeyCode::KeyT), m::PLAY_HERE),
        (keys.held(KeyCode::KeyE), m::ERASER),
        (keys.held(KeyCode::KeyG), m::AREA),
        (keys.held(KeyCode::KeyM), m::MULTI),
        (keys.held(KeyCode::KeyC) && !ctrl, m::COPY),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .fold(0, |a, (_, b)| a | b)
}

impl FrontApp {
    pub fn new(front: Front, data: HostData) -> Self {
        Self {
            front,
            data,
            layers: Vec::new(),
            step: FixedStep::default(),
            ui_audio: None,
            click: None,
            pointer: None,
            pointer_down: false,
            pointer_alt: false,
            toast: None,
            toasts: 0,
            typed: VecDeque::new(),
            audio_stream: None,
            touch_shapes: Vec::new(),
        }
    }

    /// One 60 Hz tick of the front end (and of the course when it plays).
    pub fn tick(&mut self, input: &FrontInput, game_buttons: u32, game: &mut Game) {
        self.data.input = game_buttons;
        let mut host = ShellHost {
            game,
            data: &mut self.data,
        };
        self.front.tick(input, &mut host);
        if self.front.course_done {
            // a course played on its own (the web page's course picker): back to the menus
            self.front.course_done = false;
            self.front.capture(&mut host, "title-play", 0, 0);
        }
        let sounds = self.front.take_sounds();
        if let Some(m) = &self.ui_audio {
            for s in &sounds {
                m.play(s);
            }
        }
    }

    /// Host time `dt` with the keyboard and pads: the ticks it is worth.
    pub fn advance(&mut self, dt: f64, keys: &Keyboard, pad: u32, game: &mut Game) {
        let n = self.step.advance(dt);
        let game_buttons = crate::input::combine(keys, pad).buttons;
        let mut input = menu_input(keys, pad);
        input.pointer = self.pointer;
        input.pointer_down = self.pointer_down;
        input.pointer_alt = self.pointer_alt;
        for i in 0..n {
            input.click = if i == 0 { self.click.take() } else { None };
            input.text = self.typed.pop_front();
            self.tick(&input, game_buttons, game);
        }
    }

    /// A key pressed while the front end has a text field open: queued as typed text
    /// (Backspace erases, Return finishes, Esc cancels). False when no field is open (the key
    /// is a menu key then).
    pub fn key_text(&mut self, code: KeyCode, text: Option<&str>) -> bool {
        if !self.front.typing() {
            return false;
        }
        match code {
            KeyCode::Backspace => self.typed.push_back('\u{8}'),
            KeyCode::Enter | KeyCode::NumpadEnter => self.typed.push_back('\n'),
            KeyCode::Escape => self.typed.push_back('\u{1b}'),
            _ => self
                .typed
                .extend(text.unwrap_or_default().chars().filter(|c| !c.is_control())),
        }
        true
    }

    /// A click at window pixel `at` in a `size` window.
    pub fn click(&mut self, at: (f64, f64), size: (u32, u32)) {
        if let Some(p) = Self::layout_point(at, size) {
            self.click = Some(p);
        }
    }

    /// Window pixel `at` in layout units (None: outside the view).
    fn layout_point(at: (f64, f64), size: (u32, u32)) -> Option<[f32; 2]> {
        let (vx, vy, vw, vh, _) = crate::render::viewport(size.0, size.1, false);
        let u = (at.0 as f32 - vx) / vw;
        let v = (at.1 as f32 - vy) / vh;
        ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v))
            .then_some([(u - 0.5) * 1280.0, (0.5 - v) * 720.0])
    }

    /// The mouse moved to window pixel `at` (None: it left the window).
    pub fn pointer_moved(&mut self, at: Option<(f64, f64)>, size: (u32, u32)) {
        self.pointer = at.and_then(|a| Self::layout_point(a, size));
    }

    /// A mouse button went down / up (`secondary`: the right button).
    pub fn pointer_button(&mut self, secondary: bool, down: bool) {
        if secondary {
            self.pointer_alt = down;
        } else {
            self.pointer_down = down;
        }
    }

    fn layer(&mut self, gpu: &Gpu, format: wgpu::TextureFormat, kind: UiKind) -> usize {
        if let Some(i) = self.layers.iter().position(|(k, _)| *k == kind) {
            return i;
        }
        self.layers.push((kind, UiLayer::new(gpu, format)));
        self.layers.len() - 1
    }

    /// The toast's quads (a dark box with the text), its texture in the SMM2 import's cache.
    fn toast_quads(&mut self, text: Option<String>) -> Vec<Quad> {
        let Some(text) = text else {
            self.toast = None;
            return Vec::new();
        };
        if self.toast.as_ref().is_none_or(|t| t.0 != text) {
            let scale = 3;
            let w = crate::hud::text_width(&text, scale) + 2 * scale;
            let h = (crate::hud::GLYPH_H + 2) * scale;
            let mut img = crate::hud::Image::new(w.max(1), h);
            img.text(&text, 0, 0, scale, [255, 255, 255, 255]);
            self.toasts += 1;
            let tex = mmk_assets::Mtex {
                format: mmk_assets::MtexFormat::Rgba8,
                variant: mmk_assets::MtexVariant::Srgb,
                width: img.width as u32,
                height: img.height as u32,
                channel_select: [2, 3, 4, 5],
                data: img.rgba,
            };
            let r = self.front.smm2.cache_mut().add_texture_file(
                "toast",
                &self.toasts.to_string(),
                tex,
            );
            self.toast = Some((text, Some(r)));
        }
        let Some((_, Some(t))) = &self.toast else {
            return Vec::new();
        };
        let (w, h) = (t.width as f32, t.height as f32);
        let y = -360.0 + 60.0;
        let rect = |x0: f32, y0: f32, x1: f32, y1: f32| [[x0, y1], [x1, y1], [x0, y0], [x1, y0]];
        let flags = (COMBINE_NONE << SHIFT_COMBINE_COLOR)
            | (COMBINE_NONE << SHIFT_COMBINE_ALPHA)
            | BLEND_NORMAL;
        let quad = |pos, tex, color: [u8; 4]| Quad {
            pos,
            uv0: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            uv1: [[0.0; 2]; 4],
            color: [color; 4],
            black: [0, 0, 0, 0],
            white: [255; 4],
            tex0: tex,
            tex1: TEXTURE_NONE,
            flags,
        };
        let pad = 18.0;
        vec![
            quad(
                rect(-w / 2.0 - pad, y - pad, w / 2.0 + pad, y + h + pad),
                TEXTURE_NONE,
                [0, 0, 0, 190],
            ),
            quad(rect(-w / 2.0, y, w / 2.0, y + h), t.id, [255; 4]),
        ]
    }

    /// Draw the front end into `target` (`size` px): the course view and the layouts in the
    /// letterboxed 16:9 view, then the message.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        renderer: &mut Renderer,
        game: &mut Game,
    ) {
        let frame = self.front.frame();
        let toast = self.toast_quads(frame.toast.clone());
        let (vx, vy, vw, vh, _) = crate::render::viewport(size.0, size.1, false);
        if !matches!(frame.items.first(), Some(DrawItem::Course)) {
            clear(enc, target);
        }
        renderer.integer_scale = false;
        for item in frame.items {
            match item {
                DrawItem::Course => {
                    game.no_hud = true;
                    let scene = game.scene();
                    renderer.encode(gpu, enc, target, size, &scene);
                }
                DrawItem::Layout {
                    ui,
                    size: lsize,
                    quads,
                } => {
                    let i = self.layer(gpu, format, ui);
                    let cache_ui = match ui {
                        UiKind::Smm2 => Some(&self.front.smm2),
                        UiKind::Smm1 => self.front.smm1.as_ref(),
                    };
                    if let Some(u) = cache_ui {
                        self.layers[i].1.sync(gpu, &u.cache());
                    }
                    self.layers[i]
                        .1
                        .encode(gpu, enc, target, (vx, vy, vw, vh), &quads, lsize);
                }
            }
        }
        if !self.touch_shapes.is_empty() {
            // on-screen controls over the whole window (window px, origin centre, y up)
            let (w, h) = (size.0 as f32, size.1 as f32);
            let quads = touch_quads(&self.touch_shapes, w, h);
            let i = self.layer(gpu, format, UiKind::Smm2);
            self.layers[i]
                .1
                .encode(gpu, enc, target, (0.0, 0.0, w, h), &quads, [w, h]);
        }
        if !toast.is_empty() {
            let i = self.layer(gpu, format, UiKind::Smm2);
            self.layers[i].1.sync(gpu, &self.front.smm2.cache());
            self.layers[i]
                .1
                .encode(gpu, enc, target, (vx, vy, vw, vh), &toast, [1280.0, 720.0]);
        }
    }
}

/// The on-screen controls as rings (a translucent disc with a brighter edge; brighter while
/// held), in window px around the window's centre, y up.
fn touch_quads(shapes: &[([f32; 2], f32, bool)], w: f32, h: f32) -> Vec<Quad> {
    const SEGMENTS: usize = 32;
    let flags = (COMBINE_NONE << SHIFT_COMBINE_COLOR)
        | (COMBINE_NONE << SHIFT_COMBINE_ALPHA)
        | BLEND_NORMAL;
    let mut out = Vec::new();
    // a quad from four points (TL, TR, BL, BR order; the two triangles are TL-BL-TR, TR-BL-BR)
    let mut quad = |pos: [[f32; 2]; 4], color: [u8; 4]| {
        out.push(Quad {
            pos,
            uv0: [[0.0; 2]; 4],
            uv1: [[0.0; 2]; 4],
            color: [color; 4],
            black: [0, 0, 0, 0],
            white: [255; 4],
            tex0: TEXTURE_NONE,
            tex1: TEXTURE_NONE,
            flags,
        })
    };
    for &([cx, cy], r, held) in shapes {
        let c = [cx - w / 2.0, h / 2.0 - cy];
        let at = |k: usize, rr: f32| {
            let a = k as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            [c[0] + rr * a.cos(), c[1] + rr * a.sin()]
        };
        let (fill, edge) = if held {
            ([255, 255, 255, 110], [255, 255, 255, 230])
        } else {
            ([0, 0, 0, 70], [255, 255, 255, 140])
        };
        for k in 0..SEGMENTS {
            // the disc: a fan of triangles (the quad's first triangle is degenerate)
            quad([c, at(k, r), c, at(k + 1, r)], fill);
            // the edge: a band from 0.9 r to r
            quad(
                [at(k, r * 0.9), at(k + 1, r * 0.9), at(k, r), at(k + 1, r)],
                edge,
            );
        }
    }
    out
}

fn clear(enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("front clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}
