//! Small native-only helpers for SSH lifecycle security.
//!
//! These deliberately use only the Desktop binary's existing dependency
//! surface so the migration does not widen the runtime graph merely to hash a
//! token, persist an opaque secret, or issue loopback ownership probes.

use std::{
    fs::{self, OpenOptions},
    io::{Read as _, Write as _},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use hermes_core::{ServiceError, ServiceResult};
use url::{Host, Url};

const HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_HTTP_BYTES: usize = 2 * 1024 * 1024;
const MAX_SECRET_BYTES: usize = 4_096;

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

pub fn random_hex(bytes: usize) -> ServiceResult<String> {
    if bytes == 0 || bytes > 64 {
        return Err(ServiceError::InvalidInput(
            "random byte request is outside the supported range".into(),
        ));
    }
    #[cfg(windows)]
    {
        let count = bytes.div_ceil(16);
        let script =
            format!("-join (1..{count}|ForEach-Object{{[Guid]::NewGuid().ToString('N')}})");
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(platform)?;
        if !output.status.success() {
            return Err(ServiceError::Platform(format!(
                "Windows random source failed: {}",
                sanitize(&String::from_utf8_lossy(&output.stderr))
            )));
        }
        let value = String::from_utf8_lossy(&output.stdout)
            .chars()
            .filter(char::is_ascii_hexdigit)
            .collect::<String>()
            .to_ascii_lowercase();
        if value.len() < bytes * 2 {
            return Err(ServiceError::Platform(
                "Windows random source returned too little entropy".into(),
            ));
        }
        Ok(value[..bytes * 2].to_owned())
    }
    #[cfg(not(windows))]
    {
        let mut random = fs::File::open("/dev/urandom").map_err(platform)?;
        let mut data = vec![0_u8; bytes];
        random.read_exact(&mut data).map_err(platform)?;
        Ok(hex(&data))
    }
}

pub fn new_uuid_v4() -> ServiceResult<String> {
    #[cfg(windows)]
    {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Guid]::NewGuid().ToString('D').ToLowerInvariant()",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(platform)?;
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if output.status.success() && is_uuid_v4(&value) {
            return Ok(value);
        }
        Err(ServiceError::Platform(format!(
            "could not create desktop installation id: {}",
            sanitize(&String::from_utf8_lossy(&output.stderr))
        )))
    }
    #[cfg(not(windows))]
    {
        let mut bytes = [0_u8; 16];
        fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(platform)?;
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Ok(format!(
            "{}-{}-{}-{}-{}",
            hex(&bytes[..4]),
            hex(&bytes[4..6]),
            hex(&bytes[6..8]),
            hex(&bytes[8..10]),
            hex(&bytes[10..])
        ))
    }
}

pub fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || bytes[8] != b'-'
        || bytes[13] != b'-'
        || bytes[18] != b'-'
        || bytes[23] != b'-'
        || bytes[14].to_ascii_lowercase() != b'4'
        || !matches!(bytes[19].to_ascii_lowercase(), b'8' | b'9' | b'a' | b'b')
    {
        return false;
    }
    bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit())
}

pub fn sha256_hex_prefix(input: &[u8], chars: usize) -> ServiceResult<String> {
    if chars == 0 || chars > 64 {
        return Err(ServiceError::InvalidInput(
            "SHA-256 prefix length is outside the supported range".into(),
        ));
    }
    let digest = sha256(input);
    Ok(hex(&digest)[..chars].to_owned())
}

pub fn load_protected_secret(
    data_dir: &Path,
    namespace: &str,
    account: &str,
) -> ServiceResult<Option<String>> {
    validate_secret_name(namespace)?;
    validate_secret_name(account)?;
    #[cfg(windows)]
    {
        let path = secret_path(data_dir, namespace, account)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(platform(error)),
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(ServiceError::PermissionDenied(
                "protected SSH secret path is not a regular file".into(),
            ));
        }
        let encrypted = fs::read(path).map_err(platform)?;
        if encrypted.is_empty() || encrypted.len() > MAX_SECRET_BYTES * 8 {
            return Err(ServiceError::PermissionDenied(
                "protected SSH secret is invalid".into(),
            ));
        }
        let clear = run_dpapi(false, &encrypted)?;
        if clear.is_empty() || clear.len() > MAX_SECRET_BYTES {
            return Err(ServiceError::PermissionDenied(
                "protected SSH secret decrypted to an invalid value".into(),
            ));
        }
        String::from_utf8(clear)
            .map(Some)
            .map_err(|_| ServiceError::PermissionDenied("protected SSH secret is not UTF-8".into()))
    }
    #[cfg(not(windows))]
    {
        let _ = (data_dir, namespace, account);
        Ok(None)
    }
}

pub fn store_protected_secret(
    data_dir: &Path,
    namespace: &str,
    account: &str,
    secret: &str,
) -> ServiceResult<()> {
    validate_secret_name(namespace)?;
    validate_secret_name(account)?;
    if secret.is_empty() || secret.len() > MAX_SECRET_BYTES {
        return Err(ServiceError::InvalidInput(
            "protected SSH secret has an invalid length".into(),
        ));
    }
    #[cfg(windows)]
    {
        let path = secret_path(data_dir, namespace, account)?;
        let parent = path.parent().ok_or_else(|| {
            ServiceError::Platform("protected SSH secret path has no parent".into())
        })?;
        fs::create_dir_all(parent).map_err(platform)?;
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && (!metadata.file_type().is_file() || metadata.file_type().is_symlink())
        {
            return Err(ServiceError::PermissionDenied(
                "refusing to replace a non-regular protected SSH secret path".into(),
            ));
        }
        let encrypted = run_dpapi(true, secret.as_bytes())?;
        let temporary = path.with_extension(format!("{}.tmp", random_hex(8)?));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(platform)?;
        file.write_all(&encrypted).map_err(platform)?;
        file.flush().map_err(platform)?;
        drop(file);
        if path.exists() {
            // Windows std::fs::rename does not replace an existing destination.
            // The destination has already been verified as a regular file; a
            // short truncate/write window is preferable to deleting it first.
            let replace = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)
                .and_then(|mut target| {
                    target.write_all(&encrypted)?;
                    target.flush()
                });
            let _ = fs::remove_file(&temporary);
            replace.map_err(platform)
        } else {
            match fs::rename(&temporary, &path) {
                Ok(()) => Ok(()),
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    Err(platform(error))
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (data_dir, namespace, account, secret);
        Ok(())
    }
}

pub async fn loopback_get(
    base_url: &str,
    path: &str,
    token: Option<&str>,
) -> ServiceResult<HttpResponse> {
    let base_url = base_url.to_owned();
    let path = path.to_owned();
    let token = token.map(str::to_owned);
    tokio::task::spawn_blocking(move || loopback_get_blocking(&base_url, &path, token.as_deref()))
        .await
        .map_err(|error| ServiceError::Platform(format!("loopback probe task failed: {error}")))?
}

fn loopback_get_blocking(
    base_url: &str,
    path: &str,
    token: Option<&str>,
) -> ServiceResult<HttpResponse> {
    if !path.starts_with('/') || path.contains('\r') || path.contains('\n') {
        return Err(ServiceError::InvalidInput(
            "invalid loopback HTTP path".into(),
        ));
    }
    let url = Url::parse(base_url).map_err(invalid)?;
    if url.scheme() != "http"
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ServiceError::InvalidInput(
            "SSH loopback base URL must be a plain HTTP origin".into(),
        ));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ServiceError::InvalidInput("loopback URL is missing a port".into()))?;
    let address = match url.host() {
        Some(Host::Ipv4(ip)) if ip == Ipv4Addr::LOCALHOST => SocketAddr::new(IpAddr::V4(ip), port),
        Some(Host::Domain("localhost")) => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        _ => {
            return Err(ServiceError::PermissionDenied(
                "SSH lifecycle HTTP is restricted to loopback".into(),
            ));
        }
    };
    if let Some(token) = token
        && (token.is_empty()
            || token.len() > MAX_SECRET_BYTES
            || token.contains('\r')
            || token.contains('\n'))
    {
        return Err(ServiceError::InvalidInput(
            "invalid loopback session token".into(),
        ));
    }
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT).map_err(transport)?;
    stream
        .set_read_timeout(Some(HTTP_TIMEOUT))
        .map_err(platform)?;
    stream
        .set_write_timeout(Some(HTTP_TIMEOUT))
        .map_err(platform)?;
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept-Encoding: identity\r\n"
    );
    if let Some(token) = token {
        request.push_str("X-Hermes-Session-Token: ");
        request.push_str(token);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).map_err(transport)?;
    let mut response = Vec::new();
    stream
        .take((MAX_HTTP_BYTES + 1) as u64)
        .read_to_end(&mut response)
        .map_err(transport)?;
    if response.len() > MAX_HTTP_BYTES {
        return Err(ServiceError::Transport(
            "loopback HTTP response exceeded the safety limit".into(),
        ));
    }
    parse_http_response(&response)
}

fn parse_http_response(response: &[u8]) -> ServiceResult<HttpResponse> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ServiceError::Transport("invalid loopback HTTP response".into()))?;
    let headers = String::from_utf8_lossy(&response[..split]);
    let mut lines = headers.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| ServiceError::Transport("loopback HTTP response has no status".into()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| (100..=599).contains(value))
        .ok_or_else(|| ServiceError::Transport("invalid loopback HTTP status".into()))?;
    let chunked = lines.any(|line| {
        let mut parts = line.splitn(2, ':');
        parts
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("transfer-encoding"))
            && parts
                .next()
                .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"))
    });
    let raw_body = &response[split + 4..];
    let body = if chunked {
        decode_chunked(raw_body)?
    } else {
        raw_body.to_vec()
    };
    Ok(HttpResponse {
        status,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn decode_chunked(mut input: &[u8]) -> ServiceResult<Vec<u8>> {
    let mut output = Vec::new();
    loop {
        let Some(end) = input.windows(2).position(|window| window == b"\r\n") else {
            return Err(ServiceError::Transport(
                "invalid chunked HTTP response".into(),
            ));
        };
        let size_text = String::from_utf8_lossy(&input[..end]);
        let size =
            usize::from_str_radix(size_text.split(';').next().unwrap_or_default().trim(), 16)
                .map_err(|_| ServiceError::Transport("invalid HTTP chunk size".into()))?;
        input = &input[end + 2..];
        if size == 0 {
            return Ok(output);
        }
        if input.len() < size + 2 || &input[size..size + 2] != b"\r\n" {
            return Err(ServiceError::Transport(
                "truncated chunked HTTP response".into(),
            ));
        }
        if output.len().saturating_add(size) > MAX_HTTP_BYTES {
            return Err(ServiceError::Transport(
                "chunked HTTP body exceeded the safety limit".into(),
            ));
        }
        output.extend_from_slice(&input[..size]);
        input = &input[size + 2..];
    }
}

#[cfg(windows)]
fn run_dpapi(protect: bool, input: &[u8]) -> ServiceResult<Vec<u8>> {
    let method = if protect { "Protect" } else { "Unprotect" };
    let script = format!(
        "$i=[Console]::OpenStandardInput();$m=New-Object IO.MemoryStream;$i.CopyTo($m);$b=$m.ToArray();$o=[Security.Cryptography.ProtectedData]::{method}($b,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser);$s=[Console]::OpenStandardOutput();$s.Write($o,0,$o.Length)"
    );
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(platform)?;
    child
        .stdin
        .take()
        .ok_or_else(|| ServiceError::Platform("DPAPI helper stdin was unavailable".into()))?
        .write_all(input)
        .map_err(platform)?;
    let output = child.wait_with_output().map_err(platform)?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err(ServiceError::PermissionDenied(format!(
            "Windows DPAPI operation failed: {}",
            sanitize(&String::from_utf8_lossy(&output.stderr))
        )));
    }
    Ok(output.stdout)
}

#[cfg(windows)]
fn secret_path(data_dir: &Path, namespace: &str, account: &str) -> ServiceResult<PathBuf> {
    validate_secret_name(namespace)?;
    validate_secret_name(account)?;
    Ok(data_dir
        .join("protected-secrets")
        .join(namespace)
        .join(format!("{account}.dpapi")))
}

fn validate_secret_name(value: &str) -> ServiceResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ServiceError::InvalidInput(
            "invalid protected secret identifier".into(),
        ));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
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

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut output = [0_u8; 32];
    for (index, word) in h.into_iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(1_024)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn invalid(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::InvalidInput(error.to_string())
}

fn platform(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Platform(error.to_string())
}

fn transport(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex_prefix(b"abc", 64).expect("sha"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn uuid_validation_is_strictly_v4() {
        assert!(is_uuid_v4("123e4567-e89b-42d3-a456-426614174000"));
        assert!(!is_uuid_v4("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!is_uuid_v4("not-a-guid"));
    }

    #[test]
    fn loopback_origin_rejects_non_loopback() {
        let error = loopback_get_blocking("http://192.0.2.1:9000", "/", None)
            .expect_err("non-loopback must fail");
        assert!(matches!(error, ServiceError::PermissionDenied(_)));
    }

    #[test]
    fn chunked_decoder_is_bounded_and_exact() {
        assert_eq!(
            decode_chunked(b"4\r\ntest\r\n3\r\n123\r\n0\r\n\r\n").expect("decode"),
            b"test123"
        );
        assert!(decode_chunked(b"4\r\nxy").is_err());
    }
}
