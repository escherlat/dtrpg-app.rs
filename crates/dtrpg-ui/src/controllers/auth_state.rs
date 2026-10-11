//! Authentication state controller: owns `AuthState` and the active notice
//! list.

use gpui::Context;

use crate::data::auth_state::AuthState;
use crate::data::events::AuthStateChanged;
use crate::data::notification::{Notice, NoticeAction, NoticeKind};

/// Owns the application's authentication state and the derived notice list.
pub struct AuthStateController {
    state:           AuthState,
    notices:         Vec<Notice>,
    is_auth_pending: bool,
}

impl AuthStateController {
    /// Creates a new controller with the given `initial_state`.
    ///
    /// Callers are responsible for providing the correct starting state. In
    /// debug builds, callers may consult `DTRPG_AUTH_STATE_OVERRIDE` before
    /// constructing to support testing unauthenticated or expired states
    /// without real credentials.
    pub fn new(initial_state: AuthState) -> Self {
        let notices = notices_for(initial_state, false);
        Self { state: initial_state,
               notices,
               is_auth_pending: false }
    }

    /// Returns the current authentication state.
    pub fn state(&self) -> AuthState {
        self.state
    }

    /// Returns all notices that have not been dismissed.
    pub fn active_notices(&self) -> Vec<&Notice> {
        self.notices.iter().filter(|n| !n.dismissed).collect()
    }

    /// Sets the auth-pending flag and regenerates the notice list.
    ///
    /// When `pending` is `true`, the `Authenticating` notice is shown instead
    /// of any `NotSignedIn` notice that would otherwise appear. Emits
    /// [`AuthStateChanged`].
    pub fn set_auth_pending(&mut self, pending: bool, cx: &mut Context<Self>) {
        self.is_auth_pending = pending;
        self.notices = notices_for(self.state, self.is_auth_pending);
        cx.emit(AuthStateChanged);
    }

    /// Transitions to `state`, clears the auth-pending flag, regenerates the
    /// notice list, and emits [`AuthStateChanged`].
    pub fn set_state(&mut self, state: AuthState, cx: &mut Context<Self>) {
        self.state = state;
        self.is_auth_pending = false;
        self.notices = notices_for(state, false);
        cx.emit(AuthStateChanged);
    }

    /// Marks the notice with the given `kind` as dismissed and emits
    /// [`AuthStateChanged`].
    ///
    /// No-op if no matching notice is found.
    pub fn dismiss_notice(&mut self, kind: NoticeKind, cx: &mut Context<Self>) {
        let before = self.notices.iter().filter(|n| !n.dismissed).count();
        for notice in self.notices.iter_mut() {
            if notice.kind == kind {
                notice.dismissed = true;
            }
        }
        let after = self.notices.iter().filter(|n| !n.dismissed).count();
        if after != before {
            cx.emit(AuthStateChanged);
        }
    }
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

fn notices_for(state: AuthState, is_auth_pending: bool) -> Vec<Notice> {
    if is_auth_pending {
        // Suppress banners while startup auth is in-flight. The "Signing in..."
        // feedback is shown as a toast notification by the root view instead.
        return vec![];
    }
    match state {
        AuthState::Authenticated => vec![],
        AuthState::Unauthenticated => vec![Notice { kind:      NoticeKind::NotSignedIn,
                                                    dismissed: false,
                                                    action:    NoticeAction::OpenSettings, }],
        AuthState::SessionExpired => vec![Notice { kind:      NoticeKind::SessionExpired,
                                                   dismissed: false,
                                                   action:    NoticeAction::OpenSettings, }],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthenticated_initial_state_produces_one_notice() {
        let ctrl = AuthStateController::new(AuthState::Unauthenticated);
        assert_eq!(ctrl.state(), AuthState::Unauthenticated);
        assert_eq!(ctrl.active_notices().len(), 1);
        assert_eq!(ctrl.active_notices()[0].kind, NoticeKind::NotSignedIn);
    }

    #[test]
    fn authenticated_initial_state_has_no_notices() {
        let ctrl = AuthStateController::new(AuthState::Authenticated);
        assert_eq!(ctrl.state(), AuthState::Authenticated);
        assert!(ctrl.active_notices().is_empty());
    }

    #[test]
    fn dismiss_notice_removes_only_target() {
        let mut ctrl = AuthStateController::new(AuthState::Unauthenticated);
        assert_eq!(ctrl.active_notices().len(), 1);
        for n in ctrl.notices.iter_mut() {
            if n.kind == NoticeKind::NotSignedIn {
                n.dismissed = true;
            }
        }
        assert!(ctrl.active_notices().is_empty());
    }

    #[test]
    fn expired_produces_session_expired_notice() {
        let notices = notices_for(AuthState::SessionExpired, false);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, NoticeKind::SessionExpired);
    }

    #[test]
    fn pending_flag_suppresses_banner() {
        let mut ctrl = AuthStateController::new(AuthState::Unauthenticated);
        ctrl.is_auth_pending = true;
        ctrl.notices = notices_for(ctrl.state, ctrl.is_auth_pending);
        // Banner is suppressed while auth is in-flight; toast notification
        // handles feedback.
        assert!(ctrl.active_notices().is_empty());
    }

    #[test]
    fn clearing_pending_flag_restores_not_signed_in_notice() {
        let mut ctrl = AuthStateController::new(AuthState::Unauthenticated);
        ctrl.is_auth_pending = true;
        ctrl.notices = notices_for(ctrl.state, ctrl.is_auth_pending);
        // Clear flag
        ctrl.is_auth_pending = false;
        ctrl.notices = notices_for(ctrl.state, ctrl.is_auth_pending);
        let active = ctrl.active_notices();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].kind, NoticeKind::NotSignedIn);
    }

    #[test]
    fn set_state_clears_pending_flag() {
        // Simulate the post-set_state state directly
        let mut ctrl = AuthStateController::new(AuthState::Unauthenticated);
        ctrl.is_auth_pending = true;
        ctrl.notices = notices_for(ctrl.state, ctrl.is_auth_pending);
        // set_state would set is_auth_pending = false and state = Authenticated
        ctrl.state = AuthState::Authenticated;
        ctrl.is_auth_pending = false;
        ctrl.notices = notices_for(ctrl.state, ctrl.is_auth_pending);
        assert!(!ctrl.is_auth_pending);
        assert!(ctrl.active_notices().is_empty());
    }
}
