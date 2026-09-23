use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FitMode {
    #[default]
    Cover,
    Original,
    Tile,
}

impl FitMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "cover" => Some(Self::Cover),
            "original" => Some(Self::Original),
            "tile" => Some(Self::Tile),
            _ => None,
        }
    }

    /// `IdeBackgroundUtil.Fill`: SCALE keeps the aspect ratio and crops to cover.
    pub fn intellij(self) -> &'static str {
        match self {
            Self::Cover => "SCALE",
            Self::Original => "PLAIN",
            Self::Tile => "TILE",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    #[default]
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "top-left" => Some(Self::TopLeft),
            "top-center" => Some(Self::TopCenter),
            "top-right" => Some(Self::TopRight),
            "middle-left" => Some(Self::MiddleLeft),
            "center" => Some(Self::Center),
            "middle-right" => Some(Self::MiddleRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-center" => Some(Self::BottomCenter),
            "bottom-right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub fn intellij(self) -> &'static str {
        match self {
            Self::TopLeft => "TOP_LEFT",
            Self::TopCenter => "TOP_CENTER",
            Self::TopRight => "TOP_RIGHT",
            Self::MiddleLeft => "MIDDLE_LEFT",
            Self::Center => "CENTER",
            Self::MiddleRight => "MIDDLE_RIGHT",
            Self::BottomLeft => "BOTTOM_LEFT",
            Self::BottomCenter => "BOTTOM_CENTER",
            Self::BottomRight => "BOTTOM_RIGHT",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplaySettings {
    pub fit: FitMode,
    pub anchor: Anchor,
    pub opacity: f64,
    pub blur: f64,
    pub overlay_color: [u8; 3],
    pub overlay_opacity: f64,
    pub text_contrast: f64,
    pub editor_enabled: bool,
    pub frame_enabled: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            fit: FitMode::Cover,
            anchor: Anchor::Center,
            opacity: 0.2,
            blur: 0.0,
            overlay_color: [0x10, 0x14, 0x16],
            overlay_opacity: 0.0,
            text_contrast: 0.5,
            editor_enabled: true,
            frame_enabled: true,
        }
    }
}

impl DisplaySettings {
    /// Expects a display object that already passed `protocol::validate_display`.
    pub fn from_validated(display: &serde_json::Map<String, Value>) -> Self {
        let defaults = Self::default();
        let number =
            |key: &str, fallback: f64| display.get(key).and_then(Value::as_f64).unwrap_or(fallback);
        let boolean = |key: &str, fallback: bool| {
            display
                .get(key)
                .and_then(Value::as_bool)
                .unwrap_or(fallback)
        };
        Self {
            fit: display
                .get("fit")
                .and_then(Value::as_str)
                .and_then(FitMode::parse)
                .unwrap_or(defaults.fit),
            anchor: display
                .get("anchor")
                .and_then(Value::as_str)
                .and_then(Anchor::parse)
                .unwrap_or(defaults.anchor),
            opacity: number("opacity", defaults.opacity),
            blur: number("blur", defaults.blur),
            overlay_color: display
                .get("overlayColor")
                .and_then(Value::as_str)
                .and_then(parse_hex_color)
                .unwrap_or(defaults.overlay_color),
            overlay_opacity: number("overlayOpacity", defaults.overlay_opacity),
            text_contrast: number("textContrast", defaults.text_contrast),
            editor_enabled: boolean("editorEnabled", defaults.editor_enabled),
            frame_enabled: boolean("frameEnabled", defaults.frame_enabled),
        }
    }

    /// IntelliJ takes the alpha as an integer percentage.
    pub fn opacity_percent(&self) -> u8 {
        (self.opacity * 100.0).round().clamp(0.0, 100.0) as u8
    }

    /// The bridge takes the editor text boost as an integer percentage.
    pub fn text_contrast_percent(&self) -> u8 {
        (self.text_contrast * 100.0).round().clamp(0.0, 100.0) as u8
    }
}

pub fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    if value.len() != 7 || !value.starts_with('#') {
        return None;
    }
    let channel = |index: usize| u8::from_str_radix(&value[index..index + 2], 16).ok();
    Some([channel(1)?, channel(3)?, channel(5)?])
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub phase: String,
    pub message: String,
    pub active_targets: u32,
}

impl RuntimeStatus {
    pub fn new(phase: &str, message: impl Into<String>, active_targets: u32) -> Self {
        Self {
            phase: phase.to_string(),
            message: message.into(),
            active_targets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_display_to_intellij_values() {
        let display = json!({
            "fit": "tile",
            "anchor": "bottom-right",
            "opacity": 0.345,
            "overlayColor": "#FF8000",
            "textContrast": 0.726,
            "frameEnabled": false
        });
        let settings = DisplaySettings::from_validated(display.as_object().unwrap());
        assert_eq!(settings.fit.intellij(), "TILE");
        assert_eq!(settings.anchor.intellij(), "BOTTOM_RIGHT");
        assert_eq!(settings.opacity_percent(), 35);
        assert_eq!(settings.text_contrast_percent(), 73);
        assert_eq!(settings.overlay_color, [0xff, 0x80, 0x00]);
        assert!(settings.editor_enabled);
        assert!(!settings.frame_enabled);
    }

    #[test]
    fn defaults_fill_missing_fields() {
        let settings = DisplaySettings::from_validated(&serde_json::Map::new());
        assert_eq!(settings, DisplaySettings::default());
        assert_eq!(settings.fit.intellij(), "SCALE");
        assert_eq!(settings.opacity_percent(), 20);
        assert_eq!(settings.text_contrast_percent(), 50);
    }
}
