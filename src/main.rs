//Disable terminal window opening on windows machines when built in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
mod gui;

use eframe::egui;

fn main() {
	let (screen_width, screen_height) = {
		let size = winit::event_loop::EventLoop::new()
			.available_monitors()
			.next()
			.unwrap()
			.size();

		(size.width as f32, size.height as f32)
	};

	let initial_window_size = egui::Vec2::new(
		core::BASE_WIDTH as f32 * core::INITIAL_SCALE,
		core::BASE_HEIGHT as f32 * core::INITIAL_SCALE,
	);

	let screen_center = egui::Pos2::new(
		(screen_width / 2.0) - (initial_window_size[0] / 2.0),
		(screen_height / 2.0) - (initial_window_size[1] / 2.0),
	);

	let options = eframe::NativeOptions {
		resizable: false,
		initial_window_pos: Some(screen_center),
		initial_window_size: Some(initial_window_size),
		min_window_size: Some(egui::Vec2::new(
			core::BASE_WIDTH as f32,
			core::BASE_HEIGHT as f32,
		)),
		..Default::default()
	};

	eframe::run_native(
		core::NAME,
		options,
		Box::new(|cc| Box::new(gui::Gui::new(cc))),
	);
}
