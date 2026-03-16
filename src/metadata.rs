//! Schema 2.0 metadata gathering and envelope construction.
//!
//! Reads git info and Cargo.toml to populate the envelope fields.
//! Provides envelope wrapping for output and unwrapping for input.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

const TOOL_NAME: &str = "probe-rust";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

// =============================================================================
// Envelope types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SourceInfo {
    pub repo: String,
    pub commit: String,
    pub language: String,
    pub package: String,
    pub package_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Envelope<T> {
    pub schema: String,
    pub schema_version: String,
    pub tool: ToolInfo,
    pub source: SourceInfo,
    pub timestamp: String,
    pub data: T,
}

// =============================================================================
// Project metadata
// =============================================================================

#[derive(Debug, Clone)]
pub struct ProjectMetadata {
    pub commit: String,
    pub repo: String,
    pub timestamp: String,
    pub pkg_name: String,
    pub pkg_version: String,
}

/// Walk up the directory tree from `starting_path` looking for `Cargo.toml`.
pub fn find_project_root(starting_path: &Path) -> Option<PathBuf> {
    let mut current = if starting_path.is_file() {
        starting_path.parent()?.to_path_buf()
    } else {
        starting_path.to_path_buf()
    };
    loop {
        if current.join("Cargo.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Gather all project metadata in one pass.
pub fn gather_metadata(project_path: &Path) -> ProjectMetadata {
    let commit = run_cmd_or_default("git", &["rev-parse", "HEAD"], Some(project_path), "");
    let repo = run_cmd_or_default(
        "git",
        &["remote", "get-url", "origin"],
        Some(project_path),
        "",
    );
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let (pkg_name, pkg_version) = read_cargo_package_info(project_path, &commit);

    ProjectMetadata {
        commit,
        repo,
        timestamp,
        pkg_name,
        pkg_version,
    }
}

// =============================================================================
// Envelope wrapping / unwrapping
// =============================================================================

/// Wrap data in a Schema 2.0 envelope.
pub fn wrap_in_envelope<T: Serialize>(
    schema: &str,
    command: &str,
    data: T,
    metadata: &ProjectMetadata,
) -> Envelope<T> {
    Envelope {
        schema: schema.to_string(),
        schema_version: "2.0".to_string(),
        tool: ToolInfo {
            name: TOOL_NAME.to_string(),
            version: TOOL_VERSION.to_string(),
            command: command.to_string(),
        },
        source: SourceInfo {
            repo: metadata.repo.clone(),
            commit: metadata.commit.clone(),
            language: "rust".to_string(),
            package: metadata.pkg_name.clone(),
            package_version: metadata.pkg_version.clone(),
        },
        timestamp: metadata.timestamp.clone(),
        data,
    }
}

/// Extract the data payload from JSON, unwrapping the Schema 2.0 envelope if present.
pub fn unwrap_envelope(json: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(mut map) = json {
        let is_envelope = matches!(
            map.get("schema"),
            Some(serde_json::Value::String(s)) if s.starts_with("probe-rust/")
        ) && map.contains_key("schema-version")
            && map.contains_key("data");
        if is_envelope {
            if let Some(data) = map.remove("data") {
                return data;
            }
        }
        serde_json::Value::Object(map)
    } else {
        json
    }
}

// =============================================================================
// Default output paths
// =============================================================================

/// Compute the default output path: `.verilib/probes/rust_<pkg>_<ver>[_<suffix>].json`
pub fn get_default_output_path(
    project_root: &Path,
    metadata: &ProjectMetadata,
    suffix: &str,
) -> PathBuf {
    let pkg = if metadata.pkg_name.is_empty() {
        "unknown"
    } else {
        &metadata.pkg_name
    };
    let ver = if metadata.pkg_version.is_empty() {
        "unknown"
    } else {
        &metadata.pkg_version
    };

    let filename = if suffix.is_empty() {
        format!("rust_{}_{}.json", pkg, ver)
    } else {
        format!("rust_{}_{}_{}.json", pkg, ver, suffix)
    };

    project_root.join(".verilib").join("probes").join(filename)
}

// =============================================================================
// Internal helpers
// =============================================================================

fn run_cmd_or_default(cmd: &str, args: &[&str], cwd: Option<&Path>, default: &str) -> String {
    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    match command.output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => default.to_string(),
    }
}

/// For a workspace-only Cargo.toml (no `[package]`), try the single workspace member's
/// Cargo.toml to get a package name and version.
fn try_workspace_member_info(
    project_path: &Path,
    table: &toml::Table,
    version_fallback: &dyn Fn() -> String,
) -> Option<(String, String)> {
    let workspace = table.get("workspace")?.as_table()?;
    let members = workspace.get("members")?.as_array()?;
    if members.len() != 1 {
        return None;
    }
    let member_dir = members[0].as_str()?;
    let member_toml = project_path.join(member_dir).join("Cargo.toml");
    let member_content = std::fs::read_to_string(&member_toml).ok()?;
    let member_table: toml::Table = member_content.parse().ok()?;
    let pkg = member_table.get("package")?.as_table()?;
    let name = pkg
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let version = pkg
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(version_fallback);
    Some((name, version))
}

/// Read package name and version from Cargo.toml.
/// When the version field is absent, falls back to the 7-char git short hash.
fn read_cargo_package_info(project_path: &Path, commit: &str) -> (String, String) {
    let version_fallback = || -> String {
        if commit.len() >= 7 {
            commit[..7].to_string()
        } else {
            "unknown".to_string()
        }
    };

    let cargo_toml_path = project_path.join("Cargo.toml");
    let content = match std::fs::read_to_string(&cargo_toml_path) {
        Ok(c) => c,
        Err(_) => return ("unknown".to_string(), version_fallback()),
    };

    let table: toml::Table = match content.parse() {
        Ok(t) => t,
        Err(_) => return ("unknown".to_string(), version_fallback()),
    };

    let package = match table.get("package").and_then(|p| p.as_table()) {
        Some(p) => p,
        None => {
            if let Some((name, version)) =
                try_workspace_member_info(project_path, &table, &version_fallback)
            {
                return (name, version);
            }
            let dir_name = project_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            return (dir_name, version_fallback());
        }
    };

    let name = package
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let version = package
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(version_fallback);

    (name, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unwrap_envelope_with_envelope() {
        let json = serde_json::json!({
            "schema": "probe-rust/atoms",
            "schema-version": "2.0",
            "tool": { "name": "probe-rust", "version": "0.1.0", "command": "extract" },
            "source": {
                "repo": "https://github.com/org/proj",
                "commit": "abc123",
                "language": "rust",
                "package": "my-crate",
                "package-version": "1.0.0"
            },
            "timestamp": "2026-03-06T12:00:00Z",
            "data": {
                "probe:my-crate/1.0.0/func()": {
                    "display-name": "func",
                    "dependencies": [],
                    "code-module": "",
                    "code-path": "src/lib.rs",
                    "code-text": { "lines-start": 1, "lines-end": 10 },
                    "kind": "exec",
                    "language": "rust"
                }
            }
        });

        let data = unwrap_envelope(json);
        assert!(data.is_object());
        assert!(data.get("probe:my-crate/1.0.0/func()").is_some());
        assert!(data.get("schema").is_none());
    }

    #[test]
    fn test_unwrap_envelope_bare_dict() {
        let json = serde_json::json!({
            "probe:my-crate/1.0.0/func()": {
                "display-name": "func",
                "dependencies": [],
                "code-module": "",
                "code-path": "src/lib.rs",
                "code-text": { "lines-start": 1, "lines-end": 10 },
                "kind": "exec"
            }
        });

        let data = unwrap_envelope(json.clone());
        assert_eq!(data, json);
    }

    #[test]
    fn test_get_default_output_path_extract() {
        let meta = ProjectMetadata {
            commit: "abc".to_string(),
            repo: "".to_string(),
            timestamp: "".to_string(),
            pkg_name: "my-crate".to_string(),
            pkg_version: "1.0.0".to_string(),
        };
        let path = get_default_output_path(Path::new("/project"), &meta, "");
        assert_eq!(
            path,
            PathBuf::from("/project/.verilib/probes/rust_my-crate_1.0.0.json")
        );
    }

    #[test]
    fn test_get_default_output_path_no_suffix() {
        let meta = ProjectMetadata {
            commit: "abc".to_string(),
            repo: "".to_string(),
            timestamp: "".to_string(),
            pkg_name: "my-crate".to_string(),
            pkg_version: "1.0.0".to_string(),
        };
        let path = get_default_output_path(Path::new("/project"), &meta, "");
        assert_eq!(
            path,
            PathBuf::from("/project/.verilib/probes/rust_my-crate_1.0.0.json")
        );
    }

    #[test]
    fn test_find_project_root_at_root() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        assert_eq!(
            find_project_root(tmp.path()),
            Some(tmp.path().to_path_buf())
        );
    }

    #[test]
    fn test_find_project_root_from_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let sub = tmp.path().join("src").join("commands");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(find_project_root(&sub), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_wrap_in_envelope_roundtrip() {
        let data = serde_json::json!({"key": "value"});
        let meta = ProjectMetadata {
            commit: "abc123".to_string(),
            repo: "https://github.com/org/proj".to_string(),
            timestamp: "2026-03-06T12:00:00Z".to_string(),
            pkg_name: "my-crate".to_string(),
            pkg_version: "1.0.0".to_string(),
        };

        let envelope = wrap_in_envelope("probe-rust/atoms", "extract", data.clone(), &meta);
        assert_eq!(envelope.schema, "probe-rust/atoms");
        assert_eq!(envelope.tool.name, "probe-rust");
        assert_eq!(envelope.tool.command, "extract");
        assert_eq!(envelope.source.package, "my-crate");

        let serialized = serde_json::to_value(&envelope).unwrap();
        let unwrapped = unwrap_envelope(serialized);
        assert_eq!(unwrapped, data);
    }

    #[test]
    fn test_read_cargo_package_info_workspace_single_member() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"my-crate\"]\n",
        )
        .unwrap();

        let member = root.join("my-crate");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.3.0\"\n",
        )
        .unwrap();

        let (name, version) = read_cargo_package_info(root, "abcdef1234567890");
        assert_eq!(name, "my-crate");
        assert_eq!(version, "0.3.0");
    }
}
