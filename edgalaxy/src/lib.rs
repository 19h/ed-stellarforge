//! `edgalaxy`: explorer for the StellarForge data recovered from Elite Dangerous.
//!
//! The library exposes the id64 geometry, the procedural naming algorithm, hand-authored
//! regions, the 143,667 authored system records, a spatial index and the galaxy density model.

pub mod classes;
pub mod density;
pub mod galaxy;
pub mod id64;
pub mod names;
pub mod records;
pub mod regions;
pub mod spatial;

pub use galaxy::Galaxy;
