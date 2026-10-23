//! `alhalte-cafe` — one-shot local Alhalte launcher.
//!
//! Reads `Restaurant.toml`, materializes secrets, starts `alhalte-cellar` and
//! `alhalte-maitre-d` (which spawns `alhalte-banquette` per room), and opens the
//! bundled SPA. No systemd is involved.
//!
//! See `crates/alhalte-cafe/ARCHITECTURE.md`.

#![deny(unsafe_code)]

pub mod error;
pub mod lan;
pub mod serve;

pub use error::{CafeError, Result};
pub use lan::{detect_lan_ipv4_addresses, detect_lan_urls, lan_url};
pub use serve::{CafeOptions, CafePlan, build_cafe_plan, prepare_cafe, run_cafe};
