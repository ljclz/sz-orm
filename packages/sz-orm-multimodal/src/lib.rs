//! Multimodal database interaction: voice, chart, ER diagram, screenshot.

#[cfg(feature = "multimodal")]
pub mod chart;
#[cfg(feature = "multimodal")]
pub mod dialog;
#[cfg(feature = "multimodal-er")]
pub mod er_diagram;
#[cfg(feature = "multimodal")]
pub mod fallback;
#[cfg(feature = "multimodal-vision")]
pub mod screenshot;
#[cfg(feature = "multimodal-vision")]
pub mod sketch;
#[cfg(feature = "multimodal")]
pub mod types;
#[cfg(feature = "multimodal-voice")]
pub mod voice;
