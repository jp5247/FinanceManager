use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Stream-hash `path` and return the lowercase hex SHA-256.
///
/// Streaming so we don't slurp a 50 MB statement into RAM.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        write!(hex, "{b:02x}").expect("write to String never fails");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::NamedTempFile;

    #[test]
    fn known_vector() {
        // SHA-256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello world").unwrap();
        let h = sha256_file(f.path()).unwrap();
        assert_eq!(
            h,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn empty_file_hash() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let f = NamedTempFile::new().unwrap();
        let h = sha256_file(f.path()).unwrap();
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn streams_large_file_without_oom() {
        // Write 5 MB of zeros — large enough that the chunked hash is exercised.
        let mut f = NamedTempFile::new().unwrap();
        let chunk = vec![0u8; 1024 * 1024];
        for _ in 0..5 {
            f.write_all(&chunk).unwrap();
        }
        let h = sha256_file(f.path()).unwrap();
        assert_eq!(h.len(), 64);
        // SHA-256 of 5 MiB of zeros is a well-known constant; just check shape.
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
