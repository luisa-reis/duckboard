//! What the tiles draw from. A `Snapshot` is the latest of everything the
//! sources have fetched; the frame loop takes a copy each frame.

#[derive(Clone, Debug, Default)]
pub struct Snapshot {}

impl Snapshot {
    /// Made-up data for previews that must not touch the network.
    pub fn sample() -> Self {
        Self {}
    }
}
