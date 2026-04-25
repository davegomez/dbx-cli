#![cfg_attr(coverage, allow(dead_code, unused_imports))]

//! Core building blocks for `dbx`.
//!
//! This crate keeps Dropbox API knowledge, validation, and HTTP execution out of
//! the CLI presentation layer. The binary can expose CLI, MCP, or other agent
//! surfaces from these same primitives.

pub mod auth;
pub mod client;
pub mod error;
pub mod executor;
pub mod fields;
pub mod operations;
pub mod schema;
pub mod validate;

pub use error::DbxError;
