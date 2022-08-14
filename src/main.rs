//Disable terminal window opening on windows machines when built in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;
mod gui;

use eframe::egui;
use log::{trace, warn};

fn main() {
	env_logger::init();

	let (screen_width, screen_height) = {
		let monitor = winit::event_loop::EventLoop::new()
			.available_monitors()
			.next();

		let zero_size = winit::dpi::PhysicalSize::new(0u32, 0);
		match monitor {
			Some(monitor) if monitor.size() != zero_size => {
				let screen_size = monitor.size();
				(screen_size.width as f32, screen_size.height as f32)
			}
			_ => {
				warn!("No or zero sized monitor found, using default size");

				(
					core::BASE_WIDTH as f32 * core::DEFAULT_SCALE,
					core::BASE_HEIGHT as f32 * core::DEFAULT_SCALE,
				)
			}
		}
	};
	trace!("Screen size: {}x{}", screen_width, screen_height);

	//Add 30% so max scale cannot be reached by resizing the window
	let max_scale = (screen_width / core::BASE_WIDTH as f32).round() * 1.3;
	let scale = (max_scale / 1.8).round();
	trace!("Max scale: {}, scale: {}", max_scale, scale);

	let initial_window_size = egui::Vec2::new(
		core::BASE_WIDTH as f32 * scale,
		core::BASE_HEIGHT as f32 * scale,
	);
	trace!(
		"Initial window size: {}x{}",
		initial_window_size[0],
		initial_window_size[1]
	);

	let initial_window_pos = egui::Pos2::new(
		(screen_width / 2.0) - (initial_window_size[0] / 2.0),
		(screen_height / 2.0) - (initial_window_size[1] / 2.0),
	);
	trace!(
		"Initial window position: x: {}, y: {}",
		initial_window_pos[0],
		initial_window_pos[1]
	);

	let options = eframe::NativeOptions {
		resizable: true,
		initial_window_pos: Some(initial_window_pos),
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
