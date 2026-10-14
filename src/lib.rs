// CoreCLR debugging for Zed via Samsung netcoredbg.
// Asset table: osx-arm64 ships .zip (3.2.0+); Intel macOS was dropped from the
// latest release and stays pinned to the last tag that shipped osx-amd64.
// Verified against the real binary: line + conditional breakpoints, output
// capture, attach + detach-without-kill. netcoredbg ignores logpoints and
// hit-count conditions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use zed_extension_api::{
    self as zed, serde_json, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario,
    DownloadedFileType, StartDebuggingRequestArguments, StartDebuggingRequestArgumentsRequest,
    Worktree,
};

const ADAPTER: &str = "netcoredbg";
const LATEST: &str = "3.2.0-1092";
const INTEL_MAC: &str = "3.1.3-1062";

struct NetCoreDbg {
    cached: Option<String>,
}

/// (asset name, release tag, archive kind) for the platform we are running on.
fn asset_for(os: zed::Os, arch: zed::Architecture) -> Result<(&'static str, &'static str, DownloadedFileType), String> {
    match (os, arch) {
        (zed::Os::Mac, zed::Architecture::Aarch64) => {
            Ok(("netcoredbg-osx-arm64.zip", LATEST, DownloadedFileType::Zip))
        }
        (zed::Os::Mac, zed::Architecture::X8664) => Ok((
            "netcoredbg-osx-amd64.tar.gz",
            INTEL_MAC,
            DownloadedFileType::GzipTar,
        )),
        (zed::Os::Linux, zed::Architecture::X8664) => Ok((
            "netcoredbg-linux-amd64.tar.gz",
            LATEST,
            DownloadedFileType::GzipTar,
        )),
        (zed::Os::Linux, zed::Architecture::Aarch64) => Ok((
            "netcoredbg-linux-arm64.tar.gz",
            LATEST,
            DownloadedFileType::GzipTar,
        )),
        (zed::Os::Windows, zed::Architecture::X8664) => {
            Ok(("netcoredbg-win64.zip", LATEST, DownloadedFileType::Zip))
        }
        _ => Err("netcoredbg: unsupported platform (no 32-bit, no Windows arm64)".to_string()),
    }
}

fn exe_name() -> &'static str {
    if zed::current_platform().0 == zed::Os::Windows {
        "netcoredbg.exe"
    } else {
        "netcoredbg"
    }
}

/// Depth-first search for the adapter executable inside an extracted archive.
fn find_binary(dir: &Path, exe: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|n| n == exe) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_binary(&path, exe) {
                return Some(found);
            }
        }
    }
    None
}

fn request_of(config: &serde_json::Value) -> Result<StartDebuggingRequestArgumentsRequest, String> {
    match config.get("request").and_then(|v| v.as_str()) {
        Some("launch") => Ok(StartDebuggingRequestArgumentsRequest::Launch),
        Some("attach") => Ok(StartDebuggingRequestArgumentsRequest::Attach),
        other => Err(format!("netcoredbg: 'request' must be launch or attach, got {other:?}")),
    }
}

impl NetCoreDbg {
    /// Downloads the adapter once per release tag and caches the executable path.
    fn binary_path(&mut self) -> Result<String, String> {
        if let Some(path) = &self.cached {
            if Path::new(path).exists() {
                return Ok(path.clone());
            }
        }
        let (os, arch) = zed::current_platform();
        let (asset, tag, file_type) = asset_for(os, arch)?;
        let exe = exe_name();
        let dir = format!("netcoredbg_v{tag}");

        let relative = match find_binary(Path::new(&dir), exe) {
            Some(path) => path,
            None => {
                let url = format!(
                    "https://github.com/Samsung/netcoredbg/releases/download/{tag}/{asset}"
                );
                zed::download_file(&url, &dir, file_type).map_err(|e| {
                    format!(
                        "netcoredbg: download failed ({e}). url={url} tag={tag}. \
                         Offline? Set `dap.netcoredbg.binary` to a local netcoredbg executable."
                    )
                })?;
                find_binary(Path::new(&dir), exe).ok_or_else(|| {
                    format!("netcoredbg: '{exe}' not found in {dir} after downloading {asset}")
                })?
            }
        };

        let absolute = std::env::current_dir()
            .map_err(|e| format!("netcoredbg: no working directory: {e}"))?
            .join(relative);
        let path = absolute.to_string_lossy().to_string();
        zed::make_file_executable(&path)?;
        self.cached = Some(path.clone());
        Ok(path)
    }
}

impl zed::Extension for NetCoreDbg {
    fn new() -> Self {
        NetCoreDbg { cached: None }
    }

    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: zed::DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &Worktree,
    ) -> Result<DebugAdapterBinary, String> {
        if adapter_name != ADAPTER {
            return Err(format!("netcoredbg: unknown adapter '{adapter_name}'"));
        }
        let parsed: serde_json::Value = serde_json::from_str(&config.config)
            .map_err(|e| format!("netcoredbg: invalid configuration JSON: {e}"))?;
        let request = request_of(&parsed)?;

        let command = match user_provided_debug_adapter_path {
            Some(path) => path,
            None => self.binary_path()?,
        };

        let envs = parsed
            .get("env")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let cwd = parsed
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| worktree.root_path());

        Ok(DebugAdapterBinary {
            command: Some(command),
            arguments: vec!["--interpreter=vscode".to_string()],
            envs,
            cwd: Some(cwd),
            connection: None,
            request_args: StartDebuggingRequestArguments {
                configuration: config.config,
                request,
            },
        })
    }

    fn dap_request_kind(
        &mut self,
        adapter_name: String,
        config: serde_json::Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest, String> {
        if adapter_name != ADAPTER {
            return Err(format!("netcoredbg: unknown adapter '{adapter_name}'"));
        }
        request_of(&config)
    }

    /// Feeds the "new process" modal (NOT validated in the UI; v1 contract is .zed/debug.json).
    /// No build step here (`build: None`), so the program may be stale.
    fn dap_config_to_scenario(&mut self, config: DebugConfig) -> Result<DebugScenario, String> {
        let payload = match config.request {
            DebugRequest::Launch(launch) => {
                if !(launch.program.ends_with(".dll") || launch.program.ends_with(".exe")) {
                    return Err("netcoredbg: program must be a built *.dll or *.exe".to_string());
                }
                let mut payload = serde_json::json!({
                    "request": "launch",
                    "program": launch.program,
                    "args": launch.args,
                    "env": launch.envs.into_iter().collect::<HashMap<String, String>>(),
                });
                if let Some(cwd) = launch.cwd {
                    payload["cwd"] = serde_json::Value::String(cwd);
                }
                payload
            }
            DebugRequest::Attach(attach) => {
                let process_id = attach.process_id.ok_or(
                    "netcoredbg: the process picker is not supported for .NET; \
                     put the PID in .zed/debug.json (\"processId\")",
                )?;
                serde_json::json!({ "request": "attach", "processId": process_id })
            }
        };
        Ok(DebugScenario {
            label: config.label,
            adapter: config.adapter,
            build: None,
            config: payload.to_string(),
            tcp_connection: None,
        })
    }
}

zed::register_extension!(NetCoreDbg);

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(os: zed::Os, arch: zed::Architecture) -> (&'static str, &'static str) {
        let (name, tag, _) = asset_for(os, arch).unwrap();
        (name, tag)
    }

    #[test]
    fn mac_arm64_uses_zip() {
        assert_eq!(
            asset(zed::Os::Mac, zed::Architecture::Aarch64),
            ("netcoredbg-osx-arm64.zip", LATEST)
        );
    }

    #[test]
    fn intel_mac_is_pinned_to_the_last_amd64_tag() {
        assert_eq!(
            asset(zed::Os::Mac, zed::Architecture::X8664),
            ("netcoredbg-osx-amd64.tar.gz", INTEL_MAC)
        );
        assert_ne!(LATEST, INTEL_MAC);
    }

    #[test]
    fn linux_and_windows_x64_use_latest() {
        assert_eq!(
            asset(zed::Os::Linux, zed::Architecture::X8664),
            ("netcoredbg-linux-amd64.tar.gz", LATEST)
        );
        assert_eq!(
            asset(zed::Os::Windows, zed::Architecture::X8664),
            ("netcoredbg-win64.zip", LATEST)
        );
    }

    #[test]
    fn unsupported_platforms_error() {
        assert!(asset_for(zed::Os::Windows, zed::Architecture::Aarch64).is_err());
        assert!(asset_for(zed::Os::Linux, zed::Architecture::X86).is_err());
    }

    #[test]
    fn request_kind_requires_request_field() {
        assert!(request_of(&serde_json::json!({})).is_err());
        assert!(request_of(&serde_json::json!({ "request": "launch" })).is_ok());
        assert!(request_of(&serde_json::json!({ "request": "attach" })).is_ok());
        assert!(request_of(&serde_json::json!({ "request": "run" })).is_err());
    }
}
