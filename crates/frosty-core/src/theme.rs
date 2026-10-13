//! Theme files and contrast checking (docs/DESIGN_SPEC.md section 13).

use std::collections::BTreeMap;

use crate::config::ModeSetting;

pub const REQUIRED_TOKENS: [&str; 12] = [
    "bg",
    "surface",
    "surface-raised",
    "border",
    "text",
    "text-muted",
    "accent",
    "accent-on",
    "focus-ring",
    "success",
    "warning",
    "danger",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ThemeError {
    #[error("theme file is not valid TOML: {0}")]
    Parse(String),
    #[error("theme is missing `[theme]` id or name")]
    MissingHeader,
    #[error("theme declares mode `{0}`, which is not light or dark")]
    UnknownMode(String),
    #[error("theme has no modes")]
    NoModes,
    #[error("theme mode `{mode}` is missing required token `{token}`")]
    MissingToken { mode: String, token: String },
    #[error("theme mode `{mode}` token `{token}` is not a #RRGGBB color: `{value}`")]
    BadColor {
        mode: String,
        token: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub light: Option<BTreeMap<String, String>>,
    pub dark: Option<BTreeMap<String, String>>,
    /// Optional tokens from `[theme]`: `radius-scale`, `shadow-style`,
    /// `border-width`, `surface-style`, `font-display`, `fox-tint`.
    pub options: BTreeMap<String, String>,
}

fn string_table(table: &toml::Table) -> BTreeMap<String, String> {
    table
        .iter()
        .map(|(k, v)| {
            let s = match v {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), s)
        })
        .collect()
}

impl Theme {
    pub fn parse(text: &str) -> Result<Self, ThemeError> {
        let root: toml::Table =
            toml::from_str(text).map_err(|e| ThemeError::Parse(e.to_string()))?;
        let header = root
            .get("theme")
            .and_then(toml::Value::as_table)
            .ok_or(ThemeError::MissingHeader)?;
        let id = header.get("id").and_then(toml::Value::as_str);
        let name = header.get("name").and_then(toml::Value::as_str);
        let (Some(id), Some(name)) = (id, name) else {
            return Err(ThemeError::MissingHeader);
        };

        let modes: Vec<String> = header
            .get("modes")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if modes.is_empty() {
            return Err(ThemeError::NoModes);
        }

        let mut light = None;
        let mut dark = None;
        for mode in &modes {
            let tokens = match mode.as_str() {
                "light" | "dark" => root
                    .get(mode)
                    .and_then(toml::Value::as_table)
                    .map(string_table)
                    .unwrap_or_default(),
                other => return Err(ThemeError::UnknownMode(other.to_string())),
            };
            for required in REQUIRED_TOKENS {
                let Some(value) = tokens.get(required) else {
                    return Err(ThemeError::MissingToken {
                        mode: mode.clone(),
                        token: required.to_string(),
                    });
                };
                if parse_hex(value).is_none() {
                    return Err(ThemeError::BadColor {
                        mode: mode.clone(),
                        token: required.to_string(),
                        value: value.clone(),
                    });
                }
            }
            if mode == "light" {
                light = Some(tokens);
            } else {
                dark = Some(tokens);
            }
        }

        let options = string_table(header)
            .into_iter()
            .filter(|(k, _)| !matches!(k.as_str(), "id" | "name" | "modes"))
            .collect();

        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            light,
            dark,
            options,
        })
    }

    pub fn tokens(&self, mode: Mode) -> Option<&BTreeMap<String, String>> {
        match mode {
            Mode::Light => self.light.as_ref(),
            Mode::Dark => self.dark.as_ref(),
        }
    }

    /// Picks the mode to render. Returns the mode and whether it differs from
    /// what was asked for, so the picker can say so (DESIGN_SPEC.md 13.2).
    pub fn resolve_mode(&self, requested: ModeSetting, system_is_dark: bool) -> (Mode, bool) {
        let wanted = match requested {
            ModeSetting::Light => Mode::Light,
            ModeSetting::Dark => Mode::Dark,
            ModeSetting::System => {
                if system_is_dark {
                    Mode::Dark
                } else {
                    Mode::Light
                }
            }
        };
        if self.tokens(wanted).is_some() {
            (wanted, false)
        } else if wanted == Mode::Dark {
            (Mode::Light, true)
        } else {
            (Mode::Dark, true)
        }
    }
}

pub const BUILT_IN_THEMES: [&str; 2] = [
    include_str!("../themes/frost.toml"),
    include_str!("../themes/cartridge.toml"),
];

pub fn built_in_themes() -> Vec<Theme> {
    BUILT_IN_THEMES
        .iter()
        .map(|t| Theme::parse(t).expect("built-in theme must be valid"))
        .collect()
}

pub fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

fn linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance of a `#RRGGBB` color.
pub fn luminance(value: &str) -> Option<f64> {
    let (r, g, b) = parse_hex(value)?;
    Some(0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b))
}

/// WCAG contrast ratio, 1.0 to 21.0.
pub fn contrast_ratio(a: &str, b: &str) -> Option<f64> {
    let (la, lb) = (luminance(a)?, luminance(b)?);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    Some((hi + 0.05) / (lo + 0.05))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContrastIssue {
    pub foreground: &'static str,
    pub background: &'static str,
    pub ratio: f64,
    pub required: f64,
}

/// Checks the pairs components rely on against WCAG AA: text 4.5, UI parts 3.
pub fn check_aa(tokens: &BTreeMap<String, String>) -> Vec<ContrastIssue> {
    const PAIRS: [(&str, &str, f64); 6] = [
        ("text", "bg", 4.5),
        ("text", "surface", 4.5),
        ("text-muted", "bg", 4.5),
        ("text-muted", "surface", 4.5),
        ("accent-on", "accent", 4.5),
        ("focus-ring", "bg", 3.0),
    ];
    PAIRS
        .iter()
        .filter_map(|&(fg, bg, required)| {
            let ratio = contrast_ratio(tokens.get(fg)?, tokens.get(bg)?)?;
            (ratio < required).then_some(ContrastIssue {
                foreground: fg,
                background: bg,
                ratio,
                required,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_parse() {
        let themes = built_in_themes();
        assert_eq!(themes.len(), 2);
        assert!(themes.iter().any(|t| t.id == "frost" && t.dark.is_some()));
        let cartridge = themes.iter().find(|t| t.id == "cartridge").unwrap();
        assert!(cartridge.dark.is_none());
        assert_eq!(
            cartridge.options.get("shadow-style").map(String::as_str),
            Some("hard")
        );
    }

    #[test]
    fn built_in_themes_meet_aa_in_every_mode() {
        for theme in built_in_themes() {
            for mode in [Mode::Light, Mode::Dark] {
                if let Some(tokens) = theme.tokens(mode) {
                    let issues = check_aa(tokens);
                    assert!(issues.is_empty(), "{} {:?}: {:?}", theme.id, mode, issues);
                }
            }
        }
    }

    #[test]
    fn contrast_known_values() {
        let black_on_white = contrast_ratio("#000000", "#FFFFFF").unwrap();
        assert!((black_on_white - 21.0).abs() < 0.01);
        assert!((contrast_ratio("#777777", "#777777").unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn missing_token_is_rejected() {
        let text = BUILT_IN_THEMES[0].replace("accent-on = \"#1B1206\"\n", "");
        let err = Theme::parse(&text).unwrap_err();
        assert!(matches!(err, ThemeError::MissingToken { ref token, .. } if token == "accent-on"));
    }

    #[test]
    fn bad_color_is_rejected() {
        let text = BUILT_IN_THEMES[0].replacen("#F4F9FC", "blue", 1);
        assert!(matches!(
            Theme::parse(&text),
            Err(ThemeError::BadColor { .. })
        ));
    }

    #[test]
    fn mode_falls_back_to_a_supported_mode() {
        let themes = built_in_themes();
        let cartridge = themes.iter().find(|t| t.id == "cartridge").unwrap();
        assert_eq!(
            cartridge.resolve_mode(ModeSetting::Dark, true),
            (Mode::Light, true)
        );
        assert_eq!(
            cartridge.resolve_mode(ModeSetting::System, false),
            (Mode::Light, false)
        );
        let frost = themes.iter().find(|t| t.id == "frost").unwrap();
        assert_eq!(
            frost.resolve_mode(ModeSetting::System, true),
            (Mode::Dark, false)
        );
    }
}
