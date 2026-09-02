//! Multimodal database interaction: voice, chart, ER diagram, screenshot.

#![cfg_attr(not(feature = "multimodal"), allow(unused_imports))]

#[cfg(feature = "multimodal")]
pub mod chart;
#[cfg(feature = "multimodal")]
pub mod types;
#[cfg(feature = "multimodal-voice")]
pub mod voice;
