//! On-screen touch gamepad: GameCube-proportioned diamond + floating stick. Layout
//! geometry, thread-local input state (read by `input` and `sample_input`), and the
//! build/update passes over the Control nodes.

use godot::classes::{Button, CanvasLayer, Label, Panel, Polygon2D, StyleBoxFlat};
use godot::prelude::*;

use super::KneeMan;

// --- On-screen touch gamepad (mobile), GameCube-proportioned. Everything is laid out from the LIVE
// viewport size (not fixed design coords) so the cluster anchors to the bottom corners under the thumbs
// no matter the aspect (portrait `aspect=expand` balloons height). The stick (left) drives dir/aim_y;
// the buttons drive the same Input actions the keyboard/pad use. Hit-tests read the same resolved
// layout the visuals use (TOUCH_LAYOUT), so touch + visual never drift. --

/// One quadrant of the diamond face cluster: the wedge meeting at the center, pointing one cardinal
/// way (top/left/right/bottom). The whole wedge is the hit area. `actions` are the Input actions a
/// press drives -- usually one, but the TOP wedge is multi-loaded (grab + shield) so a tap grabs / Z
/// / drops and a hold guards (and rolls/spotdodges with a stick tilt, air-dodges in the air).
pub(super) struct Quad {
    pub(super) actions: &'static [&'static str],
    pub(super) letter: &'static str,
    pub(super) color: (f32, f32, f32),
}

/// Diamond wedges, indexed by `Dir` (Top, Left, Right, Bottom). Top is the multi-loaded
/// grab/guard/Z/drop/dodge wedge; left attack, right special, bottom jump. Colors: purple guard,
/// green attack, red special, grey jump.
pub(super) const QUADS: [Quad; 4] = [
    Quad {
        actions: &["grab", "shield"],
        letter: "Z\nGUARD",
        color: (0.62, 0.42, 0.86),
    }, // Top
    Quad {
        actions: &["attack"],
        letter: "A",
        color: (0.36, 0.82, 0.45),
    }, // Left
    Quad {
        actions: &["special"],
        letter: "B",
        color: (0.90, 0.30, 0.30),
    }, // Right
    Quad {
        actions: &["jump"],
        letter: "JUMP",
        color: (0.86, 0.88, 0.93),
    }, // Bottom
];
pub(super) const DIR_TOP: usize = 0;
pub(super) const DIR_LEFT: usize = 1;
pub(super) const DIR_RIGHT: usize = 2;
pub(super) const DIR_BOTTOM: usize = 3;

/// Resolved diamond geometry in live screen coords for one frame. `input` hit-tests against this:
/// a point is in the diamond when `|dx|+|dy| <= radius` (L1), and the wedge is whichever axis
/// dominates. The shorthop is a plain rect below the bottom tip.
#[derive(Clone, Copy)]
pub(super) struct TouchLayout {
    pub(super) center: Vector2,
    pub(super) radius: f32, // center-to-tip (half-diagonal of the rotated square)
    pub(super) shorthop: Rect2,
    pub(super) stick_center: Vector2,
    pub(super) stick_radius: f32,
    pub(super) stick_zone_x: f32, // touches with screen-x below this (left side) grab the stick
    pub(super) cstick_park: Vector2, // where the idle c-stick visual sits (left of the diamond)
    pub(super) cstick_radius: f32, // full-tilt throw distance -- smaller than the move stick (flicks)
}

/// Which wedge a screen point falls in, or None if outside the diamond. Matches the Polygon2D tiling
/// exactly (the wedges are split by the 45° lines through the center).
pub(super) fn quad_at(p: Vector2, center: Vector2, radius: f32) -> Option<usize> {
    let d = p - center;
    if d.x.abs() + d.y.abs() > radius {
        return None;
    }
    Some(if d.y.abs() >= d.x.abs() {
        if d.y < 0.0 { DIR_TOP } else { DIR_BOTTOM }
    } else if d.x < 0.0 {
        DIR_LEFT
    } else {
        DIR_RIGHT
    })
}

/// The 4 vertices (relative to center) of one wedge polygon: center, two edge-midpoints, the tip.
/// `r` is center-to-tip. These four wedges tile the diamond perfectly and match `quad_at`.
pub(super) fn quad_poly(dir: usize, r: f32) -> [Vector2; 4] {
    let h = r * 0.5;
    match dir {
        DIR_TOP => [
            Vector2::ZERO,
            Vector2::new(h, -h),
            Vector2::new(0.0, -r),
            Vector2::new(-h, -h),
        ],
        DIR_BOTTOM => [
            Vector2::ZERO,
            Vector2::new(h, h),
            Vector2::new(0.0, r),
            Vector2::new(-h, h),
        ],
        DIR_LEFT => [
            Vector2::ZERO,
            Vector2::new(-h, -h),
            Vector2::new(-r, 0.0),
            Vector2::new(-h, h),
        ],
        _ => [
            Vector2::ZERO,
            Vector2::new(h, -h),
            Vector2::new(r, 0.0),
            Vector2::new(h, h),
        ],
    }
}

/// Label anchor (center of the wedge) relative to the diamond center.
pub(super) fn quad_label_pos(dir: usize, r: f32) -> Vector2 {
    let k = r * 0.52;
    match dir {
        DIR_TOP => Vector2::new(0.0, -k),
        DIR_BOTTOM => Vector2::new(0.0, k),
        DIR_LEFT => Vector2::new(-k, 0.0),
        _ => Vector2::new(k, 0.0),
    }
}

/// Build the layout from the current viewport. `u` scales to the SHORTER screen edge (thumb-sized in
/// any aspect). The diamond anchors to the bottom-right, lifted clear of the reserved HUD strip; the
/// shorthop rect sits just under its bottom tip; the stick floats on the left.
pub(super) fn touch_layout(view: Vector2) -> TouchLayout {
    // Thumb-sized to the short edge, but also capped by the available width so the left stick and the
    // right diamond never collide into a centered clump (the portrait/narrow "smushed in the middle"
    // bug). The two clusters then hug their own screen edge with an equal margin = space-around.
    // Width budget: stick spans ~3.7u from the left, diamond ~5.7u from the right; >=9.6u keeps a gap.
    let u = (view.x.min(view.y) * 0.105)
        .min(view.x / 9.6)
        .clamp(44.0, 150.0);
    let radius = u * 2.6;
    let hud_clear = 150.0; // bottom strip reserved for the % HUD / menu button + shorthop
    let sh_h = u * 0.9; // shorthop rect height
    let cy = view.y - hud_clear - sh_h - radius; // diamond center, lifted so the bottom tip + rect clear the HUD
    let side = u * 0.5; // equal breathing room from each screen edge
    let cx = view.x - radius - side; // diamond hugs the right edge
    let center = Vector2::new(cx, cy);
    let sh_w = radius * 1.3;
    let shorthop = Rect2::new(
        Vector2::new(cx - sh_w * 0.5, cy + radius + u * 0.15),
        Vector2::new(sh_w, sh_h),
    );
    let stick_radius = u * 1.6;
    TouchLayout {
        center,
        radius,
        shorthop,
        stick_center: Vector2::new(side + stick_radius, cy), // stick hugs the left edge, mirror of the diamond
        stick_radius,
        stick_zone_x: view.x * 0.46,
        // parked just left of the diamond's left tip, dropped toward the bottom tip -- the GC
        // c-stick's below-left-of-the-face-buttons spot. It FLOATS like the move stick: any
        // right-side touch that misses the buttons grabs it wherever the thumb lands.
        cstick_park: Vector2::new(cx - radius - u * 1.2, cy + radius * 0.55),
        cstick_radius: u * 1.0, // tighter throw than the move stick: a smash is a flick, not a cruise
    }
}

/// A round Panel (corner radius huge so it stays circular/stadium at any later resize), used for the
/// stick + face buttons. Visuals are repositioned/resized every frame in `update_touch`.
fn circle_panel(d: f32, fill: Color, border: Color) -> Gd<Panel> {
    let mut p = Panel::new_alloc();
    p.set_size(Vector2::splat(d));
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(fill);
    sb.set_corner_radius_all(400); // >= any radius we use -> always a circle/pill
    sb.set_border_width_all(3);
    sb.set_border_color(border);
    p.add_theme_stylebox_override("panel", &sb);
    p
}

/// Resize a Panel to diameter `d` and center it on `c` (Control positions are top-left).
fn place_circle(p: &mut Gd<Panel>, c: Vector2, d: f32) {
    p.set_size(Vector2::splat(d));
    p.set_position(c - Vector2::splat(d * 0.5));
}

thread_local! {
    /// Analog stick output in [-1,1] per axis, written by the touch handler, read by `sample_input`.
    pub(super) static TOUCH_STICK: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
    /// Finger index that owns the stick (-1 = none) + its screen origin for floating-stick math.
    pub(super) static TOUCH_FINGER: std::cell::Cell<i64> = const { std::cell::Cell::new(-1) };
    pub(super) static TOUCH_ORIGIN: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
    /// Full-tilt throw distance for the stick in px (scales with viewport; set each frame).
    pub(super) static TOUCH_STICK_RAD: std::cell::Cell<f32> = const { std::cell::Cell::new(95.0) };
    /// Fingers currently holding a wedge: (finger_index, actions) so multi-touch releases the right
    /// Input actions (the top wedge presses two; the rest one).
    pub(super) static TOUCH_BTNS: std::cell::RefCell<Vec<(i64, &'static [&'static str])>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Resolved diamond geometry for this frame; `input` hit-tests against it. None until first frame.
    pub(super) static TOUCH_DIAMOND: std::cell::Cell<Option<TouchLayout>> =
        const { std::cell::Cell::new(None) };
    pub(super) static TOUCH_STICK_ZONE_X: std::cell::Cell<f32> = const { std::cell::Cell::new(736.0) };
    /// C-stick output in [-1,1] per axis (drives `cx`/`cy`: smash flicks + helm aim), written by the
    /// touch handler, read by `sample_input` -- the right-thumb mirror of TOUCH_STICK.
    pub(super) static TOUCH_CSTICK: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
    /// Finger index that owns the c-stick (-1 = none) + its screen origin for floating-stick math.
    pub(super) static TOUCH_CSTICK_FINGER: std::cell::Cell<i64> = const { std::cell::Cell::new(-1) };
    pub(super) static TOUCH_CSTICK_ORIGIN: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
    /// Full-tilt throw distance for the c-stick in px (scales with viewport; set each frame).
    pub(super) static TOUCH_CSTICK_RAD: std::cell::Cell<f32> = const { std::cell::Cell::new(60.0) };
}

impl KneeMan {
    /// Build the on-screen touch gamepad: the floating analog stick (left) and the GameCube face
    /// cluster (right). Visuals only, built once; `update_touch` lays them out against the live
    /// viewport every frame and the `input` handler drives the sim. (Joining a match is the status
    /// chip's job now, top-left.)
    pub(super) fn build_touch_ui(&mut self) {
        let mut layer = CanvasLayer::new_alloc();
        layer.set_layer(50); // above the world, below the egui debug panel

        // MENU tab: bottom-center, between the stick + face cluster. Opens the debug panel + pauses.
        let mut menu = Button::new_alloc();
        menu.set_text("☰ MENU");
        menu.add_theme_font_size_override("font_size", 28);
        let mcb = self.to_gd();
        menu.connect("pressed", &Callable::from_object_method(&mcb, "on_menu"));
        layer.add_child(&menu);
        self.menu_btn = Some(menu);

        // Floating stick: a faint ring + grey GameCube-ish knob, parked bottom-left until grabbed.
        let base = circle_panel(
            120.0,
            Color::from_rgba(0.1, 0.12, 0.18, 0.32),
            Color::from_rgba(0.8, 0.85, 1.0, 0.45),
        );
        let knob = circle_panel(
            80.0,
            Color::from_rgba(0.62, 0.66, 0.74, 0.7),
            Color::from_rgba(0.9, 0.93, 1.0, 0.85),
        );
        layer.add_child(&base);
        layer.add_child(&knob);
        self.stick_base = Some(base);
        self.stick_knob = Some(knob);

        // Floating c-stick: GameCube-yellow ring + nub, parked below-left of the diamond until a
        // right-side touch that misses the buttons grabs it. Drives cx/cy (smash flicks, helm aim).
        let cbase = circle_panel(
            80.0,
            Color::from_rgba(0.35, 0.30, 0.08, 0.32),
            Color::from_rgba(0.95, 0.85, 0.30, 0.45),
        );
        let cknob = circle_panel(
            56.0,
            Color::from_rgba(0.93, 0.78, 0.18, 0.75),
            Color::from_rgba(1.0, 0.92, 0.45, 0.9),
        );
        layer.add_child(&cbase);
        layer.add_child(&cknob);
        self.cstick_base = Some(cbase);
        self.cstick_knob = Some(cknob);

        // Diamond face cluster: a Polygon2D wedge + centered Label per quadrant. Polygons are
        // re-pointed each frame in update_touch (geometry is viewport-relative); here we just create
        // the nodes and color them. Labels are screen-space Controls positioned each frame.
        for q in &QUADS {
            let (r, g, bl) = q.color;
            let mut poly = Polygon2D::new_alloc();
            poly.set_color(Color::from_rgba(r, g, bl, 0.9));
            layer.add_child(&poly);
            self.quad_polys.push(poly);

            let mut lbl = Label::new_alloc();
            lbl.set_text(q.letter);
            lbl.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
            lbl.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
            lbl.add_theme_color_override("font_color", Color::from_rgb(0.06, 0.06, 0.08));
            layer.add_child(&lbl);
            self.quad_labels.push(lbl);
        }

        // Shorthop rectangle under the bottom (jump) tip.
        let mut sh = Panel::new_alloc();
        let mut sb = StyleBoxFlat::new_gd();
        sb.set_bg_color(Color::from_rgba(0.55, 0.60, 0.70, 0.9));
        sb.set_corner_radius_all(16);
        sb.set_border_width_all(3);
        sb.set_border_color(Color::from_rgba(0.0, 0.0, 0.0, 0.35));
        sh.add_theme_stylebox_override("panel", &sb);
        let mut shl = Label::new_alloc();
        shl.set_text("SHORTHOP");
        shl.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        shl.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        shl.add_theme_color_override("font_color", Color::from_rgb(0.06, 0.06, 0.08));
        sh.add_child(&shl);
        layer.add_child(&sh);
        self.shorthop_panel = Some(sh);
        self.shorthop_label = Some(shl);

        self.base_mut().add_child(&layer);
    }

    /// Per-frame: resolve the GameCube layout against the live viewport (anchors to the bottom
    /// corners under the thumbs), position/size every widget, publish hitboxes for `input`, and
    /// float the stick visual at the active finger.
    pub(super) fn update_touch(&mut self) {
        let view = self.base().get_viewport_rect().size;
        let lay = touch_layout(view);

        // Only show the on-screen gamepad when the device actually needs it: a touchscreen is present
        // AND no controller is paired. Desktop (no touchscreen) or "touchscreen + gamepad" hides it,
        // so it never clutters a keyboard/pad session. Also hide it whenever the pause menu is open,
        // so the stick/buttons don't sit on top of the menu. Hidden -> no diamond published, so a
        // stray touch can't fire a button either.
        let menu_open = self
            .debug_ui
            .as_ref()
            .map(|d| d.bind().is_menu_open())
            .unwrap_or(false);
        let show_touch = godot::classes::DisplayServer::singleton().is_touchscreen_available()
            && !crate::controls::gamepad_connected()
            && !menu_open;
        for poly in self.quad_polys.iter_mut() {
            poly.set_visible(show_touch);
        }
        for label in self.quad_labels.iter_mut() {
            label.set_visible(show_touch);
        }
        if let Some(p) = self.shorthop_panel.as_mut() {
            p.set_visible(show_touch);
        }
        if let Some(l) = self.shorthop_label.as_mut() {
            l.set_visible(show_touch);
        }
        if let Some(b) = self.stick_base.as_mut() {
            b.set_visible(show_touch);
        }
        if let Some(k) = self.stick_knob.as_mut() {
            k.set_visible(show_touch);
        }
        if let Some(b) = self.cstick_base.as_mut() {
            b.set_visible(show_touch);
        }
        if let Some(k) = self.cstick_knob.as_mut() {
            k.set_visible(show_touch);
        }
        if !show_touch {
            TOUCH_DIAMOND.set(None);
            // still update the menu button below, then bail out of the gamepad layout.
            if let Some(menu) = self.menu_btn.as_mut() {
                let w = (view.x * 0.16).clamp(150.0, 340.0);
                let h = 60.0_f32.max(view.y.min(view.x) * 0.06);
                menu.set_size(Vector2::new(w, h));
                menu.set_position(Vector2::new(view.x * 0.5 - w * 0.5, view.y - h - 12.0));
            }
            return;
        }

        // publish for the input handler's hit-tests
        TOUCH_DIAMOND.set(Some(lay));
        TOUCH_STICK_RAD.set(lay.stick_radius);
        TOUCH_STICK_ZONE_X.set(lay.stick_zone_x);
        TOUCH_CSTICK_RAD.set(lay.cstick_radius);

        // diamond wedges: re-point each polygon to its wedge, place + size each label at the wedge
        // centroid. Font scales with the diamond so it reads on any screen.
        let font = (lay.radius * 0.22) as i32;
        for i in 0..QUADS.len() {
            if let Some(poly) = self.quad_polys.get_mut(i) {
                let pts: Vec<Vector2> = quad_poly(i, lay.radius).into_iter().collect();
                poly.set_position(lay.center);
                poly.set_polygon(&PackedVector2Array::from(pts.as_slice()));
            }
            if let Some(label) = self.quad_labels.get_mut(i) {
                let lp = lay.center + quad_label_pos(i, lay.radius);
                let half = lay.radius * 0.5;
                label.set_size(Vector2::new(lay.radius, half));
                label.set_position(lp - Vector2::new(lay.radius * 0.5, half * 0.5));
                label.add_theme_font_size_override("font_size", font);
            }
        }
        // shorthop rectangle
        if let Some(panel) = self.shorthop_panel.as_mut() {
            panel.set_position(lay.shorthop.position);
            panel.set_size(lay.shorthop.size);
        }
        if let Some(label) = self.shorthop_label.as_mut() {
            label.set_size(lay.shorthop.size);
            label.add_theme_font_size_override("font_size", (lay.shorthop.size.y * 0.5) as i32);
        }

        // floating stick
        let active = TOUCH_FINGER.get() >= 0;
        let (sx, sy) = TOUCH_STICK.get();
        let origin = if active {
            let (ox, oy) = TOUCH_ORIGIN.get();
            Vector2::new(ox, oy)
        } else {
            lay.stick_center
        };
        if let Some(base) = self.stick_base.as_mut() {
            place_circle(base, origin, lay.stick_radius * 2.0);
        }
        if let Some(knob) = self.stick_knob.as_mut() {
            place_circle(
                knob,
                origin + Vector2::new(sx, sy) * lay.stick_radius,
                lay.stick_radius * 0.9,
            );
        }
        // floating c-stick (yellow), mirror of the move stick above
        let c_active = TOUCH_CSTICK_FINGER.get() >= 0;
        let (cx, cy) = TOUCH_CSTICK.get();
        let c_origin = if c_active {
            let (ox, oy) = TOUCH_CSTICK_ORIGIN.get();
            Vector2::new(ox, oy)
        } else {
            lay.cstick_park
        };
        if let Some(base) = self.cstick_base.as_mut() {
            place_circle(base, c_origin, lay.cstick_radius * 2.0);
        }
        if let Some(knob) = self.cstick_knob.as_mut() {
            place_circle(
                knob,
                c_origin + Vector2::new(cx, cy) * lay.cstick_radius,
                lay.cstick_radius * 0.9,
            );
        }
        // MENU tab pinned to the very bottom-center, in the HUD strip between the two clusters.
        if let Some(menu) = self.menu_btn.as_mut() {
            let w = (view.x * 0.16).clamp(150.0, 340.0);
            let h = 60.0_f32.max(view.y.min(view.x) * 0.06);
            menu.set_size(Vector2::new(w, h));
            menu.set_position(Vector2::new(view.x * 0.5 - w * 0.5, view.y - h - 12.0));
        }
    }
}
