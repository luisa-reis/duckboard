//! Pictures made ready for the panel: album art and the `[frame]` pictures,
//! each scaled to fill every size it is shown at (cropping to that shape)
//! and gamma-corrected, so the bytes are what the LEDs should show.

use anyhow::{anyhow, Result};
use embedded_graphics::prelude::Size;
use image::imageops::FilterType;

/// One picture's RGB bytes at each size it is needed at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scaled(Vec<(Size, Vec<u8>)>);

impl Scaled {
    /// The pixels at `size`, row-major RGB, if the picture was scaled to it.
    pub fn at(&self, size: Size) -> Option<&[u8]> {
        self.0.iter().find(|(s, _)| *s == size).map(|(_, b)| b.as_slice())
    }

    /// Bytes for one picture at each of `sizes`, back to back.
    pub fn byte_len(sizes: &[Size]) -> usize {
        sizes.iter().map(|s| (s.width * s.height * 3) as usize).sum()
    }

    /// Splits bytes laid out as `to_bytes` writes them; None if the length
    /// does not match `sizes`.
    pub fn from_bytes(sizes: &[Size], bytes: &[u8]) -> Option<Self> {
        if bytes.len() != Self::byte_len(sizes) {
            return None;
        }
        let mut rest = bytes;
        let mut v = Vec::with_capacity(sizes.len());
        for &s in sizes {
            let (this, tail) = rest.split_at((s.width * s.height * 3) as usize);
            v.push((s, this.to_vec()));
            rest = tail;
        }
        Some(Self(v))
    }

    /// Every size's bytes, back to back, in the order they were made.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(|(_, b)| b.iter().copied()).collect()
    }

    /// A picture made by `f(width, height)` at each of `sizes`, for
    /// made-up art.
    pub fn from_fn(sizes: &[Size], f: impl Fn(u32, u32) -> Vec<u8>) -> Self {
        Self(sizes.iter().map(|&s| (s, f(s.width, s.height))).collect())
    }
}

/// Decodes a picture and scales it to fill each of `sizes`, cropping to its
/// shape, with `gamma` applied.
pub fn decode(bytes: &[u8], gamma: f32, sizes: &[Size]) -> Result<Scaled> {
    let img = image::load_from_memory(bytes).map_err(|e| anyhow!("decoding picture: {e}"))?;
    let lut: Vec<u8> = (0..=255u32)
        .map(|v| ((v as f32 / 255.0).powf(gamma) * 255.0).round() as u8)
        .collect();
    let convert = |w: u32, h: u32| -> Vec<u8> {
        img.resize_to_fill(w, h, FilterType::Lanczos3)
            .to_rgb8()
            .into_raw()
            .into_iter()
            .map(|v| lut[v as usize])
            .collect()
    };
    Ok(Scaled::from_fn(sizes, convert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_by_size() {
        let sizes = [Size::new(2, 1), Size::new(1, 1)];
        let s = Scaled::from_fn(&sizes, |w, h| vec![w as u8; (w * h * 3) as usize]);
        let bytes = s.to_bytes();
        assert_eq!(bytes.len(), Scaled::byte_len(&sizes));
        let back = Scaled::from_bytes(&sizes, &bytes).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.at(Size::new(1, 1)), Some(&[1u8, 1, 1][..]));
        assert!(back.at(Size::new(3, 3)).is_none());
        assert!(Scaled::from_bytes(&sizes, &bytes[1..]).is_none());
    }
}
