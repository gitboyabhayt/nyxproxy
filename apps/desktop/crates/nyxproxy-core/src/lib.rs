//! NyxProxy core engine.
//!
//! This crate provides the building blocks for the NyxProxy desktop app:
//!
//! - [`ca`] — root CA generation and on-the-fly leaf certificate minting.
//! - [`proxy`] — an intercepting HTTPS proxy built on `hyper` + `rustls`.
//! - [`history`] — an in-memory store of captured traffic with eviction.
//! - [`decoder`] — Burp-style decoder utilities (base64, url, hex, html, gzip).
//! - [`sequencer`] — Shannon entropy + byte-frequency analysis for tokens.
//! - [`intruder`] — Sniper / battering-ram / pitchfork / cluster-bomb attacks.
//! - [`scanner`] — passive rule-based scanner producing security issues.
//! - [`spider`] — scope-aware crawler that discovers in-scope URLs.
//! - [`report`] — HTML + JSON export of captured history and issues.
//!
//! The crate has **no dependency on Tauri** so the engine can be unit-tested
//! in isolation, embedded in headless tools, or wrapped by an alternative UI.

pub mod bridge;
pub mod burp_import;
pub mod ca;
pub mod compliance;
pub mod decoder;
pub mod error;
pub mod graphql;
pub mod history;
pub mod intercept;
pub mod intruder;
pub mod jwt;
pub mod macros;
pub mod model;
pub mod monitor;
pub mod nyxshare;
pub mod openapi;
pub mod owasp;
pub mod owasp_dashboard;
pub mod pcap;
pub mod playwright;
pub mod plugins;
pub mod proxy;
pub mod repeater;
pub mod report;
pub mod risk;
pub mod scanner;
pub mod selfhost;
pub mod sequencer;
pub mod spider;
pub mod websocket;
pub mod workspace;

pub use error::{NyxError, NyxResult};
pub use history::{HistoryEntry, HistoryStore};
pub use model::{CapturedRequest, CapturedResponse, HttpFlow, ProxyEvent};
pub use scanner::{Issue, IssueConfidence, IssueSeverity};
