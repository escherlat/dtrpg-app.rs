//! Headless application services shared by Libri entry points.
//!
//! The desktop binary still owns the current GPUI bootstrap. This library
//! target is the incremental seam required by ADR 0002: headless consumers can
//! depend on platform capabilities without linking the UI binary modules.

pub mod platform;
