use clap::ValueEnum;
use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub background: Color,
    pub folder: Color,
    pub note: Color,
    pub modified: Color,
    pub tag: Color,
}

impl Default for Theme {
    fn default() -> Self {
        ThemeName::default().resolve()
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[clap(rename_all = "kebab-case")]
pub enum ThemeName {
    ObsidianDark,
    ObsidianLight,
    SolarizedDark,
    SolarizedLight,
    GruvboxDark,
    GruvboxLight,
}

impl Default for ThemeName {
    fn default() -> Self {
        ThemeName::ObsidianDark
    }
}

impl ThemeName {
    pub fn resolve(self) -> Theme {
        match self {
            ThemeName::ObsidianDark => Theme {
                accent: Color::Rgb(166, 218, 149),
                background: Color::Rgb(36, 37, 38),
                folder: Color::Rgb(255, 203, 107),
                note: Color::Rgb(208, 208, 208),
                modified: Color::Rgb(255, 132, 132),
                tag: Color::Rgb(124, 174, 254),
            },
            ThemeName::ObsidianLight => Theme {
                accent: Color::Rgb(76, 110, 245),
                background: Color::Rgb(250, 250, 250),
                folder: Color::Rgb(66, 92, 162),
                note: Color::Rgb(33, 33, 33),
                modified: Color::Rgb(210, 77, 87),
                tag: Color::Rgb(114, 124, 245),
            },
            ThemeName::SolarizedDark => Theme {
                accent: Color::Rgb(147, 161, 161),
                background: Color::Rgb(0, 43, 54),
                folder: Color::Rgb(88, 110, 117),
                note: Color::Rgb(253, 246, 227),
                modified: Color::Rgb(203, 75, 22),
                tag: Color::Rgb(38, 139, 210),
            },
            ThemeName::SolarizedLight => Theme {
                accent: Color::Rgb(101, 123, 131),
                background: Color::Rgb(253, 246, 227),
                folder: Color::Rgb(38, 139, 210),
                note: Color::Rgb(0, 43, 54),
                modified: Color::Rgb(211, 54, 130),
                tag: Color::Rgb(133, 153, 0),
            },
            ThemeName::GruvboxDark => Theme {
                accent: Color::Rgb(215, 153, 33),
                background: Color::Rgb(40, 40, 40),
                folder: Color::Rgb(189, 174, 147),
                note: Color::Rgb(235, 219, 178),
                modified: Color::Rgb(204, 36, 29),
                tag: Color::Rgb(104, 157, 106),
            },
            ThemeName::GruvboxLight => Theme {
                accent: Color::Rgb(204, 36, 29),
                background: Color::Rgb(251, 241, 199),
                folder: Color::Rgb(152, 151, 26),
                note: Color::Rgb(60, 56, 54),
                modified: Color::Rgb(204, 36, 29),
                tag: Color::Rgb(69, 133, 136),
            },
        }
    }
}
