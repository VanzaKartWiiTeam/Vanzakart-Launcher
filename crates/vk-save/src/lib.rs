//! # vk-save
//!
//! Formati binari di Mario Kart Wii e della Wii: `rksys.dat`, `RFL_DB.dat` e i
//! blocchi Mii da 74 byte.
//!
//! Le scritture sui formati di salvataggio esistono e sono testate contro
//! fixture binarie **reali** e anonimizzate, in `fixtures/` (vedi
//! `docs/decisions.md` §D-012). Il test che le abilita è il round-trip: leggere
//! un file vero, riscriverlo senza modifiche e ottenere gli stessi byte, bit
//! per bit. Un `rksys.dat` corrotto costa all'utente tutte le sue licenze e non
//! è recuperabile.
//!
//! Nessuna scrittura ricostruisce il file: modifica il buffer letto dal disco e
//! ricalcola i soli checksum, così le regioni di cui non si conosce il
//! significato restano intatte.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod crc;
pub mod error;
pub mod friend_code;
pub mod mii;
pub mod miidb;
pub mod rksys;

pub use error::{SaveError, SaveResult};
pub use mii::WiiMii;
pub use rksys::LicenseCard;
