//Disable terminal window opening on windows machines when built in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
mod gui;

use eframe::egui;

fn main() {
	//TODO Add logging

	let (screen_width, screen_height) = {
		//TODO dont unwrap, propagate error through to initial_window_pos and size
		let size = winit::event_loop::EventLoop::new()
			.available_monitors()
			.next()
			.unwrap()
			.size();

		(size.width as f32, size.height as f32)
	};

	let max_scale = (screen_width / core::BASE_WIDTH as f32).round();
	let scale = (max_scale / 1.4).round();

	let initial_window_size = egui::Vec2::new(
		core::BASE_WIDTH as f32 * scale,
		core::BASE_HEIGHT as f32 * scale,
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
		Box::new(move |cc| Box::new(gui::Gui::new(cc, scale, max_scale))),
	);
}
