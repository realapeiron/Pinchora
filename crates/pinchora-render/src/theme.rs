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
}
