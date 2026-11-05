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

    pub fn with_accent(mut self, rgb: Option<u32>) -> Self {
        if let Some(rgb) = rgb {
            self.accent = format!("#{:06x}", rgb & 0xff_ffff);
        }
        self
    }
}
