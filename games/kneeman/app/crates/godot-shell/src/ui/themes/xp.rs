//! Windows XP "Luna" chrome. Worn by the menu, applied PER WINDOW via Frame + painter, never through
//! global egui Visuals, so the dark debug panel keeps its own look. `Xp` implements [`super::Theme`].

use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Order, Rect, Sense, Stroke};

// Luna palette -------------------------------------------------------------------------------------
pub const TITLE_TOP: Color32 = Color32::from_rgb(0x3A, 0x93, 0xFF); // active title gradient, light at top
pub const TITLE_BOT: Color32 = Color32::from_rgb(0x00, 0x54, 0xE3); // ... deep blue at the bottom
pub const FRAME: Color32 = Color32::from_rgb(0x00, 0x3C, 0xC8); // blue window border
pub const FACE: Color32 = Color32::from_rgb(0xEC, 0xE9, 0xD8); // classic beige-gray body
pub const FACE_HI: Color32 = Color32::from_rgb(0xFD, 0xFD, 0xF6); // hovered button face
pub const FACE_DN: Color32 = Color32::from_rgb(0xD8, 0xD4, 0xC0); // pressed button face
pub const INK: Color32 = Color32::from_rgb(0x10, 0x10, 0x10); // near-black body text
pub const BEVEL_LO: Color32 = Color32::from_rgb(0x91, 0x8E, 0x78); // button shadow edge
pub const RAIL_HOVER: Color32 = Color32::from_rgb(0xEF, 0xF3, 0xFF); // nav hover wash
pub const RAIL_SEL: Color32 = Color32::from_rgb(0xFF, 0xE9, 0x9E); // current-page highlight

/// Gap kept between the window and the viewport edge, in every clamp axis, so the beveled frame
/// never touches the screen border even at the clamp's tightest.
const VIEWPORT_MARGIN: f32 = 24.0;
/// Fraction of the (counter-scaled, see ui/scale.rs) viewport this window may claim per axis.
/// Applied as `min(fraction * viewport, hard cap)` instead of the old "viewport minus a fixed
/// margin" -- that formula filled nearly the whole screen on any window bigger than the design
/// canvas, because `content_rect()` tracked the *design-space* size, not the real window. Now
/// that `ctx.content_rect()` tracks the real window 1:1 (post counter-scale), a bare margin
/// subtraction would once again go near-fullscreen on a big monitor; the fraction caps it as a
/// proportion of the real window instead.
const VIEWPORT_WIDTH_FRACTION: f32 = 0.6;
const VIEWPORT_HEIGHT_FRACTION: f32 = 0.75;
/// Hard caps in UI points, which post counter-scale are ~1:1 with real physical pixels: a wide
/// page (Controls) should stop growing once it's comfortably readable, however big the monitor.
const MAX_WINDOW_WIDTH: f32 = 820.0;
const MAX_WINDOW_HEIGHT: f32 = 700.0;
/// Fixed chrome height inside the frame that isn't the caller's body: the title bar (26px, see
/// `title_bar`) plus the spacer drawn right after it in `frame_window`.
const TITLE_CHROME_H: f32 = 26.0 + 8.0;
/// The frame's own left+right inner margin (3px each, see `frame_window`'s `Margin`), subtracted
/// from the clamped outer width to get the body's usable width.
const FRAME_MARGIN_W: f32 = 3.0 + 3.0;
/// The frame's own bottom inner margin (10px, see `frame_window`'s `Margin`), subtracted from the
/// clamped outer height to get the body's usable height.
const FRAME_MARGIN_BOTTOM: f32 = 10.0;

/// The XP theme as a unit value. Carries no state.
pub struct Xp;

impl super::Theme for Xp {
    fn window(
        &self,
        ctx: &egui::Context,
        title: &str,
        add: impl FnOnce(&mut egui::Ui, egui::Vec2),
    ) {
        frame_window(ctx, "xp_win", title, Order::Middle, add);
    }

    fn dialog(&self, ctx: &egui::Context, title: &str, add: impl FnOnce(&mut egui::Ui)) {
        // dim the base behind the modal, then float the framed dialog above the scrim.
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("xp_scrim"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.painter()
                    .rect_filled(screen, 0.0, Color32::from_black_alpha(120));
            });
        // Dialog bodies don't need the body-size hint (they're small, fixed-shape confirms).
        frame_window(
            ctx,
            "xp_dialog",
            title,
            Order::Foreground,
            |ui, _body_max| add(ui),
        );
    }

    fn button(&self, ui: &mut egui::Ui, label: &str) -> egui::Response {
        button(ui, label)
    }

    fn nav_item(&self, ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
        nav_item(ui, label, selected)
    }
}

/// A centered XP window at the given layer: beige body, blue border, rounded top, captioned bar.
/// Body text is forced dark so labels read on the beige face despite the global dark Visuals.
///
/// `area_id` is a stable string key for the egui Area; it must NOT include the route title so
/// that widget IDs remain stable across route changes (otherwise focus resets on every nav).
///
/// Clamped to the viewport: the whole frame's outer size is capped to `min(fraction * viewport,
/// hard cap, viewport - VIEWPORT_MARGIN)` on every axis (see [`VIEWPORT_WIDTH_FRACTION`] /
/// [`MAX_WINDOW_WIDTH`] and their height counterparts), via `ui.set_max_width`/`set_max_height`
/// on the Area's own ui *before* the `Frame::show` call, so the cap propagates into the frame's
/// inner content ui (see `Frame::begin`, which derives its child's `max_rect` from
/// `ui.available_rect_before_wrap()`). A bare "viewport minus a fixed margin" (the old formula)
/// goes near-fullscreen on any window bigger than the design canvas -- the fraction+cap keeps a
/// wide page like Controls from filling an ultrawide monitor, and the margin term is the
/// belt-and-suspenders floor against a viewport smaller than the caps. `.constrain_to` is its own
/// backstop: egui's own Area constrain only re-clamps during the first invisible sizing pass,
/// then trusts the previous frame's cached size, so it is not sufficient alone against a caller
/// whose content ignores the max-width/height hint.
/// `add` receives the room left for its body (below the title bar, inside the margins) so the
/// caller can pick compact vs. roomy layout and bound its own `ScrollArea` -- overflowing page
/// content should scroll in there, not grow this window past the clamp.
fn frame_window(
    ctx: &egui::Context,
    area_id: &'static str,
    title: &str,
    order: Order,
    add: impl FnOnce(&mut egui::Ui, egui::Vec2),
) {
    let screen = ctx.content_rect();
    let viewport_minus_margin = egui::vec2(
        (screen.width() - VIEWPORT_MARGIN * 2.0).max(240.0),
        (screen.height() - VIEWPORT_MARGIN * 2.0).max(160.0),
    );
    // Modals render narrower than their parent window so the visual hierarchy reads (the dialog
    // is a child of whatever window opened it). 80px total inset (40 each side) keeps the modal
    // visibly contained without going so narrow that the confirm buttons wrap. Window entries
    // (xp_win) get no inset.
    let inset_x = if area_id == "xp_dialog" { 80.0 } else { 0.0 };
    let max_outer = egui::vec2(
        ((screen.width() * VIEWPORT_WIDTH_FRACTION).min(MAX_WINDOW_WIDTH) - inset_x)
            .min(viewport_minus_margin.x),
        (screen.height() * VIEWPORT_HEIGHT_FRACTION)
            .min(MAX_WINDOW_HEIGHT)
            .min(viewport_minus_margin.y),
    );

    egui::Area::new(egui::Id::new(area_id))
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .order(order)
        .constrain_to(screen)
        .show(ctx, |ui| {
            ui.set_max_width(max_outer.x);
            ui.set_max_height(max_outer.y);
            egui::Frame::NONE
                .fill(FACE)
                .stroke(Stroke::new(1.0_f32, FRAME))
                .corner_radius(CornerRadius {
                    nw: 8,
                    ne: 8,
                    sw: 3,
                    se: 3,
                })
                .inner_margin(Margin {
                    left: 3,
                    right: 3,
                    top: 3,
                    bottom: 10,
                })
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(INK);
                    // `.strong()` captions (e.g. the "Pause Menu" heading) resolve to
                    // `widgets.active`/`noninteractive` text color, NOT override_text_color, so the
                    // global dark Visuals paint them near-white on the beige face. Pin both dark.
                    ui.visuals_mut().widgets.active.fg_stroke.color = INK;
                    ui.visuals_mut().widgets.noninteractive.fg_stroke.color = INK;
                    // Text fields keep the global dark extreme_bg_color unless pinned, which
                    // renders the forced-INK text black-on-black. Sunken XP field: white well,
                    // dark caret, Luna-blue selection.
                    ui.visuals_mut().text_edit_bg_color = Some(Color32::WHITE);
                    ui.visuals_mut().text_cursor.stroke.color = INK;
                    ui.visuals_mut().selection.bg_fill = TITLE_TOP;
                    ui.visuals_mut().selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
                    // Never demand more than the clamp allows -- on a tiny viewport this pin used
                    // to force the frame wider than the screen (the overflow bug this all fixes).
                    ui.set_min_width(380.0_f32.min(max_outer.x));
                    title_bar(ui, title);
                    ui.add_space(8.0);
                    let body_max = egui::vec2(
                        (max_outer.x - FRAME_MARGIN_W).max(200.0),
                        (max_outer.y - TITLE_CHROME_H - FRAME_MARGIN_BOTTOM).max(120.0),
                    );
                    add(ui, body_max);
                });
        });
}

/// Vertical two-stop gradient fill (egui has no gradient primitive, so paint a 4-vert mesh).
fn vgrad(painter: &egui::Painter, rect: Rect, top: Color32, bot: Color32) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: rect.left_top(),
        uv: WHITE_UV,
        color: top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_top(),
        uv: WHITE_UV,
        color: top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_bottom(),
        uv: WHITE_UV,
        color: bot,
    });
    mesh.vertices.push(Vertex {
        pos: rect.left_bottom(),
        uv: WHITE_UV,
        color: bot,
    });
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// The Luna title bar: blue gradient strip, top gloss line, white bold caption. Spans the row.
fn title_bar(ui: &mut egui::Ui, title: &str) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 26.0), Sense::hover());
    let p = ui.painter();
    vgrad(p, rect, TITLE_TOP, TITLE_BOT);
    p.line_segment(
        [
            rect.left_top() + egui::vec2(3.0, 1.5),
            rect.right_top() + egui::vec2(-3.0, 1.5),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 120)),
    );
    p.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, FontFamily::Proportional),
        Color32::WHITE,
    );
}

/// A raised XP push button. Restyles locally via a scope so the global dark theme is untouched.
fn button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        let r = CornerRadius::same(3);
        v.widgets.inactive.weak_bg_fill = FACE;
        v.widgets.inactive.bg_fill = FACE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BEVEL_LO);
        v.widgets.inactive.corner_radius = r;
        v.widgets.hovered.weak_bg_fill = FACE_HI;
        v.widgets.hovered.bg_fill = FACE_HI;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, TITLE_BOT);
        v.widgets.hovered.corner_radius = r;
        v.widgets.active.weak_bg_fill = FACE_DN;
        v.widgets.active.bg_fill = FACE_DN;
        v.widgets.active.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.active.bg_stroke = Stroke::new(1.0_f32, TITLE_BOT);
        v.widgets.active.corner_radius = r;
        ui.add(
            egui::Button::new(egui::RichText::new(label).color(INK))
                .min_size(egui::vec2(88.0, 24.0)),
        )
    })
    .inner
}

/// One left-rail nav entry (XP task pane row). `selected` paints the current page warm + bold.
fn nav_item(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        let r = CornerRadius::same(3);
        let face = if selected {
            RAIL_SEL
        } else {
            Color32::TRANSPARENT
        };
        // Per-state text color: TITLE_BOT (Luna blue) at rest, INK on hover so blue text never
        // sits on the RAIL_HOVER light-blue wash (the contrast bug). Selected stays INK. The
        // RichText below intentionally does NOT pin `.color(...)`: a pinned color overrides the
        // per-state fg_stroke, so the hovered text would stay TITLE_BOT and stay unreadable.
        let fg = if selected { INK } else { TITLE_BOT };
        v.widgets.inactive.weak_bg_fill = face;
        v.widgets.inactive.bg_fill = face;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, fg);
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.inactive.corner_radius = r;
        v.widgets.hovered.weak_bg_fill = RAIL_HOVER;
        v.widgets.hovered.bg_fill = RAIL_HOVER;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.hovered.bg_stroke = Stroke::NONE;
        v.widgets.hovered.corner_radius = r;
        let txt = egui::RichText::new(label).strong();
        ui.add_sized([ui.available_width(), 22.0], egui::Button::new(txt))
    })
    .inner
}
