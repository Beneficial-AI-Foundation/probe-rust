//! Tool manager for auto-downloading external dependencies.
//!
//! Manages rust-analyzer and scip binaries: resolves their location
//! (managed directory, then PATH), and downloads scip on demand.
//! rust-analyzer must be installed via rustup.
//!
//! Version resolution order for scip:
//! 1. Environment variable override (`PROBE_SCIP_VERSION`)
//! 2. Latest stable release from GitHub API
//! 3. Known-good fallback version (compiled into the binary)

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SCIP_FALLBACK_VERSION: &str = "v0.6.1";
const SCIP_VERSION_ENV: &str = "PROBE_SCIP_VERSION";
const SCIP_REPO: &str = "sourcegraph/scip";

// ---------------------------------------------------------------------------
// Tool enum
// ---------------------------------------------------------------------------

/// An external tool that probe-rust can manage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    RustAnalyzer,
    Scip,
}

impl Tool {
    pub fn binary_name(&self) -> &'static str {
        match self {
            Tool::RustAnalyzer => "rust-analyzer",
            Tool::Scip => "scip",
        }
    }

    fn managed_filename(&self) -> &'static str {
        match self {
            Tool::RustAnalyzer => "rust-analyzer",
            Tool::Scip => "scip",
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.binary_name())
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ToolError {
    PlatformNotSupported(Tool, String),
    DownloadFailed(Tool, String),
    DecompressFailed(Tool, String),
    IoError(Tool, io::Error),
    NotInstalled(Tool),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::PlatformNotSupported(tool, detail) => {
                write!(f, "{tool}: platform not supported ({detail}).")
            }
            ToolError::DownloadFailed(tool, msg) => {
                write!(f, "{tool}: download failed: {msg}")
            }
            ToolError::DecompressFailed(tool, msg) => {
                write!(f, "{tool}: decompression failed: {msg}")
            }
            ToolError::IoError(tool, e) => {
                write!(f, "{tool}: I/O error: {e}")
            }
            ToolError::NotInstalled(tool) => match tool {
                Tool::RustAnalyzer => write!(
                    f,
                    "rust-analyzer not found. Install it with: rustup component add rust-analyzer"
                ),
                Tool::Scip => write!(
                    f,
                    "scip not found. Install it with: probe-rust setup\n\
                     Or download manually from https://github.com/sourcegraph/scip/releases"
                ),
            },
        }
    }
}

impl std::error::Error for ToolError {}

// ---------------------------------------------------------------------------
// Managed tools directory
// ---------------------------------------------------------------------------

/// `~/.probe-rust/tools`
pub fn tools_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".probe-rust").join("tools"))
}

fn managed_path(tool: &Tool) -> Option<PathBuf> {
    tools_dir().map(|d| d.join(tool.managed_filename()))
}

// ---------------------------------------------------------------------------
// Platform mapping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub arch: &'static str,
}

pub fn current_platform() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

/// Map (os, arch) -> the scip asset (os, arch) pair.
fn scip_target(p: &PlatformInfo) -> Result<(&'static str, &'static str), String> {
    match (p.os, p.arch) {
        ("linux", "x86_64") => Ok(("linux", "amd64")),
        ("linux", "aarch64") => Ok(("linux", "arm64")),
        ("macos", "x86_64") => Ok(("darwin", "amd64")),
        ("macos", "aarch64") => Ok(("darwin", "arm64")),
        _ => Err(format!("{}-{} (scip has no Windows binary)", p.os, p.arch)),
    }
}

// ---------------------------------------------------------------------------
// Version resolution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSource {
    EnvVar,
    GitHubLatest,
    Fallback,
}

impl std::fmt::Display for VersionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionSource::EnvVar => write!(f, "env"),
            VersionSource::GitHubLatest => write!(f, "latest"),
            VersionSource::Fallback => write!(f, "fallback"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub tag: String,
    pub source: VersionSource,
}

/// Resolve the version to install for scip.
pub fn resolve_version() -> ResolvedVersion {
    if let Ok(v) = std::env::var(SCIP_VERSION_ENV) {
        if !v.is_empty() {
            return ResolvedVersion {
                tag: v,
                source: VersionSource::EnvVar,
            };
        }
    }

    if let Some(tag) = fetch_latest_release_tag(SCIP_REPO) {
        return ResolvedVersion {
            tag,
            source: VersionSource::GitHubLatest,
        };
    }

    ResolvedVersion {
        tag: SCIP_FALLBACK_VERSION.to_string(),
        source: VersionSource::Fallback,
    }
}

fn fetch_latest_release_tag(repo: &str) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let response = ureq::get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "probe-rust")
        .call()
        .ok()?;

    let body_str = response.into_string().ok()?;
    let body: serde_json::Value = serde_json::from_str(&body_str).ok()?;
    body.get("tag_name")
        .and_then(|v| v.as_str())
        .map(String::from)
}

// ---------------------------------------------------------------------------
// Download URL construction
// ---------------------------------------------------------------------------

pub fn download_url_with_version(
    version: &str,
    platform: &PlatformInfo,
) -> Result<String, ToolError> {
    let (os, arch) =
        scip_target(platform).map_err(|d| ToolError::PlatformNotSupported(Tool::Scip, d))?;
    Ok(format!(
        "https://github.com/{SCIP_REPO}/releases/download/{version}/scip-{os}-{arch}.tar.gz",
    ))
}

// ---------------------------------------------------------------------------
// Resolution: managed dir → PATH → not found
// ---------------------------------------------------------------------------

/// Resolve a tool to an absolute path. Checks managed dir first, then PATH.
pub fn resolve_tool(tool: Tool) -> Result<PathBuf, ToolError> {
    if let Some(p) = managed_path(&tool) {
        if p.exists() {
            return Ok(p);
        }
    }

    if let Some(p) = find_in_path(tool.binary_name()) {
        return Ok(p);
    }

    Err(ToolError::NotInstalled(tool))
}

/// Resolve, or auto-download if `auto_install` is true. Only scip is downloadable.
pub fn resolve_or_install(tool: Tool, auto_install: bool) -> Result<PathBuf, ToolError> {
    match resolve_tool(tool) {
        Ok(p) => Ok(p),
        Err(ToolError::NotInstalled(_)) if auto_install && tool == Tool::Scip => {
            eprintln!("{tool} not found, downloading...");
            download_scip()?;
            resolve_tool(tool)
        }
        Err(e) => Err(e),
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let cmd = if cfg!(windows) { "where" } else { "which" };
    Command::new(cmd)
        .arg(name)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            let line = s.lines().next()?.trim().to_string();
            if line.is_empty() {
                None
            } else {
                Some(PathBuf::from(line))
            }
        })
}

// ---------------------------------------------------------------------------
// Download + decompress (scip only)
// ---------------------------------------------------------------------------

/// Download and install scip into the managed directory.
pub fn download_scip() -> Result<PathBuf, ToolError> {
    let resolved = resolve_version();
    let platform = current_platform();
    let url = download_url_with_version(&resolved.tag, &platform)?;

    let dest_dir = tools_dir().ok_or_else(|| {
        ToolError::IoError(
            Tool::Scip,
            io::Error::new(io::ErrorKind::NotFound, "cannot determine home directory"),
        )
    })?;
    fs::create_dir_all(&dest_dir).map_err(|e| ToolError::IoError(Tool::Scip, e))?;

    let dest_path = dest_dir.join(Tool::Scip.managed_filename());

    eprintln!(
        "Downloading scip {} ({}) from:",
        resolved.tag, resolved.source
    );
    eprintln!("  {url}");

    let response = ureq::get(&url)
        .call()
        .map_err(|e| ToolError::DownloadFailed(Tool::Scip, e.to_string()))?;

    let mut compressed_bytes: Vec<u8> = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut compressed_bytes)
        .map_err(|e| ToolError::DownloadFailed(Tool::Scip, e.to_string()))?;

    extract_tar_gz(&compressed_bytes, &dest_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&dest_path, perms).map_err(|e| ToolError::IoError(Tool::Scip, e))?;
    }

    eprintln!("Installed scip to {}", dest_path.display());
    Ok(dest_path)
}

fn extract_tar_gz(data: &[u8], dest: &Path) -> Result<(), ToolError> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);

    let binary_name = "scip";
    let mut found = false;

    for entry_result in archive
        .entries()
        .map_err(|e| ToolError::DecompressFailed(Tool::Scip, e.to_string()))?
    {
        let mut entry =
            entry_result.map_err(|e| ToolError::DecompressFailed(Tool::Scip, e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| ToolError::DecompressFailed(Tool::Scip, e.to_string()))?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if file_name == binary_name {
            let mut out = fs::File::create(dest).map_err(|e| ToolError::IoError(Tool::Scip, e))?;
            io::copy(&mut entry, &mut out)
                .map_err(|e| ToolError::DecompressFailed(Tool::Scip, e.to_string()))?;
            found = true;
            break;
        }
    }

    if !found {
        return Err(ToolError::DecompressFailed(
            Tool::Scip,
            format!("binary '{binary_name}' not found in archive"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_mapping_scip() {
        let linux_x86 = PlatformInfo {
            os: "linux",
            arch: "x86_64",
        };
        assert_eq!(scip_target(&linux_x86).unwrap(), ("linux", "amd64"));

        let mac_arm = PlatformInfo {
            os: "macos",
            arch: "aarch64",
        };
        assert_eq!(scip_target(&mac_arm).unwrap(), ("darwin", "arm64"));

        let win = PlatformInfo {
            os: "windows",
            arch: "x86_64",
        };
        assert!(scip_target(&win).is_err());
    }

    #[test]
    fn test_download_url_scip_mac_arm() {
        let platform = PlatformInfo {
            os: "macos",
            arch: "aarch64",
        };
        let url = download_url_with_version("v0.6.1", &platform).unwrap();
        assert_eq!(
            url,
            "https://github.com/sourcegraph/scip/releases/download/v0.6.1/scip-darwin-arm64.tar.gz"
        );
    }

    #[test]
    fn test_tool_binary_names() {
        assert_eq!(Tool::RustAnalyzer.binary_name(), "rust-analyzer");
        assert_eq!(Tool::Scip.binary_name(), "scip");
    }

    #[test]
    fn test_tools_dir() {
        let dir = tools_dir();
        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert!(dir.ends_with(".probe-rust/tools"));
    }

    #[test]
    fn test_resolve_version_env_override() {
        use std::sync::Mutex;
        static ENV_MUTEX: Mutex<()> = Mutex::new(());
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe { std::env::set_var(SCIP_VERSION_ENV, "custom-version") };
        let resolved = resolve_version();
        unsafe { std::env::remove_var(SCIP_VERSION_ENV) };
        assert_eq!(resolved.tag, "custom-version");
        assert_eq!(resolved.source, VersionSource::EnvVar);
    }
}
