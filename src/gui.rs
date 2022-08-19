use crate::core;
use crate::core::Event;
use eframe::egui::Context;
use eframe::{egui, CreationContext, Frame};
use log::{error, trace};
use pixel_buf::PixelBuf;
use std::thread;

const FONT_SIZE: f32 = 1.3;

pub struct Gui {
	theme: eframe::Theme,
	first_frame: bool,
	show_rom_window: bool,
	show_options_window: bool,
	show_info_window: bool,
	scale: f32,
	max_scale: f32,
	transparent_frame: egui::containers::Frame,
	frame_no_margin: egui::containers::Frame,
	menu_bar_height: f32,
	state_receiver: single_value_channel::Receiver<core::CoreState>,
	events: crossbeam_channel::Sender<Event>,
	gui_error: Option<String>,
	last_rom_path: Option<std::path::PathBuf>,
}

impl Gui {
	pub fn new(cc: &CreationContext, scale: f32, max_scale: f32) -> Self {
		let (state_receiver, events) = core::Core::create_and_run(cc.egui_ctx.clone());

		let theme = cc
			.integration_info
			.system_theme
			.unwrap_or(eframe::Theme::Dark);
		trace!("Theme: {:?}", theme);

		Gui {
			theme,
			first_frame: true,
			show_rom_window: true,
			show_options_window: false,
			show_info_window: false,
			scale,
			max_scale,
			transparent_frame: egui::containers::Frame::default(),
			frame_no_margin: egui::containers::Frame::default(),
			menu_bar_height: 0.0,
			state_receiver,
			events,
			gui_error: None,
			last_rom_path: None,
		}
	}

	fn setup(&mut self, ctx: &Context, frame: &mut Frame) {
		match self.theme {
			eframe::Theme::Dark => {
				ctx.set_visuals(egui::Visuals::dark());
			}
			eframe::Theme::Light => {
				ctx.set_visuals(egui::Visuals::light());
			}
		}

		self.transparent_frame = {
			let mut transparent_frame = egui::Frame::window(&ctx.style());
			let fill = egui::Color32::from_rgba_unmultiplied(
				transparent_frame.fill.r(),
				transparent_frame.fill.g(),
				transparent_frame.fill.b(),
				230,
			);
			transparent_frame.fill = fill;

			transparent_frame
		};

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
		let scaled_size = {
			let scale = self.scale;
			self.latest_frame().get_scaled_size(scale)
		};

		trace!("Resize window to {}x{}", scaled_size[0], scaled_size[1]);
		frame.set_window_size(egui::Vec2::new(
			scaled_size[0],
			scaled_size[1] + self.menu_bar_height,
		));
	}

	fn update_scale(&mut self, ctx: &Context) {
		let mut screen_size = ctx.input().screen_rect.size();
		screen_size.y -= self.menu_bar_height;

		let scale_x = screen_size.x / core::BASE_WIDTH as f32;
		let scale_y = screen_size.y / core::BASE_HEIGHT as f32;

		let new_scale = self.max_scale.min(scale_x.min(scale_y));

		if self.scale != new_scale {
			self.scale = new_scale;
			trace!("New scale: {}", self.scale);
		}
	}

	fn add_menu_bar(&mut self, ctx: &Context, frame: &mut Frame) {
		let top_bottom_panel = egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
			egui::menu::bar(ui, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					ui.menu_button("File", |ui| {
						if ui.checkbox(&mut self.show_rom_window, "Rom").clicked() {
							ui.close_menu();
						}

						if ui
							.checkbox(&mut self.show_options_window, "Options")
							.clicked()
						{
							ui.close_menu();
						}

						if ui.checkbox(&mut self.show_info_window, "Info").clicked() {
							ui.close_menu();
						}

						ui.separator();

						if ui.button("Close").clicked() {
							frame.quit();
						}
					});
				});
			});
		});

		self.menu_bar_height = top_bottom_panel.response.rect.size().y;
	}

	fn add_rom_window(&mut self, ctx: &Context) {
		let mut show_rom_window = self.show_rom_window;
		egui::Window::new("Rom")
			.open(&mut show_rom_window)
			.frame(self.transparent_frame)
			.show(ctx, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					let state = self.state_receiver.latest();

					let rom_name = state.rom_name.clone().unwrap_or_else(|| "---".into());
					let rom_size = {
						let rom_size = state.rom_size.unwrap_or(0);

						if rom_size == 0 {
							"---".into()
						} else if rom_size == 1 {
							"1 byte".into()
						} else {
							format!("{} bytes", rom_size)
						}
					};

					ui.label(format!("Rom name: {}", rom_name));
					ui.label(format!("Rom size: {}", rom_size));

					if ui.button("Load").clicked() {
						//TODO Implement dragging the ROM onto the gui
						let path = rfd::FileDialog::new()
							.add_filter("CH8 files", &["ch8"])
							.pick_file();

						if let Some(path) = path {
							trace!("ROM file picked: {}", path.display());
							if state.rom_name.is_some() {
								//Reset core if a rom was already loaded
								self.reset_core(ctx);
							}

							self.last_rom_path = Some(path.clone());
							self.send_event(Event::LoadRom(path));
							self.send_event(Event::ChangeRunning(true));
						} else {
							error!("Error while picking rom file");

							self.gui_error =
								Some("Error while picking rom file, please try again".into());
						}
					}
				});
			});

		self.show_rom_window = show_rom_window;
	}

	fn add_options_window(&mut self, ctx: &Context, frame: &mut Frame) {
		let mut show_options_window = self.show_options_window;
		egui::Window::new("Options")
			.open(&mut show_options_window)
			.frame(self.transparent_frame)
			.show(ctx, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					ui.horizontal(|ui| {
						let scale_slider = ui.add(
							egui::Slider::new(&mut self.scale, 1.0..=self.max_scale).text("Scale"),
						);

						let button = ui.button("Snap to scale");
						if button.clicked() || scale_slider.changed() {
							self.resize_to_scale(frame);
						}
					});

					ui.separator();

					let state = self.state_receiver.latest();

					let mut opcodes_per_frame = state.opcodes_per_frame;
					let slider = ui.add(
						egui::Slider::new(&mut opcodes_per_frame, 1..=100)
							.text("Opcodes per frame"),
					);

					if slider.changed() {
						self.send_event(Event::ChangeOpcodesPerFrame(opcodes_per_frame));
					}
					if slider.double_clicked() {
						self.send_event(Event::ChangeOpcodesPerFrame(20));
					}

					self.add_running_and_step_frame(ui);

					ui.separator();

					ui.horizontal(|ui| {
						if ui.button("Reset").clicked() {
							self.reset_core(ctx);
						}
						if ui.button("Reset ROM").clicked() {
							self.reset_core_keep_rom(ctx);
						}
					});
				});
			});

		self.show_options_window = show_options_window;
	}

	fn reset_core(&mut self, ctx: &Context) {
		//Keep opcodes per frame between resets
		let opcodes_per_frame = self.state_receiver.latest().opcodes_per_frame;

		self.send_event(Event::Exit);

		//Sleep so the other thread has enough time to terminate
		thread::sleep(std::time::Duration::from_millis(100));

		self.create_new_core(ctx);
		self.send_event(Event::ChangeOpcodesPerFrame(opcodes_per_frame));
	}

	fn reset_core_keep_rom(&mut self, ctx: &Context) {
		self.reset_core(ctx);

		if let Some(path) = self.last_rom_path.clone() {
			self.send_event(Event::LoadRom(path));
			self.send_event(Event::ChangeRunning(true));
		}
	}

	fn add_info_window(&mut self, ctx: &Context) {
		let mut show_info_window = self.show_info_window;
		egui::Window::new("Info")
			.open(&mut show_info_window)
			.frame(self.transparent_frame)
			.show(ctx, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					let state = self.state_receiver.latest();

					ui.label(format!("Current frame (core): {}", state.current_frame));

					ui.label(format!(
						"Actual frame time (core): {:.3}ms",
						state.actual_frame_time.as_secs_f64() * 1000.0
					));
					ui.label(format!(
						"Frame time with sleep (core): {:.3}ms",
						state.frame_time_with_sleep.as_secs_f64() * 1000.0
					));
					ui.label(format!("FPS (core): {:.3}", state.fps));

					ui.separator();

					let gui_millis = ctx.input().unstable_dt * 1000.0;
					ui.label(format!("Frame time (GUI): {:.3}ms", gui_millis));
					ui.label(format!("FPS (GUI): {:.3}", 1000.0 / gui_millis));
				});
			});

		self.show_info_window = show_info_window;
	}

	fn add_game_screen(&mut self, ctx: &Context) {
		let image = {
			let size = self.latest_frame().get_size();
			let buf = self.latest_frame().get_buf();

			egui_extras::RetainedImage::from_color_image(
				"game_image",
				egui::ColorImage::from_rgba_unmultiplied(size, &buf),
			)
			.with_texture_filter(egui::TextureFilter::Nearest)
		};

		let central_panel = egui::CentralPanel::default()
			.frame(self.frame_no_margin)
			.show(ctx, |ui| {
				image.show_scaled(ui, self.scale);
			});

		if !self.error_occurred() {
			central_panel.response.context_menu(|ui| {
				self.add_running_and_step_frame(ui);
			});
		}
	}

	fn add_running_and_step_frame(&mut self, ui: &mut egui::Ui) {
		let state = self.state_receiver.latest();
		let mut running = state.running;

		ui.add_enabled_ui(state.rom_name.is_some(), |ui| {
			if ui.checkbox(&mut running, "Running").clicked() {
				self.send_event(Event::ChangeRunning(running));
			};

			ui.add_enabled_ui(!running, |ui| {
				if ui.button("Step frame").clicked() {
					self.send_event(Event::StepFrame);
				}
			});
		});

		//TODO Add step opcode button
	}

	fn check_core_error(&mut self, ctx: &Context) {
		let state = self.state_receiver.latest().clone();

		if let Some(error) = &state.error {
			if self.show_error_window(ctx, &error.to_string()) {
				self.create_new_core(ctx);
			}
		}
	}

	fn create_new_core(&mut self, ctx: &Context) {
		trace!("Creating new core");

		let (state_receiver, events) = core::Core::create_and_run(ctx.clone());
		self.state_receiver = state_receiver;
		self.events = events;
	}

	fn check_gui_error(&mut self, ctx: &Context) {
		if let Some(error) = &self.gui_error {
			if self.show_error_window(ctx, error) {
				self.gui_error = None;
			}
		}
	}

	fn error_occurred(&mut self) -> bool {
		self.state_receiver.latest().error.is_some() || self.gui_error.is_some()
	}

	fn show_error_window(&self, ctx: &Context, error: &str) -> bool {
		let mut clicked = false;
		egui::Window::new("Error")
			.frame(self.transparent_frame)
			.show(ctx, |ui| {
				ui.colored_label(ui.visuals().error_fg_color, error);

				clicked = ui.button("Ok").clicked();
			});

		clicked
	}

	fn send_event(&mut self, event: Event) {
		match self.events.send(event) {
			Ok(_) => {}
			Err(e) => {
				error!("Error sending event: {}", e);

				if self.state_receiver.latest().error.is_some() {
					//If the core already reported an error, it will be caught sometime this frame and handled.
					//That means this error can be ignored.
					return;
				}

				panic!("{}", e);
			}
		}
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

		self.add_rom_window(ctx);
		self.add_options_window(ctx, frame);
		self.add_info_window(ctx);

		self.check_core_error(ctx);
		self.check_gui_error(ctx);

		self.update_scale(ctx);
	}
}
