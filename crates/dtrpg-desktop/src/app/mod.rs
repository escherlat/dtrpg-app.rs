//! Application shell modules for the Rust desktop frontend.

mod assets;

use dtrpg_core::data::token_status::TokenStatusHandle;
use dtrpg_core::services::sdk::{RustSdkCollectionsService, RustSdkLibraryService};
use dtrpg_ui::data::token_status_global::TokenStatusGlobal;
use dtrpg_ui::ui::app::{CollectionsServiceFactory, LoginServiceFactory, ServiceFactory, setup};
use gpui::*;

/// Boots the GPUI application with keyring-backed library and collections
/// services.
pub fn run() {
    gpui_platform::application().with_assets(assets::Assets::new("assets/icons"))
                                .with_quit_mode(QuitMode::LastWindowClosed)
                                .run(|cx| {
        // One slot shared by both services (which publish each new access
        // token's timing into it) and the Settings window (which reads it).
        // It must be registered before `setup` creates the Settings controller.
        let token_status = TokenStatusHandle::default();
        cx.set_global(TokenStatusGlobal(token_status.clone()));

        let library_token_status = token_status.clone();
        cx.set_global(ServiceFactory(Box::new(move |tokens| match tokens {
            Some(t) => Box::new(RustSdkLibraryService::from_keyring_with_tokens(
                t,
                library_token_status.clone(),
            )),
            None => Box::new(RustSdkLibraryService::unauthenticated()),
        })));

        cx.set_global(CollectionsServiceFactory(Box::new(move |tokens| match tokens {
            Some(t) => Box::new(RustSdkCollectionsService::from_keyring_with_tokens(
                t,
                token_status.clone(),
            )),
            None => Box::new(RustSdkCollectionsService::unauthenticated()),
        })));

        cx.set_global(LoginServiceFactory(Box::new(
            dtrpg_core::services::login::build_login_service,
        )));
        setup(cx);
    });
}
