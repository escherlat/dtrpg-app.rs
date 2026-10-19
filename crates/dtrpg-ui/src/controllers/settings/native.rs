//! The Native Integration switches of the settings controller.
//!
//! They are read from disk by the work that uses them, each time it runs, so a
//! change takes effect without an event.

use super::*;

impl SettingsController {
    /// Saves whether the library's tags are written to files' tags.
    pub fn set_write_tags_to_files(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.native.save_write_tags_to_files(enabled);
        cx.notify();
    }

    /// Saves whether tags changed on files outside Libri are read back into the
    /// library.
    pub fn set_read_tags_from_files(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.native.save_read_tags_from_files(enabled);
        cx.notify();
    }

    /// Saves whether files' tags are compared with the library's at launch.
    pub fn set_reconcile_on_launch(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.native.save_reconcile_on_launch(enabled);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::controllers::settings::tests::UnusedLoginService;

    #[gpui::test]
    fn each_switch_is_saved_on_its_own(cx: &mut TestAppContext) {
        let ctrl = cx.new(|_| SettingsController::for_test(Arc::new(UnusedLoginService)));

        ctrl.update(cx, |ctrl, cx| ctrl.set_read_tags_from_files(false, cx));
        let after_read = ctrl.read_with(cx, |ctrl, _| ctrl.snapshot().native);
        ctrl.update(cx, |ctrl, cx| ctrl.set_write_tags_to_files(false, cx));
        ctrl.update(cx, |ctrl, cx| ctrl.set_reconcile_on_launch(false, cx));
        let after_all = ctrl.read_with(cx, |ctrl, _| ctrl.snapshot().native);

        assert!(after_read.write_tags_to_files
                && !after_read.read_tags_from_files
                && after_read.reconcile_on_launch);
        assert!(!after_all.write_tags_to_files
                && !after_all.read_tags_from_files
                && !after_all.reconcile_on_launch);
    }

    #[gpui::test]
    fn the_switches_are_on_by_default(cx: &mut TestAppContext) {
        let ctrl = cx.new(|_| SettingsController::for_test(Arc::new(UnusedLoginService)));

        let native = ctrl.read_with(cx, |ctrl, _| ctrl.snapshot().native);

        assert!(native.write_tags_to_files
                && native.read_tags_from_files
                && native.reconcile_on_launch);
    }
}
