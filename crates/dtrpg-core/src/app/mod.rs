//! Application shell modules for the Rust desktop frontend.

mod assets;

use dtrpg_ui::ui::app::{CollectionsServiceFactory, LoginServiceFactory, ServiceFactory, setup};
use gpui::*;

/// Boots the GPUI application with keyring-backed library and collections
/// services.
pub fn run() {
    gpui_platform::application().with_assets(assets::Assets::new("assets/icons"))
                                .with_quit_mode(QuitMode::LastWindowClosed)
                                .run(|cx| {
                                    cx.set_global(ServiceFactory(Box::new(
            |tokens| match tokens {
                Some(t) => Box::new(
                    crate::services::sdk::RustSdkLibraryService::from_keyring_with_tokens(t),
                ),
                None => Box::new(crate::services::sdk::RustSdkLibraryService::unauthenticated()),
            },
        )));
                                    cx.set_global(CollectionsServiceFactory(Box::new(|tokens| {
                                          match tokens {
                Some(t) => Box::new(
                    crate::services::sdk::RustSdkCollectionsService::from_keyring_with_tokens(t),
                ),
                None => {
                    Box::new(crate::services::sdk::RustSdkCollectionsService::unauthenticated())
                }
            }
                                      })));
                                    cx.set_global(LoginServiceFactory(Box::new(
                crate::services::login::build_login_service,
            )));
                                    setup(cx);
                                });
}
