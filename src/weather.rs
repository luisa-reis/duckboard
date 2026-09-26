//! Current conditions from Open-Meteo, which needs no key: a location and
//! the unit are the whole configuration.

use crate::config::WeatherConfig;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Weather {
    pub temperature: f32,
    /// WMO weather code, see `Sky::from_code`.
    pub code: u16,
    pub is_day: bool,
}

#[derive(Deserialize)]
struct Response {
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f32,
    weather_code: u16,
    is_day: u8,
}

pub fn fetch(agent: &ureq::Agent, cfg: &WeatherConfig) -> Result<Weather> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,weather_code,is_day&temperature_unit={}",
        cfg.latitude,
        cfg.longitude,
        cfg.units.unwrap_or_default().api_name()
    );
    let r: Response = agent
        .get(&url)
        .timeout(Duration::from_secs(10))
        .call()
        .context("Open-Meteo request")?
        .into_json()
        .context("Open-Meteo response")?;
    Ok(Weather {
        temperature: r.current.temperature_2m,
        code: r.current.weather_code,
        is_day: r.current.is_day != 0,
    })
}

/// The sky, reduced to what a 12x12 icon can tell apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Rain,
    Snow,
    Storm,
}

impl Sky {
    /// WMO code groups as Open-Meteo documents them.
    pub fn from_code(code: u16) -> Sky {
        match code {
            0 => Sky::Clear,
            1 | 2 => Sky::PartlyCloudy,
            3 => Sky::Cloudy,
            45 | 48 => Sky::Fog,
            51..=67 | 80..=82 => Sky::Rain,
            71..=77 | 85 | 86 => Sky::Snow,
            95..=99 => Sky::Storm,
            _ => Sky::Cloudy,
        }
    }
}
