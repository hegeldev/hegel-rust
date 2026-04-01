use std::io::Read;
use std::path::{Path, PathBuf};

const UV_VERSION: &str = "0.11.2";

/// SHA-256 checksums for each supported platform's archive.
/// Source: https://github.com/astral-sh/uv/releases/download/0.11.2/sha256.sum
fn expected_sha256(archive_name: &str) -> Option<&'static str> {
    match archive_name {
        "uv-aarch64-apple-darwin.tar.gz" => {
            Some("4beaa9550f93ef7f0fc02f7c28c9c48cd61fe30db00f5ac8947e0a425c3fb282")
        }
        "uv-x86_64-apple-darwin.tar.gz" => {
            Some("a9c3653245031304c50dd60ac0301bf6c112e12c38c32302a71d4fa6a63ba2cb")
        }
        "uv-aarch64-unknown-linux-musl.tar.gz" => {
            Some("275d91dd1f1955136591e7ec5e1fa21e84d0d37ead7da7c35c3683df748d9855")
        }
        "uv-x86_64-unknown-linux-musl.tar.gz" => {
            Some("4700d9fc75734247587deb3e25dd2c6c24f4ac69e8fe91d6acad4a6013115c06")
        }
        _ => None,
    }
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    use std::io::BufReader;

    let file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open file for hashing: {e}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Failed to read file for hashing: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hasher.hex_digest();
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}. \
             The downloaded file may be corrupted or tampered with.",
            path.display()
        ));
    }
    Ok(())
}

/// Minimal SHA-256 implementation (FIPS 180-4) to avoid adding a dependency.
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;

        if self.buffer_len > 0 {
            let space = 64 - self.buffer_len;
            let copy = space.min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + copy].copy_from_slice(&data[..copy]);
            self.buffer_len += copy;
            offset = copy;
            if self.buffer_len == 64 {
                let block = self.buffer;
                Self::compress(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }

        while offset + 64 <= data.len() {
            let block: [u8; 64] = data[offset..offset + 64].try_into().unwrap();
            Self::compress(&mut self.state, &block);
            offset += 64;
        }

        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buffer_len = remaining;
        }
    }

    fn hex_digest(mut self) -> String {
        let bit_len = self.total_len * 8;
        // Padding
        let mut pad = vec![0x80u8];
        let pad_len = (55 - (self.total_len % 64) as i64).rem_euclid(64) as usize;
        pad.resize(1 + pad_len, 0);
        pad.extend_from_slice(&bit_len.to_be_bytes());
        self.update(&pad.clone());

        self.state
            .iter()
            .map(|w| format!("{w:08x}"))
            .collect::<String>()
    }

    #[allow(clippy::many_single_char_names)]
    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
}

/// Returns the path to a `uv` binary.
///
/// Lookup order:
/// 1. `uv` found on `PATH`
/// 2. Cached binary at `~/.cache/hegel/uv`
/// 3. Downloads uv to `~/.cache/hegel/uv` and returns that path
///
/// Panics if uv cannot be found or downloaded.
pub fn find_uv() -> String {
    resolve_uv(find_in_path("uv"), cached_uv_path(), cache_dir())
}

fn resolve_uv(path_uv: Option<PathBuf>, cached: PathBuf, cache: PathBuf) -> String {
    if let Some(path) = path_uv {
        return path.to_string_lossy().into_owned();
    }
    if cached.is_file() {
        return cached.to_string_lossy().into_owned();
    }
    download_uv_to(&cache).unwrap_or_else(|e| panic!("{e}"));
    cached.to_string_lossy().into_owned()
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

fn cache_dir() -> PathBuf {
    cache_dir_from(std::env::var("XDG_CACHE_HOME").ok(), std::env::home_dir())
}

fn cache_dir_from(xdg_cache_home: Option<String>, home_dir: Option<PathBuf>) -> PathBuf {
    if let Some(xdg_cache) = xdg_cache_home {
        return PathBuf::from(xdg_cache).join("hegel");
    }
    let home = home_dir.expect("Could not determine home directory");
    home.join(".cache").join("hegel")
}

fn cached_uv_path() -> PathBuf {
    cache_dir().join("uv")
}

fn platform_archive_name() -> Result<String, String> {
    archive_name_for(std::env::consts::ARCH, std::env::consts::OS)
}

fn archive_name_for(arch: &str, os: &str) -> Result<String, String> {
    let triple = match (arch, os) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        // musl builds are statically linked, so they work on any Linux
        // regardless of glibc version (including Alpine and older distros).
        ("aarch64", "linux") => "aarch64-unknown-linux-musl",
        ("x86_64", "linux") => "x86_64-unknown-linux-musl",
        _ => {
            return Err(format!(
                "Unsupported platform: {arch}-{os}. \
                 Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
            ));
        }
    };
    Ok(format!("uv-{triple}.tar.gz"))
}

fn download_uv_to(cache: &Path) -> Result<(), String> {
    let archive_name = platform_archive_name()?;
    let url =
        format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{archive_name}");
    let expected_hash = expected_sha256(&archive_name);
    download_url_to_cache(&url, &archive_name, expected_hash, cache)
}

fn download_url_to_cache(
    url: &str,
    archive_name: &str,
    expected_sha256: Option<&str>,
    cache: &Path,
) -> Result<(), String> {
    std::fs::create_dir_all(cache)
        .map_err(|e| format!("Failed to create cache directory {}: {e}", cache.display()))?;

    // Use a per-process temp directory inside the cache dir so that:
    // 1. Concurrent downloads don't interfere with each other
    // 2. The final rename is atomic (same filesystem)
    let temp_dir = cache.join(format!(".uv-download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {e}"))?;
    let _cleanup = CleanupGuard(&temp_dir);

    let archive_path = temp_dir.join(archive_name);

    let output = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&archive_path)
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to run curl to download uv: {e}. Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to download uv from {url}: {stderr}\n\
             Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
        ));
    }

    if let Some(expected) = expected_sha256 {
        verify_sha256(&archive_path, expected)?;
    }

    let output = std::process::Command::new("tar")
        .args(["xzf"])
        .arg(&archive_path)
        .args(["--strip-components", "1", "-C"])
        .arg(&temp_dir)
        .output()
        .map_err(|e| format!("Failed to extract uv archive: {e}. Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to extract uv archive: {stderr}\n\
             Install uv manually: https://docs.astral.sh/uv/getting-started/installation/"
        ));
    }

    let extracted_uv = temp_dir.join("uv");

    // Atomic rename — safe under concurrent downloads because rename on the
    // same filesystem is atomic on Unix, so the last writer wins with a
    // valid binary.
    let final_path = cache.join("uv");
    std::fs::rename(&extracted_uv, &final_path)
        .map_err(|e| format!("Failed to install uv to {}: {e}", final_path.display()))?;

    Ok(())
}

/// RAII guard that removes a directory on drop.
struct CleanupGuard<'a>(&'a std::path::Path);

impl Drop for CleanupGuard<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real release archive names from uv 0.11.2 on GitHub.
    /// Source: https://github.com/astral-sh/uv/releases/tag/0.11.2
    const UV_RELEASE_ARCHIVES: &[&str] = &[
        "uv-aarch64-apple-darwin.tar.gz",
        "uv-aarch64-pc-windows-msvc.zip",
        "uv-aarch64-unknown-linux-gnu.tar.gz",
        "uv-aarch64-unknown-linux-musl.tar.gz",
        "uv-arm-unknown-linux-musleabihf.tar.gz",
        "uv-armv7-unknown-linux-gnueabihf.tar.gz",
        "uv-armv7-unknown-linux-musleabihf.tar.gz",
        "uv-i686-pc-windows-msvc.zip",
        "uv-i686-unknown-linux-gnu.tar.gz",
        "uv-i686-unknown-linux-musl.tar.gz",
        "uv-powerpc64le-unknown-linux-gnu.tar.gz",
        "uv-riscv64gc-unknown-linux-gnu.tar.gz",
        "uv-riscv64gc-unknown-linux-musl.tar.gz",
        "uv-s390x-unknown-linux-gnu.tar.gz",
        "uv-x86_64-apple-darwin.tar.gz",
        "uv-x86_64-pc-windows-msvc.zip",
        "uv-x86_64-unknown-linux-gnu.tar.gz",
        "uv-x86_64-unknown-linux-musl.tar.gz",
    ];

    const ARCHES: &[&str] = &["aarch64", "x86_64"];
    const OSES: &[&str] = &["macos", "linux"];

    #[test]
    fn test_all_supported_platforms_have_real_release_archives() {
        for arch in ARCHES {
            for os in OSES {
                let name = archive_name_for(arch, os).unwrap();
                assert!(
                    UV_RELEASE_ARCHIVES.contains(&name.as_str()),
                    "archive_name_for({arch:?}, {os:?}) = {name:?} is not in the uv release"
                );
            }
        }
    }

    #[test]
    fn test_all_release_archives_are_covered() {
        let supported: Vec<String> = ARCHES
            .iter()
            .flat_map(|arch| {
                OSES.iter()
                    .map(move |os| archive_name_for(arch, os).unwrap())
            })
            .collect();

        let uncovered: Vec<&&str> = UV_RELEASE_ARCHIVES
            .iter()
            .filter(|name| !supported.contains(&name.to_string()))
            .collect();

        // We only expect to not cover Windows (.zip) and non-musl Linux
        // variants — we don't need to support every platform uv ships for.
        for name in &uncovered {
            assert!(
                name.ends_with(".zip")
                    || name.contains("-gnu")
                    || name.contains("-musleabihf")
                    || name.contains("-i686-")
                    || name.contains("-arm-")
                    || name.contains("-armv7-")
                    || name.contains("-powerpc64le-")
                    || name.contains("-riscv64gc-")
                    || name.contains("-s390x-"),
                "release archive {name} is not covered by archive_name_for and is not \
                 an expected exclusion — should it be added as a supported platform?"
            );
        }
    }

    #[test]
    fn test_unsupported_platform_returns_error() {
        let err = archive_name_for("mips", "freebsd").unwrap_err();
        assert!(err.contains("Unsupported platform: mips-freebsd"));
        assert!(err.contains("Install uv manually"));
    }

    #[test]
    fn test_cache_dir_with_xdg() {
        let result = cache_dir_from(Some("/tmp/xdg".to_string()), None);
        assert_eq!(result, PathBuf::from("/tmp/xdg/hegel"));
    }

    #[test]
    fn test_cache_dir_with_home() {
        let result = cache_dir_from(None, Some(PathBuf::from("/home/test")));
        assert_eq!(result, PathBuf::from("/home/test/.cache/hegel"));
    }

    #[test]
    fn test_find_in_path_finds_known_binary() {
        assert!(find_in_path("sh").is_some());
    }

    #[test]
    fn test_find_in_path_returns_none_for_missing() {
        assert!(find_in_path("definitely_not_a_real_binary_xyz").is_none());
    }

    #[test]
    fn test_resolve_uv_prefers_path() {
        let temp = tempfile::tempdir().unwrap();
        let fake_uv = temp.path().join("uv");
        std::fs::write(&fake_uv, "fake").unwrap();

        let result = resolve_uv(
            Some(fake_uv.clone()),
            PathBuf::from("/nonexistent/uv"),
            PathBuf::from("/nonexistent"),
        );
        assert_eq!(result, fake_uv.to_string_lossy());
    }

    #[test]
    fn test_resolve_uv_uses_cache() {
        let temp = tempfile::tempdir().unwrap();
        let cached = temp.path().join("uv");
        std::fs::write(&cached, "fake").unwrap();

        let result = resolve_uv(None, cached.clone(), PathBuf::from("/nonexistent"));
        assert_eq!(result, cached.to_string_lossy());
    }

    /// Creates a tar.gz archive containing a fake "uv" binary inside a
    /// subdirectory (matching the real uv release structure that
    /// --strip-components 1 expects).
    fn create_fake_uv_archive(dir: &Path) -> PathBuf {
        let content_dir = dir.join("uv-fake");
        std::fs::create_dir_all(&content_dir).unwrap();
        std::fs::write(content_dir.join("uv"), "#!/bin/sh\necho fake-uv").unwrap();

        let archive = dir.join("uv-fake.tar.gz");
        let output = std::process::Command::new("tar")
            .args(["czf"])
            .arg(&archive)
            .args(["-C", dir.to_str().unwrap(), "uv-fake"])
            .output()
            .unwrap();
        assert!(output.status.success(), "failed to create test archive");
        archive
    }

    #[test]
    fn test_download_and_extract_pipeline() {
        let temp = tempfile::tempdir().unwrap();
        let archive = create_fake_uv_archive(temp.path());
        let url = format!("file://{}", archive.display());
        let cache = temp.path().join("cache");

        download_url_to_cache(&url, "uv-fake.tar.gz", None, &cache).unwrap();
        assert!(cache.join("uv").is_file());
    }

    #[test]
    fn test_sha256_known_value() {
        // SHA-256 of empty string is well-known
        let mut hasher = Sha256::new();
        hasher.update(b"");
        assert_eq!(
            hasher.hex_digest(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hello_world() {
        let mut hasher = Sha256::new();
        hasher.update(b"hello world");
        assert_eq!(
            hasher.hex_digest(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_verify_sha256_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("test.bin");
        std::fs::write(&file, "some data").unwrap();

        let err = verify_sha256(
            &file,
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap_err();
        assert!(err.contains("SHA-256 mismatch"));
    }

    #[test]
    fn test_download_with_sha256_verification() {
        let temp = tempfile::tempdir().unwrap();
        let archive = create_fake_uv_archive(temp.path());

        // Compute the real hash of the archive
        let data = std::fs::read(&archive).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hasher.hex_digest();

        let url = format!("file://{}", archive.display());
        let cache = temp.path().join("cache");
        download_url_to_cache(&url, "uv-fake.tar.gz", Some(&hash), &cache).unwrap();
        assert!(cache.join("uv").is_file());
    }

    #[test]
    fn test_download_with_wrong_sha256_fails() {
        let temp = tempfile::tempdir().unwrap();
        let archive = create_fake_uv_archive(temp.path());
        let url = format!("file://{}", archive.display());
        let cache = temp.path().join("cache");

        let err = download_url_to_cache(
            &url,
            "uv-fake.tar.gz",
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
            &cache,
        )
        .unwrap_err();
        assert!(err.contains("SHA-256 mismatch"));
    }

    #[test]
    fn test_cleanup_guard_removes_directory() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("cleanup-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("file.txt"), "data").unwrap();
        {
            let _guard = CleanupGuard(&dir);
        }
        assert!(!dir.exists());
    }

    #[test]
    fn test_download_invalid_cache_path() {
        let temp = tempfile::tempdir().unwrap();
        let archive = create_fake_uv_archive(temp.path());
        let url = format!("file://{}", archive.display());

        // Create a file where a directory is expected
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let bad_cache = blocker.join("hegel");

        let err = download_url_to_cache(&url, "uv-fake.tar.gz", None, &bad_cache).unwrap_err();
        assert!(err.contains("Failed to create cache directory"));
    }

    #[test]
    fn test_download_bad_url() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("hegel");

        let err = download_url_to_cache(
            "file:///nonexistent/fake.tar.gz",
            "fake.tar.gz",
            None,
            &cache,
        )
        .unwrap_err();
        assert!(err.contains("Failed to download uv"));
    }

    #[test]
    fn test_download_invalid_archive() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("hegel");

        // Create a fake non-tar file and serve it via file:// URL
        let fake_archive = temp.path().join("not-a-tar.tar.gz");
        std::fs::write(&fake_archive, "this is not a tar archive").unwrap();
        let url = format!("file://{}", fake_archive.display());

        let err = download_url_to_cache(&url, "not-a-tar.tar.gz", None, &cache).unwrap_err();
        assert!(err.contains("Failed to extract uv archive"));
    }
}
