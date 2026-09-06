use egui::{Color32, CornerRadius, Margin, Stroke, Style, Visuals};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub surface: Color32,
    pub surface_alt: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub danger: Color32,
    pub border: Color32,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            surface: Color32::from_rgb(30, 34, 45),
            surface_alt: Color32::from_rgb(42, 47, 61),
            ink: Color32::from_rgb(235, 239, 248),
            muted: Color32::from_rgb(165, 175, 195),
            accent: Color32::from_rgb(92, 160, 255),
            danger: Color32::from_rgb(235, 96, 105),
            border: Color32::from_rgb(75, 87, 112),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub palette: Palette,
    pub spacing: f32,
    pub radius: u8,
    pub stroke_width: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            spacing: 8.0,
            radius: 6,
            stroke_width: 1.0,
        }
    }
}

impl Theme {
    pub fn frame(self, fill: Color32) -> egui::Frame {
        egui::Frame::NONE
            .fill(fill)
            .inner_margin(Margin::same(self.spacing as i8))
            .corner_radius(CornerRadius::same(self.radius))
            .stroke(Stroke::new(self.stroke_width, self.palette.border))
    }

    pub fn install(self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();
        visuals.override_text_color = Some(self.palette.ink);
        visuals.widgets.inactive.bg_fill = self.palette.surface_alt;
        visuals.widgets.hovered.bg_fill = self.palette.accent;
        visuals.widgets.active.bg_fill = self.palette.accent;
        ctx.set_visuals(visuals);
    }
}

impl From<Theme> for Style {
    fn from(theme: Theme) -> Self {
        let mut style = Style::default();
        style.spacing.item_spacing = egui::vec2(theme.spacing, theme.spacing);
        style
    }
}
