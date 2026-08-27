//! Casi d'uso applicativi.
//!
//! Ogni servizio orchestra i crate di dominio e lo stato persistente; non
//! conosce Tauri. I comandi IPC in `crate::commands` sono un guscio sottile
//! sopra queste funzioni.

pub mod addons;
pub mod beta;
pub mod community;
pub mod controller;
pub mod diagnostics;
pub mod dolphin;
pub mod gamebanana;
pub mod launch;
pub mod launcher;
pub mod mii;
pub mod mii_render;
pub mod mods;
pub mod music_pack;
pub mod news;
pub mod saves;
