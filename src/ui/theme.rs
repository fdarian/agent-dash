use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub primary: Color,
    pub border_unfocused: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_subtle: Color,
    pub text_muted: Color,
    pub bg_selected: Color,
    pub selected_fg: Color,
    pub selected_dim_fg: Color,
    pub bg_subtle: Color,
    pub accent_warning: Color,
    pub accent_info: Color,
    pub accent_flag: Color,
    pub search_match_bg: Color,
    pub search_match_fg: Color,
    pub search_current_bg: Color,
    pub search_current_fg: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
}

impl Palette {
    pub fn dark() -> Self {
        Palette {
            primary: Color::Rgb(0xD9, 0x77, 0x57),
            border_unfocused: Color::Rgb(0x66, 0x66, 0x66),
            text: Color::Rgb(0xCC, 0xCC, 0xCC),
            text_dim: Color::Rgb(0xAA, 0xAA, 0xAA),
            text_subtle: Color::Rgb(0x88, 0x88, 0x88),
            text_muted: Color::Rgb(0x55, 0x55, 0x55),
            bg_selected: Color::Rgb(0x44, 0x44, 0x44),
            selected_fg: Color::White,
            selected_dim_fg: Color::Rgb(0xAA, 0xAA, 0xAA),
            bg_subtle: Color::Rgb(0x33, 0x33, 0x33),
            accent_warning: Color::Rgb(0xE5, 0xC0, 0x7B),
            accent_info: Color::Rgb(0x61, 0xAF, 0xEF),
            accent_flag: Color::Rgb(0x61, 0x96, 0xCC),
            search_match_bg: Color::Rgb(0x88, 0x88, 0x00),
            search_match_fg: Color::Black,
            search_current_bg: Color::Rgb(0xFF, 0xFF, 0x00),
            search_current_fg: Color::Black,
            selection_bg: Color::Rgb(0x44, 0x44, 0x88),
            selection_fg: Color::White,
        }
    }

    pub fn light() -> Self {
        Palette {
            primary: Color::Rgb(0xD9, 0x77, 0x57),
            border_unfocused: Color::Rgb(0x99, 0x99, 0x99),
            text: Color::Rgb(0x33, 0x33, 0x33),
            text_dim: Color::Rgb(0x55, 0x55, 0x55),
            text_subtle: Color::Rgb(0x77, 0x77, 0x77),
            text_muted: Color::Rgb(0xAA, 0xAA, 0xAA),
            bg_selected: Color::Rgb(0xDD, 0xDD, 0xDD),
            selected_fg: Color::Rgb(0x11, 0x11, 0x11),
            selected_dim_fg: Color::Rgb(0x55, 0x55, 0x55),
            bg_subtle: Color::Rgb(0xEE, 0xEE, 0xEE),
            accent_warning: Color::Rgb(0xB8, 0x8C, 0x2E),
            accent_info: Color::Rgb(0x2E, 0x7D, 0xC4),
            accent_flag: Color::Rgb(0x2E, 0x6E, 0xA8),
            search_match_bg: Color::Rgb(0xE5, 0xE0, 0x60),
            search_match_fg: Color::Black,
            search_current_bg: Color::Rgb(0xFF, 0xFF, 0x00),
            search_current_fg: Color::Black,
            selection_bg: Color::Rgb(0xC0, 0xC0, 0xE5),
            selection_fg: Color::Rgb(0x11, 0x11, 0x11),
        }
    }

    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Palette::dark(),
            ThemeMode::Light => Palette::light(),
        }
    }
}

pub fn resolve_palette(config_theme: ThemeMode) -> Palette {
    if let Ok(value) = std::env::var("AGENT_DASH_THEME") {
        let mode = if value.eq_ignore_ascii_case("light") {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        return Palette::for_mode(mode);
    }

    if config_theme != ThemeMode::Dark {
        return Palette::for_mode(config_theme);
    }

    Palette::dark()
}
