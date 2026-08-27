//! SHA-256 su file e buffer.
//!
//! Regola di normalizzazione (vedi `docs/decisions.md` §D-022): gli hash
//! prodotti sono **sempre** in minuscolo; i confronti sono sempre
//! case-insensitive, perché i manifest legacy contengono entrambe le forme.

use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::error::{CoreError, CoreResult};

/// Stessa dimensione di buffer del `NetworkService` legacy.
pub const HASH_BUFFER_SIZE: usize = 256 * 1024;

/// Calcola lo SHA-256 di un file, in esadecimale minuscolo.
pub async fn sha256_file(path: impl AsRef<Path>) -> CoreResult<String> {
    let path = path.as_ref();
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| CoreError::io(path, e))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];

    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|e| CoreError::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Variante sincrona di [`sha256_file`], per i percorsi che girano dentro
/// `spawn_blocking` (estrazione ZIP).
pub fn sha256_file_sync(path: impl AsRef<std::path::Path>) -> CoreResult<String> {
    use std::io::Read;

    let path = path.as_ref();
    let mut file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];

    loop {
        let read = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Calcola lo SHA-256 di un buffer, in esadecimale minuscolo.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Confronta due hash ignorando maiuscole/minuscole e spazi ai bordi.
pub fn hash_eq(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// `true` se la stringa è uno SHA-256 esadecimale di 64 caratteri.
pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Verifica un file contro l'hash atteso; errore [`CoreError::HashMismatch`] se diverso.
pub async fn verify_file(
    path: impl AsRef<Path>,
    expected: &str,
    label: &str,
) -> CoreResult<String> {
    let actual = sha256_file(&path).await?;
    if hash_eq(&actual, expected) {
        Ok(actual)
    } else {
        Err(CoreError::HashMismatch {
            path: label.to_string(),
            expected: expected.trim().to_lowercase(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // SHA-256 della stringa vuota.
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn comparison_is_case_insensitive() {
        assert!(hash_eq(
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
        assert!(hash_eq("  abc  ", "ABC"));
        assert!(!hash_eq("abc", "abd"));
    }

    #[test]
    fn validates_shape() {
        assert!(is_valid_sha256(&sha256_bytes(b"x")));
        assert!(!is_valid_sha256("short"));
        assert!(!is_valid_sha256(&"z".repeat(64)));
    }

    #[test]
    fn sync_and_async_hashes_agree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        let payload = vec![7u8; HASH_BUFFER_SIZE + 3];
        std::fs::write(&path, &payload).unwrap();
        assert_eq!(sha256_file_sync(&path).unwrap(), sha256_bytes(&payload));
    }

    #[tokio::test]
    async fn hashes_a_file_in_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        // Più grande del buffer, per esercitare il loop di lettura.
        let payload = vec![0xABu8; HASH_BUFFER_SIZE * 2 + 17];
        tokio::fs::write(&path, &payload).await.unwrap();

        assert_eq!(sha256_file(&path).await.unwrap(), sha256_bytes(&payload));
        verify_file(&path, &sha256_bytes(&payload), "payload.bin")
            .await
            .unwrap();

        let err = verify_file(&path, &"0".repeat(64), "payload.bin")
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::HashMismatch { .. }));
    }
}
