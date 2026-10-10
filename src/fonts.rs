//! The U8g2 fonts a text tile can name with `font`, from the u8g2-fonts
//! crate: a choice of its two thousand, picked to read well on a panel.
//! Most are proportional, a character as wide as it needs to be.
//!
//! A font here is called what U8g2 calls it, less the `u8g2_font_` before
//! and the `_tf` or `_tr` after: <https://github.com/olikraus/u8g2/wiki/fntlistall>
//! shows each. To add one, add its line to the list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use embedded_graphics::prelude::Point;
use u8g2_fonts::types::VerticalPosition;
use u8g2_fonts::{fonts, FontRenderer};

macro_rules! fonts {
    ($($variant:ident => $name:literal, $font:ident;)*) => {
        /// A U8g2 font, by its U8g2 name.
        #[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
        pub enum Font {
            $(#[serde(rename = $name)] $variant,)*
        }

        impl Font {
            /// Every font, in the list's order.
            pub const ALL: &'static [Font] = &[$(Font::$variant,)*];

            /// What the configuration calls it.
            pub fn name(self) -> &'static str {
                match self {
                    $(Font::$variant => $name,)*
                }
            }

            /// What draws it. A character the font does not have is left out.
            pub fn renderer(self) -> FontRenderer {
                match self {
                    $(Font::$variant => FontRenderer::new::<fonts::$font>(),)*
                }
                .with_ignore_unknown_chars(true)
            }
        }
    };
}

impl Font {
    /// How far a line of the font reaches above its baseline and below it,
    /// in pixels: the font's own rise and fall, or more where one of its
    /// letters or digits goes further (brackets and the like may go
    /// further still).
    pub fn extent(self) -> (i32, i32) {
        let r = self.renderer();
        let ascii: String = ('0'..='z').filter(char::is_ascii_alphanumeric).collect();
        let drawn = r.get_rendered_dimensions(ascii.as_str(), Point::zero(), VerticalPosition::Baseline).ok().and_then(|d| d.bounding_box);
        // A font whose characters all stop short of the baseline has no fall.
        let (up, down) = (r.get_ascent() as i32, (-(r.get_descent() as i32)).max(0));
        match drawn {
            Some(b) => (up.max(-b.top_left.y), down.max(b.top_left.y + b.size.height as i32)),
            None => (up, down),
        }
    }
}

fonts! {
    TomThumb4x6 => "tom_thumb_4x6", u8g2_font_tom_thumb_4x6_tf;
    Micro => "micro", u8g2_font_micro_tr;
    PixelleMicro => "pixelle_micro", u8g2_font_pixelle_micro_tr;
    Tinytim => "tinytim", u8g2_font_tinytim_tf;
    Haxrcorp4089 => "haxrcorp4089", u8g2_font_haxrcorp4089_tr;
    SqueezedR7 => "squeezed_r7", u8g2_font_squeezed_r7_tr;
    SqueezedB7 => "squeezed_b7", u8g2_font_squeezed_b7_tr;
    Nokiafc22 => "nokiafc22", u8g2_font_nokiafc22_tf;
    MozartNbp => "mozart_nbp", u8g2_font_mozart_nbp_tf;
    GlasstownNbp => "glasstown_nbp", u8g2_font_glasstown_nbp_tf;
    SmartPatrolNbp => "smart_patrol_nbp", u8g2_font_smart_patrol_nbp_tf;
    Bitcasual => "bitcasual", u8g2_font_bitcasual_tf;
    Tenthinguys => "tenthinguys", u8g2_font_tenthinguys_tf;
    Tenfatguys => "tenfatguys", u8g2_font_tenfatguys_tf;
    Fewture => "fewture", u8g2_font_fewture_tf;
    Halftone => "halftone", u8g2_font_halftone_tf;
    Born2bSportyV2 => "Born2bSportyV2", u8g2_font_Born2bSportyV2_tf;
    DigitalDiscoThin => "DigitalDiscoThin", u8g2_font_DigitalDiscoThin_tf;
    Pixellari => "Pixellari", u8g2_font_Pixellari_tf;
    VcrOsd => "VCR_OSD", u8g2_font_VCR_OSD_tf;
    HelvR08 => "helvR08", u8g2_font_helvR08_tf;
    HelvB08 => "helvB08", u8g2_font_helvB08_tf;
    HelvR10 => "helvR10", u8g2_font_helvR10_tf;
    HelvB10 => "helvB10", u8g2_font_helvB10_tf;
    HelvR12 => "helvR12", u8g2_font_helvR12_tf;
    HelvB12 => "helvB12", u8g2_font_helvB12_tf;
    HelvB14 => "helvB14", u8g2_font_helvB14_tf;
    HelvB18 => "helvB18", u8g2_font_helvB18_tf;
    HelvB24 => "helvB24", u8g2_font_helvB24_tf;
    NcenR08 => "ncenR08", u8g2_font_ncenR08_tf;
    NcenB08 => "ncenB08", u8g2_font_ncenB08_tf;
    NcenB10 => "ncenB10", u8g2_font_ncenB10_tf;
    NcenB14 => "ncenB14", u8g2_font_ncenB14_tf;
    TimR08 => "timR08", u8g2_font_timR08_tf;
    TimB10 => "timB10", u8g2_font_timB10_tf;
    TimB14 => "timB14", u8g2_font_timB14_tf;
    Profont10 => "profont10", u8g2_font_profont10_tf;
    Profont12 => "profont12", u8g2_font_profont12_tf;
    Profont15 => "profont15", u8g2_font_profont15_tf;
    Profont17 => "profont17", u8g2_font_profont17_tf;
    Profont22 => "profont22", u8g2_font_profont22_tf;
    Profont29 => "profont29", u8g2_font_profont29_tf;
    T011 => "t0_11", u8g2_font_t0_11_tf;
    T011b => "t0_11b", u8g2_font_t0_11b_tf;
    T014b => "t0_14b", u8g2_font_t0_14b_tf;
    Logisoso16 => "logisoso16", u8g2_font_logisoso16_tf;
    Logisoso20 => "logisoso20", u8g2_font_logisoso20_tf;
    Logisoso24 => "logisoso24", u8g2_font_logisoso24_tf;
    Logisoso28 => "logisoso28", u8g2_font_logisoso28_tf;
    Fub11 => "fub11", u8g2_font_fub11_tf;
    Fub14 => "fub14", u8g2_font_fub14_tf;
    Fub17 => "fub17", u8g2_font_fub17_tf;
    Fub20 => "fub20", u8g2_font_fub20_tf;
    OpenIconicWeather1x => "open_iconic_weather_1x", u8g2_font_open_iconic_weather_1x_t;
    OpenIconicWeather2x => "open_iconic_weather_2x", u8g2_font_open_iconic_weather_2x_t;
    OpenIconicAll1x => "open_iconic_all_1x", u8g2_font_open_iconic_all_1x_t;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_is_named_once_and_measures() {
        let mut names: Vec<_> = Font::ALL.iter().map(|f| f.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Font::ALL.len());
        for font in Font::ALL {
            let r = font.renderer();
            r.get_rendered_dimensions("A9é", Point::zero(), VerticalPosition::Top).unwrap();
            let (up, down) = font.extent();
            assert!(up > 0 && down >= 0 && up + down <= 64, "{}: {up} up, {down} down", font.name());
        }
    }
}
