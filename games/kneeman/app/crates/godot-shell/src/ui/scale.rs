//! Pure helpers that counter `project.godot`'s `window/stretch/mode="canvas_items"`. That mode
//! scales EVERY CanvasItem -- including the egui bridge's own `CanvasLayer` -- by the ratio
//! between the real window size and the design canvas (`window/size/viewport_width|height`,
//! 1600x900). Left alone, a 12px egui label renders at `real_px/1600 * 12` on screen: on a
//! 3400px-wide window that's ~25px, and the XP window (sized in the same inflated units) fills
//! nearly the whole screen. See plans/menu-responsive-remap.md #1 for the diagnosis.
//!
//! The fix: read the real window size (the Godot side knows it; ui/debug.rs is the only caller,
//! see its `process()`), compute how hard `canvas_items` is stretching, and set egui's
//! `pixels_per_point` to the inverse. Egui lays out and rasterizes in "points"; Godot's stretch
//! then scales that raster by the same factor again before it reaches the screen, so shrinking
//! `pixels_per_point` by the stretch factor cancels it -- a widget sized in points lands at a
//! stable *physical* size no matter how big the window gets. `project.godot` itself is never
//! touched; the 1600x900 design size is mirrored here as a constant.
//!
//! These are plain functions over plain numbers (no `egui::Context`, no Godot types) so they're
//! cargo-testable without a running engine, matching the `ws_scheme`-style pure-fn pattern used
//! elsewhere in this shell (see rtc.rs).

/// The design canvas `project.godot` stretches to fill the real window
/// (`window/size/viewport_width` / `viewport_height`). Mirrored here, not read from the file.
pub const DESIGN_WIDTH: f32 = 1600.0;
pub const DESIGN_HEIGHT: f32 = 900.0;

/// `pixels_per_point` is clamped to this band so a transient bad readout (a minimized window, a
/// mid-resize zero frame, or a monitor absurdly larger than the design canvas) can't make the
/// whole egui layer vanish (huge ppp -> everything shrinks to nothing) or blow up (tiny ppp ->
/// everything balloons past the clamp this module exists to prevent).
const MIN_PPP: f32 = 0.35;
const MAX_PPP: f32 = 3.0;

/// How hard `canvas_items` + aspect `expand` is scaling every CanvasItem, given the real window
/// size and the design size in the same units. `expand` scales uniformly by the SMALLER of the
/// two axis ratios -- the other axis just reveals extra design-space canvas instead of
/// distorting -- so this mirrors that: `min(real.x/design.x, real.y/design.y)`. Degenerate input
/// (a zero-size window, still mid-resize) falls back to `1.0` (no correction) rather than
/// dividing by zero.
pub fn stretch_factor(real: (f32, f32), design: (f32, f32)) -> f32 {
    if real.0 <= 0.0 || real.1 <= 0.0 || design.0 <= 0.0 || design.1 <= 0.0 {
        return 1.0;
    }
    (real.0 / design.0).min(real.1 / design.1)
}

/// The egui `pixels_per_point` that cancels a given stretch factor (see the module doc), times
/// the display's own hiDPI scale, clamped to [`MIN_PPP`]..=[`MAX_PPP`] so a degenerate reading
/// can't make the counter-scale runaway in either direction.
///
/// Why `dpr` matters (2026-07-04 "menu so tiny"): the window size the stretch factor is
/// computed from is PHYSICAL pixels, so on a retina/hiDPI display the stretch silently
/// includes devicePixelRatio -- inverting it landed a 12pt label at 12 physical px = 6 logical
/// px, half size. Multiplying the inverse by the display scale lands a point at one LOGICAL
/// pixel instead: `ppp * stretch = dpr` physical px per point, stable across window resizes
/// AND monitor densities. Pass 1.0 on platforms that don't report a scale.
pub fn counter_scale_ppp(stretch: f32, dpr: f32) -> f32 {
    if stretch <= 0.0 {
        return 1.0;
    }
    let dpr = if dpr > 0.0 { dpr } else { 1.0 };
    (dpr / stretch).clamp(MIN_PPP, MAX_PPP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretch_factor_at_design_size_is_identity() {
        assert_eq!(stretch_factor((1600.0, 900.0), (1600.0, 900.0)), 1.0);
    }

    #[test]
    fn stretch_factor_uses_the_tighter_axis() {
        // 3400x1800: x ratio 2.125, y ratio 2.0 -- expand-mode scale is the smaller one.
        let s = stretch_factor((3400.0, 1800.0), (DESIGN_WIDTH, DESIGN_HEIGHT));
        assert!((s - 2.0).abs() < 1e-4, "got {s}");
    }

    #[test]
    fn stretch_factor_below_design_size_shrinks() {
        // A window smaller than the design canvas has stretch < 1 (canvas_items shrinks it).
        let s = stretch_factor((800.0, 450.0), (DESIGN_WIDTH, DESIGN_HEIGHT));
        assert!((s - 0.5).abs() < 1e-4, "got {s}");
    }

    #[test]
    fn stretch_factor_degenerate_input_falls_back_to_identity() {
        assert_eq!(
            stretch_factor((0.0, 900.0), (DESIGN_WIDTH, DESIGN_HEIGHT)),
            1.0
        );
        assert_eq!(
            stretch_factor((1600.0, 0.0), (DESIGN_WIDTH, DESIGN_HEIGHT)),
            1.0
        );
    }

    #[test]
    fn counter_scale_is_the_inverse_of_stretch() {
        assert!((counter_scale_ppp(2.0, 1.0) - 0.5).abs() < 1e-6);
        assert!((counter_scale_ppp(0.5, 1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn counter_scale_folds_the_display_scale_back_in() {
        // Retina (dpr 2): the physical-px stretch factor includes dpr, so the inverse alone
        // would land points at half logical size (the "menu so tiny" bug). dpr/stretch keeps
        // a point at one logical pixel.
        assert!((counter_scale_ppp(2.0, 2.0) - 1.0).abs() < 1e-6);
        // 3024x1964 physical on a 1600x900 design (a 1512x982 css window at dpr 2).
        let s = stretch_factor((3024.0, 1964.0), (DESIGN_WIDTH, DESIGN_HEIGHT));
        let ppp = counter_scale_ppp(s, 2.0);
        assert!(
            (ppp * s - 2.0).abs() < 1e-4,
            "one point = one logical px = dpr physical px"
        );
    }

    #[test]
    fn counter_scale_clamps_huge_stretch() {
        // A monitor absurdly larger than the design canvas would want ppp << MIN_PPP; clamp it.
        assert_eq!(counter_scale_ppp(100.0, 1.0), MIN_PPP);
    }

    #[test]
    fn counter_scale_clamps_tiny_stretch() {
        // A window much smaller than the design canvas would want ppp >> MAX_PPP; clamp it.
        assert_eq!(counter_scale_ppp(0.01, 1.0), MAX_PPP);
    }

    #[test]
    fn counter_scale_degenerate_inputs_are_identity_or_unscaled() {
        assert_eq!(counter_scale_ppp(0.0, 2.0), 1.0);
        // a zero/negative dpr readout falls back to the plain inverse, not a vanishing layer
        assert!((counter_scale_ppp(2.0, 0.0) - 0.5).abs() < 1e-6);
    }
}
