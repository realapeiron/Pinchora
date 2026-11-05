use crate::{
    style::{Frame, Opacity},
    svg::{panel, panel_alpha},
};

/// Card palette. Colors are `#rrggbb`.
#[derive(Debug, Clone)]
pub struct Theme {
    pub bg_top: String,
    pub bg_bottom: String,
    pub panel: String,
    pub panel_edge: String,
    pub ink: String,
    pub muted: String,
    pub accent: String,
    pub gold: String,
    pub mint: String,
    pub heart: String,
    /// How much of the background shows through panels, plates and the veil.
    pub opacity: Opacity,
    /// The frame drawn around the whole card.
    pub frame: Frame,
}

impl Default for Theme {
    fn default() -> Self {
        Self::reef()
    }
}

impl Theme {
    /// Night-sea navy with coral, Pinchora's house style.
    pub fn reef() -> Self {
        Self {
            bg_top: "#1d2b53".into(),
            bg_bottom: "#0f1631".into(),
            panel: "#141b3d".into(),
            panel_edge: "#2e3d75".into(),
            ink: "#fff1e8".into(),
            muted: "#8fa0d8".into(),
            accent: "#ff6b4a".into(),
            gold: "#ffd166".into(),
            mint: "#06d6a0".into(),
            heart: "#ff4d6d".into(),
            opacity: Opacity::Normal,
            frame: Frame::Plain,
        }
    }

    /// Named presets for server-designed cards.
    pub fn preset(name: &str) -> Self {
        let base = Self::reef();
        let with = |top: &str, bottom: &str, panel: &str, edge: &str, accent: &str| Self {
            bg_top: top.into(),
            bg_bottom: bottom.into(),
            panel: panel.into(),
            panel_edge: edge.into(),
            accent: accent.into(),
            ..base.clone()
        };
        match name {
            "sunset" => with("#7a2a3a", "#2b1330", "#3a1838", "#c2413f", "#ffd166"),
            "forest" => with("#1f4d3a", "#0e2419", "#173a2b", "#2f7d4f", "#a6e39a"),
            "candy" => with("#9b6bc4", "#5a3a7a", "#4a2f66", "#ff8fb1", "#fff1e8"),
            "midnight" => with("#0f1631", "#05070f", "#0b1029", "#2e3d75", "#8fd3ff"),
            _ => base,
        }
    }

    pub const PRESETS: [&'static str; 5] = ["reef", "sunset", "forest", "candy", "midnight"];

    pub fn with_accent(mut self, rgb: Option<u32>) -> Self {
        if let Some(rgb) = rgb {
            self.accent = format!("#{:06x}", rgb & 0xff_ffff);
        }
        self
    }

    /// The card's chosen look: the opacity of its panels and the frame around it.
    pub fn with_style(mut self, opacity: Opacity, frame: Frame) -> Self {
        self.opacity = opacity;
        self.frame = frame;
        self
    }

    /// A panel of the card: the panel colour in its edge, see-through as far as the opacity says.
    pub(crate) fn panel(&self, out: &mut String, x: f32, y: f32, w: f32, h: f32, border: f32) {
        match self.opacity.panel() {
            alpha if alpha < 1.0 => panel_alpha(out, x, y, w, h, border, &self.panel, &self.panel_edge, alpha),
            _ => panel(out, x, y, w, h, border, &self.panel, &self.panel_edge),
        }
    }

    /// A plate behind text that sits on a background picture.
    pub(crate) fn plate(&self, out: &mut String, x: f32, y: f32, w: f32, h: f32) {
        crate::svg::plate(out, x, y, w, h, self.opacity.plate());
    }

    /// Draws the card's frame over everything else; `w` and `h` are the card's size.
    pub(crate) fn draw_frame(&self, out: &mut String, w: f32, h: f32) {
        crate::cards::draw_frame(out, self.frame, w, h, &self.accent);
    }
}
