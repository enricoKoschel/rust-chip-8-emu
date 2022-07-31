use crate::core;
use crate::core::Event;
use eframe::egui::Context;
use eframe::{egui, CreationContext, Frame};
use pixel_buf::PixelBuf;
use rc_event_queue::spmc::{EventQueue, EventReader};
use std::thread;

const FONT_SIZE: f32 = 1.3;

pub struct Gui {
	dark_mode: bool,
	first_frame: bool,
	show_options: bool,
	scale: usize,
	frame_no_margin: egui::containers::Frame,
	menu_bar_height: f32,
	state_receiver: single_value_channel::Receiver<core::CoreState>,
	events: EventQueue<Event>,
}

impl Gui {
	pub fn new(cc: &CreationContext) -> Self {
		let test_image = PixelBuf::new_test_image([core::BASE_WIDTH, core::BASE_HEIGHT]);
		let state = core::CoreState::new(test_image);
		let (state_receiver, state_updater) = single_value_channel::channel_starting_with(state);

		let mut events = EventQueue::<Event>::new();
		let event_reader = EventReader::new(&mut events);

		let mut updater = core::Core::new(cc.egui_ctx.clone(), state_updater, event_reader);
		thread::spawn(move || {
			updater.run();
		});

		Gui {
			dark_mode: cc.integration_info.prefer_dark_mode.unwrap_or(true),
			first_frame: true,
			show_options: false,
			scale: core::INITIAL_SCALE,
			frame_no_margin: egui::containers::Frame::default(),
			menu_bar_height: 0.0,
			state_receiver,
			events,
		}
	}

	fn setup(&mut self, ctx: &Context, frame: &mut Frame) {
		if self.dark_mode {
			ctx.set_visuals(egui::Visuals::dark());
		} else {
			ctx.set_visuals(egui::Visuals::light());
		}

		self.frame_no_margin = egui::Frame::window(&ctx.style()).inner_margin(0.0);

		if FONT_SIZE != 1.0 {
			let mut style = (*ctx.style()).clone();

			for text_style in style.text_styles.iter_mut() {
				text_style.1.size *= FONT_SIZE;
			}

			ctx.set_style(style);
		}

		self.resize_to_scale(frame);
	}

	fn latest_frame(&mut self) -> &PixelBuf {
		&self.state_receiver.latest().image
	}

	fn resize_to_scale(&mut self, frame: &mut Frame) {
		let scale = self.scale;
		let scaled_size = self.latest_frame().get_scaled_size(scale);

		frame.set_window_size(egui::Vec2::new(
			scaled_size[0] as f32,
			scaled_size[1] as f32 + self.menu_bar_height,
		));
	}

	fn add_menu_bar(&mut self, ctx: &Context, frame: &mut Frame) {
		let top_bottom_panel = egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
			egui::menu::bar(ui, |ui| {
				ui.menu_button("File", |ui| {
					if ui.checkbox(&mut self.show_options, "Options").clicked() {
						ui.close_menu();
					}

					if ui.button("Close").clicked() {
						frame.quit();
					}
				});
			});
		});

		self.menu_bar_height = top_bottom_panel.response.rect.size().y;
	}

	fn add_game_screen(&mut self, ctx: &Context) {
		let image = {
			let scale = self.scale;

			let size = self.latest_frame().get_scaled_size(scale);
			let buf = self.latest_frame().get_scaled_buf(scale);

			egui_extras::RetainedImage::from_color_image(
				"game_image",
				egui::ColorImage::from_rgba_unmultiplied(size, &buf),
			)
		};

		egui::CentralPanel::default()
			.frame(self.frame_no_margin)
			.show(ctx, |ui| {
				image.show(ui);
			});
	}

	fn add_options_window(&mut self, ctx: &Context, frame: &mut Frame) {
		let mut show_options = self.show_options;
		egui::Window::new("Options")
			.open(&mut show_options)
			.show(ctx, |ui| {
				let old_scale = self.scale;

				ui.add(egui::Slider::new(&mut self.scale, 1..=core::MAX_SCALE).text("Scale"));

				if old_scale != self.scale {
					self.resize_to_scale(frame);
				}

				let state = self.state_receiver.latest();

				let mut running = state.config.running;
				if ui.checkbox(&mut running, "Running").clicked() {
					self.events.push(Event::ChangeRunning(running));
				};

				ui.add_enabled_ui(!running, |ui| {
					if ui.button("Step frame").clicked() {
						self.events.push(Event::StepFrame);
					}
				});

				ui.label(format!("Current frame: {}", state.current_frame));

				ui.label(format!(
					"Actual frame time: {:.3}ms",
					state.actual_frame_time.as_secs_f64() * 1000.0
				));
				ui.label(format!(
					"Frame time with sleep: {:.3}ms",
					state.frame_time_with_sleep.as_secs_f64() * 1000.0
				));
				ui.label(format!("FPS: {:.3}", state.fps));
			});

		self.show_options = show_options;
	}
}

impl eframe::App for Gui {
	fn update(&mut self, ctx: &Context, frame: &mut Frame) {
		self.add_menu_bar(ctx, frame);

		//Has to be done after the menu bar is added
		if self.first_frame {
			self.first_frame = false;

			self.setup(ctx, frame);
		}

		self.add_game_screen(ctx);

		self.add_options_window(ctx, frame);
	}
}