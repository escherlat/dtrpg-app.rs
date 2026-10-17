//! GPUI global wrapper for
//! [`dtrpg_core::data::token_status::TokenStatusHandle`].
//!
//! `TokenStatusHandle` lives in `dtrpg-core`, which has no GPUI dependency, so
//! it cannot implement `gpui::Global` itself (neither the trait nor the type
//! is local to this crate -- see ADR 0002). This newtype is the one place
//! that bridges it into GPUI's global registry for the two call sites that
//! need it: `ui::app::run`'s bootstrap (`cx.set_global`) and
//! `controllers::settings`'s read (`cx.try_global`).

use dtrpg_core::data::token_status::TokenStatusHandle;
use gpui::Global;

/// Wraps a [`TokenStatusHandle`] so it can be registered as a GPUI global.
#[derive(Clone, Default)]
pub struct TokenStatusGlobal(pub TokenStatusHandle);

impl Global for TokenStatusGlobal {}
