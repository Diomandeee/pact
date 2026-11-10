//! PHASE-ZERO-LOCKED
//! This file is on the map and is not open yet.
//! Opens after: P8.1
//! Why that order: Content-keyed cache, mirroring the readings table exactly. Local first, so the reader works before Convex exists and offline afterwards.
//!
//! Do not implement here. Building on an unproven layer means every
//! result above it inherits an assumption nobody checked.
//! When the gates pass:  ./scripts/pz open crates/pact-reader/src/cache.rs
