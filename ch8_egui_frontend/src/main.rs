//Disable terminal window opening on windows machines when built in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

use eframe::egui;

fn main() {
	env_logger::init();

	let options = eframe::NativeOptions {
		resizable: true,
		min_window_size: Some(egui::Vec2::new(
			ch8_core::WIDTH as f32,
			ch8_core::HEIGHT as f32,
		)),
		follow_system_theme: true,
		default_theme: eframe::Theme::Dark,
		..Default::default()
	};

	eframe::run_native(
		"Chip-8 Emulator",
		options,
		Box::new(move |cc| Box::new(gui::Gui::new(cc))),
	);
}
