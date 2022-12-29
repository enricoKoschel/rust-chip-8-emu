//Disable terminal window opening on windows machines when built in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
mod gui;
mod sound;

use eframe::egui;

fn main() {
	env_logger::init();

	let options = eframe::NativeOptions {
		resizable: true,
		min_window_size: Some(egui::Vec2::new(
			core::BASE_WIDTH as f32,
			core::BASE_HEIGHT as f32,
		)),
		follow_system_theme: true,
		default_theme: eframe::Theme::Dark,
		..Default::default()
	};

	eframe::run_native(
		core::NAME,
		options,
		Box::new(move |cc| Box::new(gui::Gui::new(cc))),
	);
}
