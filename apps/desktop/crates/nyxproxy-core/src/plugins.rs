//! Out-of-process plugin host. Plugins are described by a small JSON manifest
//! placed under `<data_dir>/plugins/<plugin_id>/plugin.json`. Each manifest
//! looks like:
//!
//! ```json
//! {
//!   "id": "wordpress-fingerprint",
//!   "name": "WordPress fingerprint",
//!   "version": "0.1.0",
//!   "description": "Flags responses that look like a WordPress install.",
//!   "command": ["python3", "main.py"],
//!   "capabilities": ["scan_flow"]
//! }
//! ```
//!
//! The host process spawns the plugin on demand, sends a single newline-
//! delimited JSON-RPC 2.0 request on stdin (`{"jsonrpc":"2.0","method":"scan_flow","params":{"flow":..},"id":1}`)
//! and reads exactly one JSON response line back from stdout
//! (`{"jsonrpc":"2.0","result":{"issues":[..]},"id":1}`). The plugin then
//! terminates. This stateless model keeps the contract simple, makes plugin
//! crashes survivable, and means there is no long-lived inter-process state to
//! reason about.
//!
//! See `apps/desktop/plugins/example-wordpress/` for a runnable reference.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::{NyxError, NyxResult};
use crate::model::HttpFlow;
use crate::scanner::Issue;

const PLUGIN_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    pub command: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub manifest: PluginManifest,
    pub manifest_path: PathBuf,
    pub working_dir: PathBuf,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct PluginManager {
    plugins_dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, PluginDescriptor>>>,
}

impl PluginManager {
    pub fn new(plugins_dir: impl AsRef<Path>) -> Self {
        Self {
            plugins_dir: plugins_dir.as_ref().to_path_buf(),
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Re-scan the plugins directory and rebuild the in-memory descriptor map.
    pub fn reload(&self) -> NyxResult<Vec<PluginDescriptor>> {
        let mut next = HashMap::new();
        if !self.plugins_dir.exists() {
            std::fs::create_dir_all(&self.plugins_dir).map_err(NyxError::Io)?;
        }
        let entries = match std::fs::read_dir(&self.plugins_dir) {
            Ok(it) => it,
            Err(err) => return Err(NyxError::Io(err)),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("plugin.json");
            if !manifest_path.exists() {
                continue;
            }
            let bytes = match std::fs::read(&manifest_path) {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(?err, path = %manifest_path.display(), "plugin: skip unreadable manifest");
                    continue;
                }
            };
            let manifest: PluginManifest = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(err) => {
                    tracing::warn!(?err, path = %manifest_path.display(), "plugin: skip invalid manifest");
                    continue;
                }
            };
            if manifest.command.is_empty() {
                tracing::warn!(id = %manifest.id, "plugin: manifest has empty command");
                continue;
            }
            let descriptor = PluginDescriptor {
                manifest_path: manifest_path.clone(),
                working_dir: path.clone(),
                enabled: true,
                manifest,
            };
            next.insert(descriptor.manifest.id.clone(), descriptor);
        }
        let snapshot: Vec<PluginDescriptor> = next.values().cloned().collect();
        *self.inner.write() = next;
        Ok(snapshot)
    }

    pub fn list(&self) -> Vec<PluginDescriptor> {
        self.inner.read().values().cloned().collect()
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut inner = self.inner.write();
        if let Some(p) = inner.get_mut(id) {
            p.enabled = enabled;
            true
        } else {
            false
        }
    }

    fn get_enabled(&self, id: &str) -> Option<PluginDescriptor> {
        let inner = self.inner.read();
        inner
            .get(id)
            .filter(|p| p.enabled)
            .cloned()
    }

    /// Invoke `scan_flow` on a specific plugin. Returns the set of issues the
    /// plugin produced or an error if the plugin process failed.
    pub async fn scan_flow(&self, plugin_id: &str, flow: &HttpFlow) -> NyxResult<Vec<Issue>> {
        let Some(descriptor) = self.get_enabled(plugin_id) else {
            return Err(NyxError::NotFound(format!(
                "plugin '{}' not loaded or disabled",
                plugin_id
            )));
        };
        if !descriptor
            .manifest
            .capabilities
            .iter()
            .any(|c| c == "scan_flow")
        {
            return Err(NyxError::Invalid(format!(
                "plugin '{}' does not advertise scan_flow capability",
                plugin_id
            )));
        }
        let request = json!({
            "jsonrpc": "2.0",
            "method": "scan_flow",
            "params": { "flow": flow },
            "id": 1,
        });
        let response = invoke(&descriptor, &request).await?;
        let issues = response
            .get("result")
            .and_then(|r| r.get("issues"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        match serde_json::from_value::<Vec<Issue>>(issues) {
            Ok(v) => Ok(v),
            Err(err) => Err(NyxError::Invalid(format!(
                "plugin '{}' returned invalid issues: {err}",
                plugin_id
            ))),
        }
    }

    /// Invoke every enabled plugin that advertises `scan_flow` against `flow`
    /// and concatenate their issue sets. Errors from individual plugins are
    /// logged but do not abort the scan.
    pub async fn scan_flow_all(&self, flow: &HttpFlow) -> Vec<Issue> {
        let plugins: Vec<PluginDescriptor> = self
            .inner
            .read()
            .values()
            .filter(|p| {
                p.enabled
                    && p.manifest.capabilities.iter().any(|c| c == "scan_flow")
            })
            .cloned()
            .collect();
        let mut issues = Vec::new();
        for plugin in plugins {
            match self.scan_flow(&plugin.manifest.id, flow).await {
                Ok(mut v) => issues.append(&mut v),
                Err(err) => tracing::warn!(plugin = %plugin.manifest.id, ?err, "plugin scan failed"),
            }
        }
        issues
    }
}

async fn invoke(descriptor: &PluginDescriptor, request: &Value) -> NyxResult<Value> {
    let mut iter = descriptor.manifest.command.iter();
    let program = iter
        .next()
        .ok_or_else(|| NyxError::Invalid("plugin command is empty".into()))?;
    let mut cmd = Command::new(program);
    for arg in iter {
        cmd.arg(arg);
    }
    cmd.current_dir(&descriptor.working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NYXPROXY_PLUGIN_ID", &descriptor.manifest.id);
    let mut child = cmd
        .spawn()
        .map_err(|err| NyxError::Internal(format!("plugin spawn failed: {err}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| NyxError::Internal("plugin stdin missing".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| NyxError::Internal("plugin stdout missing".into()))?;
    let line = format!("{}\n", request);
    timeout(PLUGIN_TIMEOUT, async {
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin);
        let mut reader = BufReader::new(stdout);
        let mut buf = String::new();
        reader.read_line(&mut buf).await?;
        let _ = child.wait().await;
        Ok::<String, std::io::Error>(buf)
    })
    .await
    .map_err(|_| NyxError::Internal("plugin invocation timed out".into()))?
    .map_err(|err| NyxError::Internal(format!("plugin io error: {err}")))
    .and_then(|line| {
        serde_json::from_str(line.trim())
            .map_err(|err| NyxError::Invalid(format!("plugin returned invalid json: {err}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CapturedRequest;
    use base64::Engine;

    fn write_manifest(dir: &Path, manifest: &PluginManifest) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("plugin.json");
        let json = serde_json::to_vec_pretty(manifest).unwrap();
        std::fs::write(path, json).unwrap();
    }

    fn write_plugin_script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("plugin.sh");
        std::fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    fn sample_flow() -> HttpFlow {
        let req = CapturedRequest {
            method: "GET".into(),
            url: "https://example.com/wp-login.php".into(),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: "/wp-login.php".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![],
            body_b64: base64::engine::general_purpose::STANDARD.encode(b""),
            body_size: 0,
        };
        HttpFlow::new(req)
    }

    #[test]
    fn reload_picks_up_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("hello-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            &PluginManifest {
                id: "hello".into(),
                name: "Hello plugin".into(),
                version: "0.0.1".into(),
                description: "".into(),
                author: None,
                command: vec!["true".into()],
                capabilities: vec!["scan_flow".into()],
            },
        );
        let mgr = PluginManager::new(dir.path());
        let plugins = mgr.reload().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest.id, "hello");
    }

    #[tokio::test]
    async fn scan_flow_invokes_plugin_process() {
        if cfg!(windows) {
            eprintln!("skipping: shell-script plugin test only runs on unix");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("script-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let script = write_plugin_script(
            &plugin_dir,
            r#"#!/usr/bin/env bash
read line
echo '{"jsonrpc":"2.0","id":1,"result":{"issues":[{"id":"plug-1","flow_id":"f","rule_id":"plug","name":"Plugin issue","severity":"medium","confidence":"firm","description":"d","evidence":null,"remediation":null,"host":"example.com","path":"/"}]}}'
"#,
        );
        write_manifest(
            &plugin_dir,
            &PluginManifest {
                id: "script".into(),
                name: "Script plugin".into(),
                version: "0.0.1".into(),
                description: "".into(),
                author: None,
                command: vec![script.to_string_lossy().to_string()],
                capabilities: vec!["scan_flow".into()],
            },
        );
        let mgr = PluginManager::new(dir.path());
        mgr.reload().unwrap();
        let issues = mgr.scan_flow("script", &sample_flow()).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].rule_id, "plug");
    }

    #[tokio::test]
    async fn missing_plugin_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = PluginManager::new(dir.path());
        let result = mgr.scan_flow("nope", &sample_flow()).await;
        assert!(result.is_err());
    }
}
