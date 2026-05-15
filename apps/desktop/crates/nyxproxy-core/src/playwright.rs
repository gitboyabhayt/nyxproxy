//! Playwright-recorded browser macros.
//!
//! Burp Pro lets you record a login flow in its embedded browser and replay
//! it. NyxProxy doesn't ship an embedded browser, but Microsoft's
//! [`playwright codegen`](https://playwright.dev/docs/codegen) command does
//! exactly this and emits a TypeScript spec file describing every action.
//!
//! This module parses that spec file into a structured DSL
//! ([`PlaywrightRecording`]) that NyxProxy stores next to the user's macros.
//! When the user clicks "Play" we shell out to `npx playwright test` with the
//! recording's location, the browser's HTTP/HTTPS proxy pointed at NyxProxy's
//! own listener and `ignoreHTTPSErrors: true` so the on-the-fly CA is trusted.
//! Every captured request lands in `HistoryStore` just like a manual browser
//! session.
//!
//! Parsing is pure-function and exhaustively unit-tested. The actual
//! `npx playwright …` spawn is a thin wrapper around [`std::process::Command`]
//! that we exercise as an integration test on hosts where Playwright is
//! installed; if it isn't, the Tauri command returns a structured
//! "playwright_not_installed" error instead of panicking.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{NyxError, NyxResult};

/// One discrete browser action inside a recording.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaywrightAction {
    /// `await page.goto(<url>);`
    Navigate { url: String },
    /// `await page.locator(<selector>).click();` (or convenience helpers
    /// like `getByRole`, `getByText`, `getByLabel` which we normalise into
    /// a selector string).
    Click { selector: String },
    /// `await page.locator(<selector>).fill(<value>);`
    Fill { selector: String, value: String },
    /// `await page.locator(<selector>).press(<key>);`
    Press { selector: String, key: String },
    /// `await page.waitForURL(<url-or-regex>);`
    WaitForUrl { url: String },
    /// `await expect(page).toHaveURL(<url>);`
    ExpectUrl { url: String },
    /// Anything we didn't recognise. We keep the raw line so the user can
    /// edit it by hand or report a parser miss.
    Raw { line: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaywrightRecording {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub actions: Vec<PlaywrightAction>,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl PlaywrightRecording {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            actions: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Parse the output of `npx playwright codegen` into a [`PlaywrightRecording`].
///
/// The codegen tool emits a TypeScript file like:
///
/// ```text
/// import { test, expect } from '@playwright/test';
///
/// test('test', async ({ page }) => {
///   await page.goto('https://example.com/login');
///   await page.getByRole('textbox', { name: 'Email' }).fill('user@example.com');
///   await page.getByRole('textbox', { name: 'Password' }).fill('hunter2');
///   await page.getByRole('button', { name: 'Sign in' }).click();
///   await expect(page).toHaveURL('https://example.com/dashboard');
/// });
/// ```
///
/// We parse it line-by-line. Unrecognised lines turn into
/// [`PlaywrightAction::Raw`] so nothing is silently dropped.
pub fn parse_codegen_spec(spec: &str) -> NyxResult<PlaywrightRecording> {
    let mut recording = PlaywrightRecording::new("Recorded macro");
    for raw_line in spec.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with("import")
            || line.starts_with("test(")
            || line.starts_with("test.")
            || line.starts_with('}')
            || line.starts_with('{')
            || line == "});"
        {
            continue;
        }
        let stripped = line
            .trim_start_matches("await ")
            .trim_end_matches(';')
            .trim();

        if let Some(rest) = stripped.strip_prefix("page.goto(") {
            let url = unwrap_string_arg(rest)?;
            recording.actions.push(PlaywrightAction::Navigate { url });
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("page.waitForURL(") {
            let url = unwrap_string_arg(rest)?;
            recording.actions.push(PlaywrightAction::WaitForUrl { url });
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("expect(page).toHaveURL(") {
            let url = unwrap_string_arg(rest)?;
            recording.actions.push(PlaywrightAction::ExpectUrl { url });
            continue;
        }
        if stripped.starts_with("page.") {
            if let Some(action) = parse_locator_call(stripped) {
                recording.actions.push(action);
                continue;
            }
        }
        recording.actions.push(PlaywrightAction::Raw {
            line: line.to_string(),
        });
    }
    Ok(recording)
}

/// Take a chain like `page.getByRole('button', { name: 'Sign in' }).click()`
/// or `page.locator('#submit').fill('hello')` and produce a normalised
/// selector + the trailing call.
fn parse_locator_call(line: &str) -> Option<PlaywrightAction> {
    // Identify the final method call after the last `).`.
    let (selector_part, action_part) = split_locator_action(line)?;
    let selector = normalise_selector(selector_part)?;

    let action_name = action_part
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let args_inside = inside_parens(action_part)?;
    match action_name.as_str() {
        "click" => Some(PlaywrightAction::Click { selector }),
        "fill" => {
            let value = unwrap_string_arg(args_inside.as_str()).ok()?;
            Some(PlaywrightAction::Fill { selector, value })
        }
        "press" => {
            let key = unwrap_string_arg(args_inside.as_str()).ok()?;
            Some(PlaywrightAction::Press { selector, key })
        }
        _ => None,
    }
}

/// Split `page.locator('x').click()` into `page.locator('x')` and `click()`.
fn split_locator_action(line: &str) -> Option<(&str, &str)> {
    // Find the last `).` that splits the selector chain from the action call.
    let mut depth = 0i32;
    let bytes = line.as_bytes();
    let mut last = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b'.' {
                last = Some(i + 1);
            }
        } else if c == b'\'' || c == b'"' || c == b'`' {
            // skip string literal
            let quote = c;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
        }
        i += 1;
    }
    let split = last?;
    Some((&line[..split], &line[split + 1..]))
}

/// Turn the selector chain back into a single string the user can paste back
/// into Playwright if they edit the macro by hand. We preserve the original
/// source verbatim — the proxy doesn't have a DOM, so we don't need to make
/// CSS selectors out of `getByRole`.
fn normalise_selector(chain: &str) -> Option<String> {
    let trimmed = chain.trim();
    let body = trimmed.strip_prefix("page.")?;
    Some(body.to_string())
}

fn inside_parens(call: &str) -> Option<String> {
    let start = call.find('(')?;
    // Match the closing paren accounting for nested calls/strings.
    let bytes = call.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(call[start + 1..i].to_string());
                }
            }
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn unwrap_string_arg(args: &str) -> NyxResult<String> {
    let s = args.trim();
    // Strip a single trailing `)` if `unwrap_string_arg` is called with the
    // full call (e.g. `page.goto('https://x')` minus the prefix). We also
    // handle the inside-parens case which has no trailing paren.
    let s = s.strip_suffix(')').unwrap_or(s);
    let s = s.trim();
    if s.is_empty() {
        return Err(NyxError::Invalid("empty string argument".into()));
    }
    // For multi-arg calls like `getByRole('textbox', { name: 'Email' })`,
    // we want just the first argument.
    let first = split_first_arg(s);
    let inner = first.trim();
    let unquoted = if (inner.starts_with('\'') && inner.ends_with('\''))
        || (inner.starts_with('"') && inner.ends_with('"'))
        || (inner.starts_with('`') && inner.ends_with('`'))
    {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };
    Ok(unquoted.to_string())
}

fn split_first_arg(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b',' if depth == 0 => return &s[..i],
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    s
}

/// On-disk store. Recordings live as one JSON blob per recording inside
/// `dir`; we keep them separate from the existing JSON-based macro store to
/// avoid muddying that file format.
#[derive(Clone)]
pub struct PlaywrightStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, PlaywrightRecording>>>,
}

impl PlaywrightStore {
    pub fn open(dir: impl AsRef<Path>) -> NyxResult<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(NyxError::Io)?;
        let mut map = HashMap::new();
        if dir.is_dir() {
            for entry in std::fs::read_dir(&dir).map_err(NyxError::Io)? {
                let entry = entry.map_err(NyxError::Io)?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let bytes = std::fs::read(&path).map_err(NyxError::Io)?;
                let recording: PlaywrightRecording = serde_json::from_slice(&bytes)
                    .map_err(|e| NyxError::Decode(format!("playwright recording: {e}")))?;
                map.insert(recording.id.clone(), recording);
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(map)),
        })
    }

    pub fn list(&self) -> Vec<PlaywrightRecording> {
        let mut v: Vec<PlaywrightRecording> = self.inner.read().values().cloned().collect();
        v.sort_by(|a, b| a.updated_at.cmp(&b.updated_at).reverse());
        v
    }

    pub fn get(&self, id: &str) -> Option<PlaywrightRecording> {
        self.inner.read().get(id).cloned()
    }

    pub fn save(&self, mut recording: PlaywrightRecording) -> NyxResult<PlaywrightRecording> {
        recording.updated_at = Utc::now();
        let path = self.dir.join(format!("{}.json", recording.id));
        let serialized = serde_json::to_vec_pretty(&recording)
            .map_err(|e| NyxError::Decode(e.to_string()))?;
        std::fs::write(&path, serialized).map_err(NyxError::Io)?;
        self.inner.write().insert(recording.id.clone(), recording.clone());
        Ok(recording)
    }

    pub fn delete(&self, id: &str) -> NyxResult<bool> {
        let removed = self.inner.write().remove(id).is_some();
        if removed {
            let path = self.dir.join(format!("{id}.json"));
            if path.exists() {
                std::fs::remove_file(&path).ok();
            }
        }
        Ok(removed)
    }
}

/// Result of attempting to launch `npx playwright codegen`. We expose the
/// child PID so the Tauri layer can offer a "Stop recording" button.
#[derive(Debug, Serialize)]
pub struct CodegenLaunch {
    pub pid: u32,
}

/// Result of attempting to detect whether Playwright is installed on this
/// machine. Returned by [`detect_playwright`] so the UI can show actionable
/// guidance instead of a generic spawn error.
#[derive(Debug, Clone, Serialize)]
pub struct PlaywrightAvailability {
    pub available: bool,
    pub version: Option<String>,
    pub install_hint: String,
}

pub fn detect_playwright() -> PlaywrightAvailability {
    let install_hint = "Run `npm install -D @playwright/test && npx playwright install` \
        in any working directory, then refresh this page."
        .to_string();
    let output = std::process::Command::new("npx")
        .arg("--no")
        .arg("playwright")
        .arg("--version")
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            PlaywrightAvailability {
                available: true,
                version: Some(raw),
                install_hint,
            }
        }
        _ => PlaywrightAvailability {
            available: false,
            version: None,
            install_hint,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SPEC: &str = r#"
import { test, expect } from '@playwright/test';

test('login', async ({ page }) => {
  await page.goto('https://example.com/login');
  await page.getByRole('textbox', { name: 'Email' }).fill('user@example.com');
  await page.getByRole('textbox', { name: 'Password' }).fill('hunter2');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.locator('#submit').press('Enter');
  await page.waitForURL('https://example.com/dashboard');
  await expect(page).toHaveURL('https://example.com/dashboard');
});
"#;

    #[test]
    fn parses_navigate() {
        let r = parse_codegen_spec("await page.goto('https://x.test');").unwrap();
        assert_eq!(r.actions.len(), 1);
        match &r.actions[0] {
            PlaywrightAction::Navigate { url } => assert_eq!(url, "https://x.test"),
            other => panic!("expected navigate, got {other:?}"),
        }
    }

    #[test]
    fn parses_fill_with_get_by_role() {
        let r = parse_codegen_spec(
            "await page.getByRole('textbox', { name: 'Email' }).fill('a@b.c');",
        )
        .unwrap();
        assert_eq!(r.actions.len(), 1);
        match &r.actions[0] {
            PlaywrightAction::Fill { selector, value } => {
                assert!(selector.contains("getByRole"));
                assert_eq!(value, "a@b.c");
            }
            other => panic!("expected fill, got {other:?}"),
        }
    }

    #[test]
    fn parses_click_with_locator() {
        let r = parse_codegen_spec("await page.locator('#submit').click();").unwrap();
        assert_eq!(r.actions.len(), 1);
        match &r.actions[0] {
            PlaywrightAction::Click { selector } => assert_eq!(selector, "locator('#submit')"),
            other => panic!("expected click, got {other:?}"),
        }
    }

    #[test]
    fn parses_full_login_spec() {
        let r = parse_codegen_spec(SAMPLE_SPEC).unwrap();
        // navigate + 2 fill + 2 click+press + wait + expect = 7
        assert_eq!(r.actions.len(), 7);
        assert!(matches!(r.actions[0], PlaywrightAction::Navigate { .. }));
        assert!(matches!(r.actions[1], PlaywrightAction::Fill { .. }));
        assert!(matches!(r.actions[2], PlaywrightAction::Fill { .. }));
        assert!(matches!(r.actions[3], PlaywrightAction::Click { .. }));
        assert!(matches!(r.actions[4], PlaywrightAction::Press { .. }));
        assert!(matches!(r.actions[5], PlaywrightAction::WaitForUrl { .. }));
        assert!(matches!(r.actions[6], PlaywrightAction::ExpectUrl { .. }));
    }

    #[test]
    fn unknown_lines_become_raw() {
        let spec = r#"
await page.someUnknownAction('thing');
"#;
        let r = parse_codegen_spec(spec).unwrap();
        assert_eq!(r.actions.len(), 1);
        assert!(matches!(r.actions[0], PlaywrightAction::Raw { .. }));
    }

    #[test]
    fn store_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaywrightStore::open(tmp.path()).unwrap();
        let mut rec = PlaywrightRecording::new("login");
        rec.actions
            .push(PlaywrightAction::Navigate { url: "https://x".into() });
        let saved = store.save(rec.clone()).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get(&saved.id).unwrap().name, "login");
        assert!(store.delete(&saved.id).unwrap());
        assert!(store.list().is_empty());
    }

    #[test]
    fn detect_playwright_returns_a_result_always() {
        // We don't assert on `available` because CI machines vary; we just
        // make sure the call doesn't panic and the struct is well-formed.
        let avail = detect_playwright();
        if !avail.available {
            assert!(avail.version.is_none());
        }
    }
}
