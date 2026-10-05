//! Optional local copy of the Hydrus Public Tag Repository (PTR).
//!
//! See `PTR_PLAN.md`. This module holds the protocol client and the update
//! parser. The spike test measures the real PTR sizes.

#![allow(dead_code)] // Used by later phases.

pub mod client;
pub mod db;
pub mod lookup;
pub mod place;
pub mod sync;
pub mod update;

#[cfg(test)]
mod spike;
