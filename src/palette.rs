//! Colours with alpha, and the palette of roles the tiles draw with.
//!
//! Everything is drawn in `Rgba`; the canvas blends a pixel over what is
//! already there by its alpha, so a translucent colour shows the background
//! art through it. The palette names colours by role, with the values the
//! tiles were tuned with on the panel as defaults; `[colors]` in the config
//! overrides any of them for every tile, and a tile's own `colors` for that
//! tile alone.

use embedded_graphics::{pixelcolor::raw::RawU32, prelude::PixelColor};
use serde::{de, Deserialize, Deserializer};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl PixelColor for Rgba {
    type Raw = RawU32;
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// The same colour at `alpha` times its own alpha.
    pub fn scaled(self, alpha: f32) -> Self {
        Self { a: (self.a as f32 * alpha.clamp(0.0, 1.0)).round() as u8, ..self }
    }

    /// Parses "#rrggbb" or "#rrggbbaa".
    pub fn parse(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').ok_or_else(|| format!("{s:?}: a colour is #rrggbb or #rrggbbaa"))?;
        if hex.len() != 6 && hex.len() != 8 {
            return Err(format!("{s:?}: a colour is #rrggbb or #rrggbbaa"));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("{s:?}: bad hex digits"));
        Ok(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: if hex.len() == 8 { byte(6)? } else { 255 } })
    }
}

impl serde::Serialize for Rgba {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let rgb = format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b);
        s.serialize_str(&if self.a == 255 { rgb } else { format!("{rgb}{:02x}", self.a) })
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgba::parse(&s).map_err(de::Error::custom)
    }
}

impl schemars::JsonSchema for Rgba {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Colour".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A colour, \"#rrggbb\", or \"#rrggbbaa\" with alpha to blend it over what is under it.",
            "type": "string",
            "pattern": "^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"
        })
    }
}

pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);

/// The roles. Every field is what the tiles were tuned with.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Primary values: hours, the day, sensor readings, the title.
    pub text: Rgba,
    /// Labels, units, the month, the artist.
    pub label: Rgba,
    /// Tracks and placeholders: the unfilled seconds ring, an empty bar, "--".
    pub track: Rgba,
    /// Highlights: the weekday, the seconds fill, a bar on its way.
    pub accent: Rgba,
    /// The minutes.
    pub secondary: Rgba,
    /// A bar that has reached its maximum.
    pub full: Rgba,
    pub sun: Rgba,
    pub moon: Rgba,
    pub cloud: Rgba,
    pub storm_cloud: Rgba,
    pub rain: Rgba,
    pub snow: Rgba,
    pub fog: Rgba,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            text: Rgba::rgb(255, 255, 255),
            label: Rgba::rgb(110, 110, 110),
            track: Rgba::rgb(45, 45, 45),
            accent: Rgba::rgb(255, 170, 0),
            secondary: Rgba::rgb(80, 170, 255),
            full: Rgba::rgb(60, 220, 90),
            sun: Rgba::rgb(255, 170, 0),
            moon: Rgba::rgb(220, 220, 180),
            cloud: Rgba::rgb(200, 200, 210),
            storm_cloud: Rgba::rgb(120, 120, 140),
            rain: Rgba::rgb(60, 140, 255),
            snow: Rgba::rgb(255, 255, 255),
            fog: Rgba::rgb(110, 110, 110),
        }
    }
}

/// The same roles, each optional: what a config table sets.
#[derive(Clone, Copy, Debug, Default, Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moon: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storm_cloud: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rain: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow: Option<Rgba>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog: Option<Rgba>,
}

impl Palette {
    pub fn with(mut self, o: &Overrides) -> Self {
        macro_rules! apply {
            ($($f:ident),*) => { $( if let Some(c) = o.$f { self.$f = c; } )* };
        }
        apply!(text, label, track, accent, secondary, full, sun, moon, cloud, storm_cloud, rain, snow, fog);
        self
    }
}
