use godot::classes::Camera2D;
use godot::prelude::*;

pub(crate) fn update_shared_camera(
    cam: &mut Gd<Camera2D>,
    view: Vector2,
    subjects: &[Vector2],
    local: Vector2,
    bounds_min: Vector2,
    bounds_max: Vector2,
) {
    let Some(first) = subjects.first().copied() else {
        return;
    };
    let mut lo = first;
    let mut hi = lo;
    for p in subjects {
        lo = Vector2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vector2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let mid = (lo + hi) * 0.5;
    let aspect = if view.y > 0.0 { view.x / view.y } else { 1.78 };
    let portrait = aspect < 1.3;
    let focus = if portrait { local } else { mid };

    let span_x = (hi.x - lo.x) + 700.0;
    let span_y = (hi.y - lo.y) + 450.0;
    let (zmin, zmax) = if portrait { (0.9, 1.7) } else { (0.65, 1.35) };
    let fit = (view.x / span_x).min(view.y / span_y).clamp(zmin, zmax);
    let half_w = view.x * 0.5 / fit;
    let half_h = view.y * 0.5 / fit;
    let (cx0, cx1) = (bounds_min.x + half_w, bounds_max.x - half_w);
    let (cy0, cy1) = (bounds_min.y + half_h, bounds_max.y - half_h);
    let cx = if cx0 <= cx1 {
        focus.x.clamp(cx0, cx1)
    } else {
        (bounds_min.x + bounds_max.x) * 0.5
    };
    let cy = if cy0 <= cy1 {
        focus.y.clamp(cy0, cy1)
    } else {
        (bounds_min.y + bounds_max.y) * 0.5
    };
    let local_inside = local.x >= bounds_min.x
        && local.x <= bounds_max.x
        && local.y >= bounds_min.y
        && local.y <= bounds_max.y;
    let margin = 90.0_f32;
    let (cx, cy) = if local_inside {
        let (lx0, lx1) = (local.x - half_w + margin, local.x + half_w - margin);
        let (ly0, ly1) = (local.y - half_h + margin, local.y + half_h - margin);
        (
            if lx0 <= lx1 {
                cx.clamp(lx0, lx1)
            } else {
                local.x
            },
            if ly0 <= ly1 {
                cy.clamp(ly0, ly1)
            } else {
                local.y
            },
        )
    } else {
        (cx, cy)
    };
    let target = Vector2::new(cx, cy);
    let zoom = Vector2::splat(fit);
    let ease = 0.12;
    let next_position = cam.get_position().lerp(target, ease);
    let next_zoom = cam.get_zoom().lerp(zoom, ease);
    cam.set_position(next_position);
    cam.set_zoom(next_zoom);
}
