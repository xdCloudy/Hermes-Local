#![allow(dead_code)] // PF-08 service foundation; preview UI wiring is a later stage.

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use url::Url;

const TEXT_PREVIEW_MAX_BYTES: u64 = 512 * 1024;
const BINARY_SAMPLE_BYTES: usize = 4096;
const SAFE_ENV_SUFFIXES: [&str; 4] = ["dist", "example", "sample", "template"];
const SENSITIVE_EXTENSIONS: [&str; 4] = ["kdbx", "p12", "pem", "pfx"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewKind {
    Html,
    Image,
    Binary,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewTarget {
    Url {
        label: String,
        source: String,
        url: String,
    },
    File {
        binary: bool,
        byte_size: u64,
        large: bool,
        label: String,
        language: String,
        mime_type: String,
        path: PathBuf,
        preview_kind: PreviewKind,
        source: String,
        url: String,
    },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PreviewNormalizationService;

impl PreviewNormalizationService {
    /// Normalize a URL or local file target using the Electron main-process
    /// preview contract as the oracle. This is native authority and therefore
    /// deliberately stays in the Desktop composition layer until PF-08 UI wiring.
    pub fn normalize(
        &self,
        raw_target: &str,
        base_dir: Option<&Path>,
    ) -> Result<Option<PreviewTarget>, String> {
        let raw = raw_target.trim();
        if raw.is_empty() {
            return Ok(None);
        }

        if starts_http_url(raw) {
            return normalize_url_target(raw).map(Some);
        }

        normalize_file_target(raw, base_dir)
    }
}

fn normalize_url_target(raw: &str) -> Result<PreviewTarget, String> {
    let mut url = Url::parse(raw).map_err(|_| "Preview URL is invalid.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Preview URL must use HTTP or HTTPS.".to_owned());
    }
    if url.host_str() == Some("0.0.0.0") {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| "Preview URL host is invalid.".to_owned())?;
    }

    let host = match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_owned(),
        (None, _) => String::new(),
    };
    let label = if url.path() == "/" {
        host
    } else {
        format!("{host}{}", url.path())
    };

    Ok(PreviewTarget::Url {
        label,
        source: raw.to_owned(),
        url: url.to_string(),
    })
}

fn normalize_file_target(
    raw: &str,
    base_dir: Option<&Path>,
) -> Result<Option<PreviewTarget>, String> {
    reject_unsafe_path_syntax(raw)?;
    let source = raw.to_owned();
    let expanded = expand_user_path(raw);
    let mut resolved = if starts_file_url(&expanded) {
        let url = Url::parse(&expanded).map_err(|_| "Preview file URL is invalid.".to_owned())?;
        if url.scheme() != "file" {
            return Ok(None);
        }
        url.to_file_path()
            .map_err(|_| "Preview file URL is invalid on this platform.".to_owned())?
    } else {
        let base = match base_dir {
            Some(base) => {
                let value = base.to_string_lossy();
                reject_unsafe_path_syntax(&value)?;
                lexical_absolute(base)?
            }
            None => std::env::current_dir()
                .map_err(|error| format!("Could not resolve preview base directory: {error}"))?,
        };
        let path = PathBuf::from(&expanded);
        if path.is_absolute() {
            lexical_normalize(&path)
        } else {
            lexical_normalize(&base.join(path))
        }
    };

    reject_unsafe_path_syntax(&resolved.to_string_lossy())?;
    if fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_dir()) {
        resolved = resolved.join("index.html");
    }
    if !resolved.exists() {
        return Ok(None);
    }

    reject_sensitive_file_path(&resolved)?;
    let metadata = fs::metadata(&resolved)
        .map_err(|error| format!("Could not inspect preview target: {error}"))?;
    if !metadata.is_file() {
        return Err("Preview target must be a regular file.".to_owned());
    }
    let real_path = normalize_canonical_path(
        resolved
            .canonicalize()
            .map_err(|error| format!("Could not resolve preview target: {error}"))?,
    );
    reject_unsafe_path_syntax(&real_path.to_string_lossy())?;
    reject_sensitive_file_path(&real_path)?;

    // Electron performs an explicit readability probe before returning the
    // target. Opening once here provides the same fail-closed contract.
    let mut file = fs::File::open(&resolved)
        .map_err(|error| format!("Preview target is not readable: {error}"))?;

    let extension = extension_key(&resolved);
    let mime_type = mime_type_for_extension(&extension).to_owned();
    let byte_size = metadata.len();
    let binary = if mime_type.starts_with("image/") {
        false
    } else {
        let mut sample = vec![0_u8; BINARY_SAMPLE_BYTES.min(byte_size as usize)];
        let bytes_read = file
            .read(&mut sample)
            .map_err(|error| format!("Could not sample preview target: {error}"))?;
        looks_binary(&sample[..bytes_read])
    };
    let preview_kind = if matches!(extension.as_str(), ".html" | ".htm") {
        PreviewKind::Html
    } else if mime_type.starts_with("image/") {
        PreviewKind::Image
    } else if binary {
        PreviewKind::Binary
    } else {
        PreviewKind::Text
    };
    let url = Url::from_file_path(&resolved)
        .map_err(|_| "Could not convert preview file path to a file URL.".to_owned())?
        .to_string();

    Ok(Some(PreviewTarget::File {
        binary,
        byte_size,
        large: byte_size > TEXT_PREVIEW_MAX_BYTES,
        label: resolved
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned(),
        language: language_for_extension(&extension).to_owned(),
        mime_type,
        path: resolved,
        preview_kind,
        source,
        url,
    }))
}

fn starts_http_url(value: &str) -> bool {
    value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        || value
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
}

fn starts_file_url(value: &str) -> bool {
    value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
}

fn expand_user_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed == "~" || trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        if let Some(home) = home_dir() {
            return home
                .join(
                    trimmed
                        .get(1..)
                        .unwrap_or_default()
                        .trim_start_matches(['/', '\\']),
                )
                .to_string_lossy()
                .into_owned();
        }
    }
    trimmed.to_owned()
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let keys = ["USERPROFILE", "HOME"];
    #[cfg(not(windows))]
    let keys = ["HOME", "USERPROFILE"];

    keys.into_iter()
        .find_map(|key| std::env::var_os(key).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(lexical_normalize(path))
    } else {
        std::env::current_dir()
            .map(|cwd| lexical_normalize(&cwd.join(path)))
            .map_err(|error| format!("Could not resolve preview base directory: {error}"))
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(output.components().next_back(), Some(Component::RootDir)) {
                    output.pop();
                }
            }
            _ => output.push(component.as_os_str()),
        }
    }
    output
}

fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

fn reject_unsafe_path_syntax(value: &str) -> Result<(), String> {
    let raw = value.trim();
    if raw.is_empty() {
        return Err("Preview target path is required.".to_owned());
    }
    if raw.contains('\0') {
        return Err("Preview target path contains a NUL character.".to_owned());
    }
    let normalized = raw.replace('\\', "/").to_ascii_lowercase();
    if normalized.starts_with("//?/")
        || normalized.starts_with("//./")
        || normalized.starts_with("globalroot/device/")
        || normalized.contains("/globalroot/device/")
    {
        return Err("Windows device paths are not allowed for previews.".to_owned());
    }
    Ok(())
}

fn reject_sensitive_file_path(path: &Path) -> Result<(), String> {
    if sensitive_file_block_reason(path).is_some() {
        Err(
            "Preview target is blocked because it may contain credentials or key material."
                .to_owned(),
        )
    } else {
        Ok(())
    }
}

fn sensitive_file_block_reason(path: &Path) -> Option<&'static str> {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let basename = normalized.rsplit('/').next().unwrap_or_default();
    let extension = basename.rsplit_once('.').map(|(_, extension)| extension);

    if normalized.contains("/.ssh/") {
        return Some("ssh");
    }
    if normalized.contains("/.gnupg/") {
        return Some("gpg");
    }
    if normalized.ends_with("/.aws/credentials") {
        return Some("aws");
    }
    if basename == ".env" {
        return Some("env");
    }
    if let Some(suffix) = basename.strip_prefix(".env.")
        && !SAFE_ENV_SUFFIXES.contains(&suffix)
    {
        return Some("env");
    }
    if is_private_key_basename(basename) {
        return Some("ssh-key");
    }
    if extension.is_some_and(|extension| SENSITIVE_EXTENSIONS.contains(&extension)) {
        return Some("certificate");
    }
    if matches!(basename, ".npmrc" | ".netrc" | ".pypirc") {
        return Some("credentials");
    }
    None
}

fn is_private_key_basename(basename: &str) -> bool {
    if basename.ends_with(".pub") {
        return false;
    }
    ["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"]
        .into_iter()
        .any(|prefix| basename == prefix || basename.starts_with(&format!("{prefix}.")))
}

fn extension_key(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn mime_type_for_extension(extension: &str) -> &'static str {
    match extension {
        ".avi" => "video/x-msvideo",
        ".bmp" => "image/bmp",
        ".flac" => "audio/flac",
        ".gif" => "image/gif",
        ".jpeg" | ".jpg" => "image/jpeg",
        ".m4a" => "audio/mp4",
        ".mkv" => "video/x-matroska",
        ".mov" => "video/quicktime",
        ".mp3" => "audio/mpeg",
        ".mp4" => "video/mp4",
        ".ogg" => "audio/ogg",
        ".opus" => "audio/ogg; codecs=opus",
        ".png" => "image/png",
        ".svg" => "image/svg+xml",
        ".wav" => "audio/wav",
        ".webm" => "video/webm",
        ".webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn language_for_extension(extension: &str) -> &'static str {
    match extension {
        ".c" => "c",
        ".conf" => "ini",
        ".cpp" => "cpp",
        ".css" => "css",
        ".csv" => "csv",
        ".go" => "go",
        ".graphql" => "graphql",
        ".h" => "c",
        ".hpp" => "cpp",
        ".html" => "html",
        ".java" => "java",
        ".js" | ".mjs" => "javascript",
        ".json" => "json",
        ".jsx" => "jsx",
        ".kt" => "kotlin",
        ".lua" => "lua",
        ".md" => "markdown",
        ".py" => "python",
        ".rb" => "ruby",
        ".rs" => "rust",
        ".sh" | ".zsh" => "shell",
        ".sql" => "sql",
        ".svg" | ".xml" => "xml",
        ".toml" => "toml",
        ".ts" => "typescript",
        ".tsx" => "tsx",
        ".txt" => "text",
        ".yaml" | ".yml" => "yaml",
        _ => "text",
    }
}

fn looks_binary(buffer: &[u8]) -> bool {
    if buffer.is_empty() {
        return false;
    }
    let mut suspicious = 0_usize;
    for byte in buffer {
        if *byte == 0 {
            return true;
        }
        if *byte < 32 && !matches!(*byte, 9 | 10 | 13) {
            suspicious += 1;
        }
    }
    (suspicious as f64 / buffer.len() as f64) > 0.12
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn url_normalization_preserves_http_contract_and_rewrites_wildcard_host() {
        let target = PreviewNormalizationService
            .normalize(" http://0.0.0.0:8080/docs?q=1 ", None)
            .expect("normalize")
            .expect("target");
        assert_eq!(
            target,
            PreviewTarget::Url {
                label: "127.0.0.1:8080/docs".to_owned(),
                source: "http://0.0.0.0:8080/docs?q=1".to_owned(),
                url: "http://127.0.0.1:8080/docs?q=1".to_owned(),
            }
        );
        assert!(
            PreviewNormalizationService
                .normalize("ftp://example.com/file", None)
                .expect("non-http target")
                .is_none()
        );
    }

    #[test]
    fn file_normalization_maps_directory_binary_image_and_large_metadata() {
        let root = test_directory("files");
        fs::write(root.join("index.html"), "<h1>hello</h1>\n").expect("index");
        let directory = PreviewNormalizationService
            .normalize(root.to_string_lossy().as_ref(), None)
            .expect("directory preview")
            .expect("directory target");
        let PreviewTarget::File {
            preview_kind,
            language,
            label,
            binary,
            ..
        } = directory
        else {
            panic!("expected file target");
        };
        assert_eq!(preview_kind, PreviewKind::Html);
        assert_eq!(language, "html");
        assert_eq!(label, "index.html");
        assert!(!binary);

        let binary_path = root.join("sample.bin");
        fs::write(&binary_path, [0_u8, 1, 2, 3]).expect("binary");
        let binary_target = PreviewNormalizationService
            .normalize(binary_path.to_string_lossy().as_ref(), None)
            .expect("binary preview")
            .expect("binary target");
        assert!(matches!(
            binary_target,
            PreviewTarget::File {
                binary: true,
                preview_kind: PreviewKind::Binary,
                ..
            }
        ));

        let image_path = root.join("image.png");
        fs::write(&image_path, [0_u8; 32]).expect("image");
        let image_target = PreviewNormalizationService
            .normalize(image_path.to_string_lossy().as_ref(), None)
            .expect("image preview")
            .expect("image target");
        assert!(matches!(
            image_target,
            PreviewTarget::File {
                binary: false,
                preview_kind: PreviewKind::Image,
                ..
            }
        ));

        let large_path = root.join("large.txt");
        fs::write(&large_path, vec![b'x'; TEXT_PREVIEW_MAX_BYTES as usize + 1]).expect("large");
        let large_target = PreviewNormalizationService
            .normalize(large_path.to_string_lossy().as_ref(), None)
            .expect("large preview")
            .expect("large target");
        assert!(matches!(
            large_target,
            PreviewTarget::File { large: true, .. }
        ));
        cleanup(root);
    }

    #[test]
    fn relative_paths_use_base_directory_and_missing_targets_fail_soft() {
        let root = test_directory("relative");
        fs::create_dir_all(root.join("nested")).expect("nested");
        fs::write(root.join("nested/readme.md"), "# hello\n").expect("markdown");
        let target = PreviewNormalizationService
            .normalize("nested/../nested/readme.md", Some(&root))
            .expect("relative")
            .expect("target");
        assert!(matches!(
            target,
            PreviewTarget::File {
                language,
                preview_kind: PreviewKind::Text,
                ..
            } if language == "markdown"
        ));
        assert!(
            PreviewNormalizationService
                .normalize("missing.txt", Some(&root))
                .expect("missing")
                .is_none()
        );
        cleanup(root);
    }

    #[test]
    fn sensitive_and_device_paths_fail_closed() {
        let root = test_directory("sensitive");
        fs::write(root.join(".env"), "TOKEN=secret\n").expect("env");
        assert!(
            PreviewNormalizationService
                .normalize(root.join(".env").to_string_lossy().as_ref(), None)
                .is_err()
        );
        fs::write(root.join(".env.example"), "SAFE=1\n").expect("safe env");
        assert!(
            PreviewNormalizationService
                .normalize(root.join(".env.example").to_string_lossy().as_ref(), None)
                .expect("safe env")
                .is_some()
        );
        assert!(reject_unsafe_path_syntax(r"\\?\C:\secret.txt").is_err());
        assert!(reject_unsafe_path_syntax(r"\\.\PhysicalDrive0").is_err());
        cleanup(root);
    }

    #[test]
    fn binary_detector_matches_electron_threshold() {
        assert!(!looks_binary(b"hello\nworld\t"));
        assert!(looks_binary(&[0, b'a']));
        assert!(!looks_binary(&[
            1, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'
        ]));
        assert!(looks_binary(&[
            1, 2, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'
        ]));
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "hermes-preview-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("test directory");
        directory
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
