//! Database module for SwingMusic
//!
//! This module handles all database operations using SQLx with SQLite.

mod engine;
mod migrations;
pub mod tables;
mod userdata;

pub use engine::{setup_sqlite, DbEngine};
pub use migrations::run_migrations;
pub use tables::*;
pub use userdata::{setup_userdata, UserdataEngine};
