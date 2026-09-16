//! Firma dei pacchetti: verifica, non produzione.
//!
//! Un'impronta SHA-256 dichiarata nel manifest dice che il file scaricato è
//! quello che il manifest descrive. Non dice chi ha scritto il manifest. La
//! firma sì: è Ed25519, la chiave privata non esiste su nessuna macchina che
//! non sia quella di chi pubblica, e un pacchetto sostituito lungo la strada
//! — o un manifest riscritto per intero — non la supera (§D-084).
//!
//! Il formato è minisign, lo stesso che produce `tauri signer sign`: i
//! rilasci già firmati restano verificabili e lo script di pubblicazione non
//! cambia strumento. Il file `.sig` contiene il documento minisign in base64,
//! ed è quella stringa che finisce dentro `install.json`.

use std::io::Read;
use std::path::Path;

use base64::Engine;

use crate::error::{InstallError, InstallResult};

/// Chiave pubblica dell'updater, nella forma del file `.pub` in base64.
///
/// È pubblica per definizione: sta nel binario perché ogni copia installata
/// deve poter verificare da sé, senza chiedere niente a nessuno.
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDcxQjJCREM5OEIxMjU5MEEKUldRS1dSS0x5YjJ5Y2JxcUIyT0N6b2JSSTVoTFU0UmxtNTF4QWhwdE5XZDBtLzRUZUFaa1hwbEMK";

/// Quanto si legge per volta mentre si calcola l'impronta della firma.
const CHUNK: usize = 64 * 1024;

fn decode_base64_text(value: &str, what: &str) -> InstallResult<String> {
    let cleaned: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned)
        .map_err(|_| InstallError::InvalidSignature(format!("{what}: not valid base64")))?;
    String::from_utf8(bytes)
        .map_err(|_| InstallError::InvalidSignature(format!("{what}: not valid text")))
}

/// La chiave pubblica compilata, pronta all'uso.
pub fn public_key() -> InstallResult<minisign_verify::PublicKey> {
    let text = decode_base64_text(PUBLIC_KEY, "public key")?;
    minisign_verify::PublicKey::decode(text.trim())
        .map_err(|error| InstallError::InvalidSignature(format!("public key: {error}")))
}

/// Verifica `path` contro la firma dichiarata dal manifest.
///
/// `signature` è il contenuto del file `.sig`, cioè il documento minisign in
/// base64 — esattamente ciò che `install.json` pubblica.
pub fn verify_file(path: &Path, signature: &str) -> InstallResult<()> {
    let document = decode_base64_text(signature, "signature")?;
    let parsed = minisign_verify::Signature::decode(document.trim())
        .map_err(|error| InstallError::InvalidSignature(format!("signature: {error}")))?;

    let key = public_key()?;
    let mut verifier = key
        .verify_stream(&parsed)
        .map_err(|error| InstallError::InvalidSignature(error.to_string()))?;

    let mut file = std::fs::File::open(path).map_err(|error| InstallError::io(path, error))?;
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| InstallError::io(path, error))?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }

    verifier
        .finalize()
        .map_err(|error| InstallError::InvalidSignature(error.to_string()))
}

/// `true` se la stringa ha la forma di una firma, senza ancora verificarla.
///
/// Serve a distinguere "il manifest non dichiara una firma" da "la firma è
/// scritta male": il primo caso è un rilascio non firmato, il secondo un
/// errore da segnalare.
pub fn looks_like_signature(signature: &str) -> bool {
    decode_base64_text(signature, "signature")
        .map(|text| text.contains("untrusted comment:"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_public_key_is_readable() {
        let key = public_key().expect("chiave pubblica");
        assert!(key
            .untrusted_comment()
            .is_some_and(|comment| comment.contains("minisign public key")));
    }

    #[test]
    fn a_signature_that_is_not_base64_is_refused_before_any_read() {
        let temp = tempfile::tempdir().expect("temp");
        let file = temp.path().join("pacchetto.zip");
        std::fs::write(&file, b"contenuto").expect("scritto");

        let error = verify_file(&file, "non una firma").expect_err("rifiutata");
        assert_eq!(error.code(), "invalid-signature");
        assert!(!looks_like_signature("non una firma"));
    }

    /// Una firma vera, ma di un altro file: è il caso che conta, perché è
    /// quello che si presenterebbe se qualcuno sostituisse il pacchetto
    /// lasciando il manifest com'è.
    #[test]
    fn a_signature_of_another_file_does_not_pass() {
        let temp = tempfile::tempdir().expect("temp");
        let file = temp.path().join("pacchetto.zip");
        std::fs::write(&file, b"un contenuto qualunque").expect("scritto");

        // La firma del rilascio 2.0.0, valida in sé ma di un altro pacchetto.
        const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRS1dSS0x5YjJ5Y1VZeVZCVEJkdkMxN0pRUGtSSDZ6UDByVzhjaFBKRmNMRGNLR1JJa2UrV3oxN0JEZ25kelF3bm9YOEVHZTdOZCtMQ1dwaHV6b2NCazdsdnJidGNuT2djPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzg3Nzg3MzYyCWZpbGU6VmFuemFLYXJ0IExhdW5jaGVyXzIuMC4wX3g2NC1zZXR1cC5leGUKSUROa2QvSC9LcitpOWQxejQwbVp0L1RLc0RDbjkrdStVV25uTmlhazR1YVRjUUs2dWErQnI3L003R3NkUDdoZUE1emVJOEg0MHZJUVpDNXJmYmlkRGc9PQo=";

        assert!(looks_like_signature(SIGNATURE), "la firma è ben formata");
        let error = verify_file(&file, SIGNATURE).expect_err("non verificata");
        assert_eq!(error.code(), "invalid-signature");
    }
}
