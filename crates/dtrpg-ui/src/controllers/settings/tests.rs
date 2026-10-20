//! Tests for the settings controller: focus handling, API key masking (see
//! `auth.rs`), and snapshot/logout behavior.

use dtrpg_core::services::{LoginError, LoginTokens};
use gpui::{AppContext, TestAppContext};

use super::*;

/// Never invoked by the tests below — `request_email_focus_on_account_tab`
/// and `take_focus_email_pending` don't touch the login flow.
pub(super) struct UnusedLoginService;

impl LoginService for UnusedLoginService {
    fn login_with_credentials(&self, _email: &str, _password: &str) -> Result<String, LoginError> {
        unimplemented!("not exercised by this test")
    }

    fn authenticate(&self, _api_key: &str) -> Result<LoginTokens, LoginError> {
        unimplemented!("not exercised by this test")
    }
}

#[gpui::test]
fn request_email_focus_sets_account_tab_and_pending_flag(cx: &mut TestAppContext) {
    let ctrl = cx.new(|_| SettingsController::for_test(Arc::new(UnusedLoginService)));

    ctrl.update(cx, |ctrl, cx| {
            ctrl.set_active_page_ix(4, cx);
            ctrl.request_email_focus_on_account_tab(cx);
        });

    ctrl.read_with(cx, |ctrl, _| {
            assert_eq!(ctrl.active_page_ix, 0);
            assert!(ctrl.focus_email_pending);
        });
}

#[gpui::test]
fn take_focus_email_pending_clears_flag_after_reading_it(cx: &mut TestAppContext) {
    let ctrl = cx.new(|_| SettingsController::for_test(Arc::new(UnusedLoginService)));

    ctrl.update(cx, |ctrl, cx| ctrl.request_email_focus_on_account_tab(cx));

    let first_read = ctrl.update(cx, |ctrl, _| ctrl.take_focus_email_pending());
    assert!(first_read);

    let second_read = ctrl.update(cx, |ctrl, _| ctrl.take_focus_email_pending());
    assert!(!second_read);
}

fn logged_in_controller(status: TokenStatus) -> SettingsController {
    let mut ctrl = SettingsController::for_test(Arc::new(UnusedLoginService));
    ctrl.auth_state = AuthState::LoggedIn { email:        Some("a@b.c".to_string()),
                                            avatar_bytes: None, };
    ctrl.token_status.set(status);
    ctrl
}

#[test]
fn snapshot_reports_token_status_while_logged_in() {
    let status = TokenStatus { acquired_at: 1_000,
                               expires_at:  Some(2_000), };
    let ctrl = logged_in_controller(status);

    assert_eq!(ctrl.snapshot().auth.token_status, Some(status));
}

#[test]
fn snapshot_hides_token_status_while_logged_out() {
    let ctrl = SettingsController::for_test(Arc::new(UnusedLoginService));
    ctrl.token_status.set(TokenStatus { acquired_at: 1_000,
                                        expires_at:  None, });

    assert_eq!(ctrl.snapshot().auth.token_status, None);
}

#[gpui::test]
fn logout_clears_the_token_status(cx: &mut TestAppContext) {
    let ctrl = cx.new(|_| {
                     logged_in_controller(TokenStatus { acquired_at: 1_000,
                                                        expires_at:  Some(2_000), })
                 });

    ctrl.update(cx, |ctrl, cx| ctrl.logout(cx));

    ctrl.read_with(cx, |ctrl, _| assert_eq!(ctrl.token_status.get(), None));
}
