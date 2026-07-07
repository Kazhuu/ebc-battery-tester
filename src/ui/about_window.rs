pub(crate) struct AboutWindow {
    pub(crate) open: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) update_check_state: crate::update_check::UpdateCheckState,
    #[cfg(not(target_arch = "wasm32"))]
    update_check_rx:
        futures::channel::mpsc::UnboundedReceiver<crate::update_check::UpdateCheckState>,
}

impl Default for AboutWindow {
    fn default() -> Self {
        Self {
            open: false,
            #[cfg(not(target_arch = "wasm32"))]
            update_check_state: crate::update_check::UpdateCheckState::Checking,
            #[cfg(not(target_arch = "wasm32"))]
            update_check_rx: futures::channel::mpsc::unbounded().1,
        }
    }
}

impl AboutWindow {
    pub(crate) fn new(_ctx: &egui::Context) -> Self {
        #[cfg_attr(target_arch = "wasm32", expect(unused_mut))]
        let mut window = Self::default();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (update_tx, update_rx) = futures::channel::mpsc::unbounded();
            crate::update_check::spawn_update_check(_ctx.clone(), update_tx);
            window.update_check_rx = update_rx;
        }
        window
    }

    /// Drains newly arrived update-check results. Called once per frame.
    pub(crate) fn poll(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        while let Ok(state) = self.update_check_rx.try_recv() {
            self.update_check_state = state;
        }
    }

    pub(crate) fn ui(&mut self, ui: &egui::Ui) {
        if !self.open {
            return;
        }
        egui::Window::new("About")
            .open(&mut self.open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.heading("EBC Battery Tester");
                ui.horizontal(|ui| {
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        match &self.update_check_state {
                            crate::update_check::UpdateCheckState::Checking => {
                                ui.spinner();
                                ui.weak("checking...");
                            }
                            crate::update_check::UpdateCheckState::UpToDate => {
                                ui.label("(up to date)");
                            }
                            crate::update_check::UpdateCheckState::UpdateAvailable(tag) => {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    format!("({tag} available)"),
                                );
                                ui.hyperlink_to("Download", crate::update_check::RELEASES_PAGE_URL);
                            }
                            crate::update_check::UpdateCheckState::Failed => {}
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    "Open source, cross-platform desktop and browser application to \
                     control the ZKetech EBC-A20 battery tester.",
                );
                ui.add_space(4.0);
                ui.hyperlink_to(
                    "github.com/Kazhuu/ebc-battery-tester",
                    "https://github.com/Kazhuu/ebc-battery-tester",
                );
                ui.add_space(4.0);
            });
    }
}
