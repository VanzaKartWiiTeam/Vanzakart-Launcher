//! # vk-dolphin
//!
//! Tutto ciò che riguarda l'emulatore: INI format-preserving, risoluzione dei
//! percorsi, descrittore Riivolution e mapping dei controller.
//!
//! Come `vk-core`, non dipende da Tauri e non chiama API specifiche di un
//! sistema operativo: i dati d'ambiente arrivano dall'adapter di piattaforma
//! sotto forma di [`paths::PathProbe`].

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod controller;
pub mod error;
pub mod ini;
pub mod modxml;
pub mod paths;
pub mod riivolution;
pub mod settings;

pub use controller::{ControllerMode, ControllerProfile, DeviceKind, DeviceRef};
pub use error::{DolphinError, DolphinResult};
pub use modxml::ModXml;
pub use riivolution::{GameModDescriptor, LaunchOptions};
pub use settings::DolphinSettings;
