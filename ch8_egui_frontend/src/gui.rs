use ch8_core::{pixel_buf::PixelBuf, Chip8Core, CoreState};
use eframe::egui::Context;
use eframe::{egui, CreationContext, Frame};
use egui::{RichText, Widget};
use egui_bind::{Bind, BindTarget};
use egui_dnd::DragDropUi;
use log::{error, trace, warn};
use std::thread;

const FONT_SIZE: f32 = 1.3;
const DEFAULT_SCALE: f32 = 4.0;

const DEFAULT_KEYMAP: [Option<(egui_bind::KeyOrPointer, egui::Modifiers)>; 16] = {
	use egui::Key::*;
	use egui::Modifiers;
	use egui_bind::KeyOrPointer;

	[
		Some((KeyOrPointer::Key(X), Modifiers::NONE)),    //0
		Some((KeyOrPointer::Key(Num1), Modifiers::NONE)), //1
		Some((KeyOrPointer::Key(Num2), Modifiers::NONE)), //2
		Some((KeyOrPointer::Key(Num3), Modifiers::NONE)), //3
		Some((KeyOrPointer::Key(Q), Modifiers::NONE)),    //4
		Some((KeyOrPointer::Key(W), Modifiers::NONE)),    //5
		Some((KeyOrPointer::Key(E), Modifiers::NONE)),    //6
		Some((KeyOrPointer::Key(A), Modifiers::NONE)),    //7
		Some((KeyOrPointer::Key(S), Modifiers::NONE)),    //8
		Some((KeyOrPointer::Key(D), Modifiers::NONE)),    //9
		Some((KeyOrPointer::Key(Y), Modifiers::NONE)),    //A
		Some((KeyOrPointer::Key(C), Modifiers::NONE)),    //B
		Some((KeyOrPointer::Key(Num4), Modifiers::NONE)), //C
		Some((KeyOrPointer::Key(R), Modifiers::NONE)),    //D
		Some((KeyOrPointer::Key(F), Modifiers::NONE)),    //E
		Some((KeyOrPointer::Key(V), Modifiers::NONE)),    //F
	]
};

#[derive(Hash, Clone)]
enum SideMenuSection {
	Rom,
	Options,
	Info,
	Keymap,
}

#[derive(Hash, Clone)]
struct SideMenuDragDropItem(SideMenuSection);

pub struct Gui {
	theme: eframe::Theme,
	first_frame: bool,
	scale: f32,
	max_scale: f32,
	transparent_frame: egui::containers::Frame,
	frame_no_margin: egui::containers::Frame,
	emu_core: Chip8Core,
	gui_error: Option<String>,
	last_rom_path: Option<std::path::PathBuf>,
	side_menu_width: f32,
	side_menu_sections: Vec<SideMenuDragDropItem>,
	side_menu_drag_state: DragDropUi,
	scale_locked: bool,
	keymap: [Option<(egui_bind::KeyOrPointer, egui::Modifiers)>; 16],
}

impl Gui {
	pub fn new(cc: &CreationContext) -> Self {
		let egui_ctx = cc.egui_ctx.clone();
		let emu_core = ch8_core::create_and_run(Box::new(move || {
			egui_ctx.request_repaint();
		}));

		let theme = cc
			.integration_info
			.system_theme
			.unwrap_or(eframe::Theme::Dark);
		trace!("Theme: {:?}", theme);

		use SideMenuSection::*;
		Gui {
			theme,
			first_frame: true,
			scale: 0.0,
			max_scale: 0.0,
			transparent_frame: egui::containers::Frame::default(),
			frame_no_margin: egui::containers::Frame::default(),
			emu_core,
			gui_error: None,
			last_rom_path: None,
			side_menu_width: 0.0,
			side_menu_sections: vec![
				SideMenuDragDropItem(Rom),
				SideMenuDragDropItem(Options),
				SideMenuDragDropItem(Info),
				SideMenuDragDropItem(Keymap),
			],
			side_menu_drag_state: DragDropUi::default(),
			scale_locked: false,
			keymap: DEFAULT_KEYMAP,
		}
	}

	fn setup(&mut self, ctx: &Context, frame: &mut Frame) {
		self.update_theme(ctx);

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

		self.frame_no_margin = egui::Frame::window(&ctx.style())
			.inner_margin(0.0)
			.shadow(egui::epaint::Shadow::NONE);

		if FONT_SIZE != 1.0 {
			let mut style = (*ctx.style()).clone();

			for text_style in style.text_styles.iter_mut() {
				text_style.1.size *= FONT_SIZE;
			}

			ctx.set_style(style);
		}

		self.setup_window(frame);
	}

	fn get_monitor_size(frame: &Frame) -> (f32, f32) {
		match frame.info().window_info.monitor_size {
			Some(size) if size != egui::Vec2::new(0.0, 0.0) => (size.x, size.y),
			_ => {
				warn!("No or zero sized monitor found, using default size");

				(
					ch8_core::WIDTH as f32 * DEFAULT_SCALE,
					ch8_core::HEIGHT as f32 * DEFAULT_SCALE,
				)
			}
		}
	}

	fn setup_window(&mut self, frame: &mut Frame) {
		let (screen_width, screen_height) = Gui::get_monitor_size(frame);
		trace!("Screen size: {}x{}", screen_width, screen_height);

		//Add 30% so max scale cannot be reached by resizing the window
		self.max_scale = (screen_width / ch8_core::WIDTH as f32).round() * 1.3;
		self.scale = (self.max_scale / 1.8).round();
		trace!("Max scale: {}, scale: {}", self.max_scale, self.scale);

		self.resize_to_scale(frame);

		//Cannot get window size with `ctx.input().screen_rect.size()`
		//because the size returned lags behind by one frame
		//and the `setup_window()` method gets called on the first frame
		let window_size = {
			let scale = self.scale;
			let mut size = self.latest_frame().get_scaled_size(scale);

			size[0] += self.side_menu_width;

			size
		};
		let monitor_size = Gui::get_monitor_size(frame);

		let screen_center = egui::Pos2::new(
			(monitor_size.0 / 2.0) - (window_size[0] / 2.0),
			(monitor_size.1 / 2.0) - (window_size[1] / 2.0),
		);
		frame.set_window_pos(screen_center);
	}

	fn update_theme(&self, ctx: &Context) {
		match self.theme {
			eframe::Theme::Dark => {
				ctx.set_visuals(egui::Visuals::dark());
			}
			eframe::Theme::Light => {
				ctx.set_visuals(egui::Visuals::light());
			}
		}
	}

	fn latest_frame(&mut self) -> &PixelBuf {
		&self.core().image
	}

	fn resize_to_scale(&mut self, frame: &mut Frame) {
		//TODO Snap to scale after resizing the window
		let scaled_size = {
			let scale = self.scale;
			self.latest_frame().get_scaled_size(scale)
		};

		trace!("Resize window to {}x{}", scaled_size[0], scaled_size[1]);
		frame.set_window_size(egui::Vec2::new(
			scaled_size[0] + self.side_menu_width,
			scaled_size[1],
		));
	}

	fn update_scale(&mut self, ctx: &Context) {
		if self.scale_locked {
			return;
		}

		let mut screen_size = ctx.input().screen_rect.size();
		screen_size.x -= self.side_menu_width;

		let scale_x = screen_size.x / ch8_core::WIDTH as f32;
		let scale_y = screen_size.y / ch8_core::HEIGHT as f32;

		let new_scale = self.max_scale.min(scale_x.min(scale_y));

		if self.scale != new_scale {
			self.scale = new_scale;
			trace!("New scale: {}", self.scale);
		}
	}

	fn add_side_menu(&mut self, ctx: &Context, frame: &mut Frame) {
		let side_menu = egui::SidePanel::right("side_menu")
			.exact_width(400.0)
			.frame(self.frame_no_margin)
			.show(ctx, |ui| {
				self.show_side_menu_sections(ctx, frame, ui);
			});

		self.side_menu_width = side_menu.response.rect.size().x;
	}

	fn show_side_menu_sections(&mut self, ctx: &Context, _frame: &mut Frame, ui: &mut egui::Ui) {
		let mut drag_state = self.side_menu_drag_state.clone();

		ui.add_space(10.0);
		ui.separator();

		let drag_response = drag_state.ui::<SideMenuDragDropItem>(
			ui,
			self.side_menu_sections.clone().iter_mut(),
			|item, ui, handle| {
				ui.horizontal(|ui| {
					handle.ui(ui, item, |ui| {
						ui.label("↕");
					});

					use SideMenuSection::*;
					match item.0 {
						Info => {
							self.show_info_section(ctx, ui);
						}
						Options => {
							self.show_options_section(ctx, ui);
						}
						Rom => {
							self.show_rom_section(ctx, ui);
						}
						Keymap => {
							self.show_keymap_section(ui);
						}
					}
				});

				ui.separator();
			},
		);

		self.scale_locked = drag_response.current_drag.is_some();

		if let Some(response) = drag_response.completed {
			egui_dnd::utils::shift_vec(response.from, response.to, &mut self.side_menu_sections);
		}

		self.side_menu_drag_state = drag_state;
	}

	fn show_rom_section(&mut self, ctx: &Context, ui: &mut egui::Ui) {
		egui::CollapsingHeader::new("ROM")
			.default_open(true)
			.show(ui, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					let core = self.core();

					let rom_name = core.rom_name.clone().unwrap_or_else(|| "---".into());
					let rom_size = {
						let rom_size = core.rom_size.unwrap_or(0);

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
							if core.rom_name.is_some() {
								//Reset core if a rom was already loaded
								self.reset_core(ctx);
							}

							self.last_rom_path = Some(path.clone());
							self.send_event(ch8_core::Event::LoadRom(path));
							self.send_event(ch8_core::Event::ChangeRunning(true));
						} else {
							error!("Error while picking rom file");

							self.gui_error =
								Some("Error while picking rom file, please try again".into());
						}
					}
				});
			});
	}

	fn show_options_section(&mut self, ctx: &Context, ui: &mut egui::Ui) {
		egui::CollapsingHeader::new("Options")
			.default_open(true)
			.show(ui, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					let core = self.core();

					let mut opcodes_per_frame = core.opcodes_per_frame;
					let slider = ui.add(
						egui::Slider::new(&mut opcodes_per_frame, 1..=200)
							.text("Opcodes per frame"),
					);

					if slider.changed() {
						self.send_event(ch8_core::Event::ChangeOpcodesPerFrame(opcodes_per_frame));
					}
					if slider.double_clicked() {
						self.send_event(ch8_core::Event::ChangeOpcodesPerFrame(20));
					}

					self.show_running_and_step_frame(ui);

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
	}

	fn show_info_section(&mut self, ctx: &Context, ui: &mut egui::Ui) {
		egui::CollapsingHeader::new("Info")
			.default_open(true)
			.show(ui, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					let core = self.core();

					ui.label(format!("Current frame (core): {}", core.current_frame));

					ui.label(format!(
						"Actual frame time (core): {:.3}ms",
						core.actual_frame_time.as_secs_f64() * 1000.0
					));
					ui.label(format!(
						"Frame time with sleep (core): {:.3}ms",
						core.frame_time_with_sleep.as_secs_f64() * 1000.0
					));
					ui.label(format!("FPS (core): {:.3}", core.fps));

					ui.separator();

					let gui_millis = ctx.input().unstable_dt * 1000.0;
					ui.label(format!("Frame time (GUI): {:.3}ms", gui_millis));
					ui.label(format!("FPS (GUI): {:.3}", 1000.0 / gui_millis));
				});
			});
	}

	fn show_keymap_section(&mut self, ui: &mut egui::Ui) {
		egui::CollapsingHeader::new("Keymap")
			.default_open(true)
			.show(ui, |ui| {
				ui.add_enabled_ui(!self.error_occurred(), |ui| {
					//CHIP-8 Keypad
					//1 2 3 C
					//4 5 6 D
					//7 8 9 E
					//A 0 B F

					ui.horizontal(|ui| {
						ui.label(RichText::new("1 -").monospace());
						Bind::new("btn_1", &mut self.keymap[0x1]).ui(ui).changed();

						ui.label(RichText::new("2 -").monospace());
						Bind::new("btn_2", &mut self.keymap[0x2]).ui(ui).changed();

						ui.label(RichText::new("3 -").monospace());
						Bind::new("btn_3", &mut self.keymap[0x3]).ui(ui).changed();

						ui.label(RichText::new("C -").monospace());
						Bind::new("btn_C", &mut self.keymap[0xC]).ui(ui).changed();
					});
					ui.horizontal(|ui| {
						ui.label(RichText::new("4 -").monospace());
						Bind::new("btn_4", &mut self.keymap[0x4]).ui(ui).changed();

						ui.label(RichText::new("5 -").monospace());
						Bind::new("btn_5", &mut self.keymap[0x5]).ui(ui).changed();

						ui.label(RichText::new("6 -").monospace());
						Bind::new("btn_6", &mut self.keymap[0x6]).ui(ui).changed();

						ui.label(RichText::new("D -").monospace());
						Bind::new("btn_D", &mut self.keymap[0xD]).ui(ui).changed();
					});
					ui.horizontal(|ui| {
						ui.label(RichText::new("7 -").monospace());
						Bind::new("btn_7", &mut self.keymap[0x7]).ui(ui).changed();

						ui.label(RichText::new("8 -").monospace());
						Bind::new("btn_8", &mut self.keymap[0x8]).ui(ui).changed();

						ui.label(RichText::new("9 -").monospace());
						Bind::new("btn_9", &mut self.keymap[0x9]).ui(ui).changed();

						ui.label(RichText::new("E -").monospace());
						Bind::new("btn_E", &mut self.keymap[0xE]).ui(ui).changed();
					});
					ui.horizontal(|ui| {
						ui.label(RichText::new("A -").monospace());
						Bind::new("btn_A", &mut self.keymap[0xA]).ui(ui).changed();

						ui.label(RichText::new("0 -").monospace());
						Bind::new("btn_0", &mut self.keymap[0x0]).ui(ui).changed();

						ui.label(RichText::new("B -").monospace());
						Bind::new("btn_B", &mut self.keymap[0xB]).ui(ui).changed();

						ui.label(RichText::new("F -").monospace());
						Bind::new("btn_F", &mut self.keymap[0xF]).ui(ui).changed();
					});
				});
			});
	}

	fn add_game_screen(&mut self, ctx: &Context) {
		let image = {
			let size = self.latest_frame().get_size();
			let buf = self.latest_frame().get_buf();

			egui_extras::RetainedImage::from_color_image(
				"game_image",
				egui::ColorImage::from_rgba_unmultiplied(size, &buf),
			)
			.with_options(egui::TextureOptions::NEAREST)
		};

		let central_panel = egui::CentralPanel::default()
			.frame(self.frame_no_margin)
			.show(ctx, |ui| {
				image.show_scaled(ui, self.scale);
			});

		if !self.error_occurred() {
			central_panel.response.context_menu(|ui| {
				self.show_running_and_step_frame(ui);
			});
		}
	}

	fn show_running_and_step_frame(&mut self, ui: &mut egui::Ui) {
		let core = self.core();
		let mut running = core.running;

		ui.add_enabled_ui(core.rom_name.is_some(), |ui| {
			if ui.checkbox(&mut running, "Running").clicked() {
				self.send_event(ch8_core::Event::ChangeRunning(running));
			};

			ui.add_enabled_ui(!running, |ui| {
				if ui.button("Step frame").clicked() {
					self.send_event(ch8_core::Event::StepFrame);
				}
			});
		});

		//TODO Add step opcode button
	}

	fn check_core_error(&mut self, ctx: &Context) {
		let core = self.core().clone();

		if let Some(error) = &core.error {
			if self.show_error_window(ctx, &error.to_string()) {
				self.reset_core(ctx);
			}
		}
	}

	fn reset_core(&mut self, ctx: &Context) {
		trace!("Resetting core");

		//Keep these settings between resets
		let opcodes_per_frame = self.core().opcodes_per_frame;

		self.send_event(ch8_core::Event::Exit);

		//Sleep so the core thread has enough time to terminate
		thread::sleep(std::time::Duration::from_millis(100));

		let egui_ctx = ctx.clone();
		let emu_core = ch8_core::create_and_run(Box::new(move || {
			egui_ctx.request_repaint();
		}));
		self.emu_core = emu_core;

		self.send_event(ch8_core::Event::ChangeOpcodesPerFrame(opcodes_per_frame));
	}

	fn reset_core_keep_rom(&mut self, ctx: &Context) {
		self.reset_core(ctx);

		if let Some(path) = self.last_rom_path.clone() {
			self.send_event(ch8_core::Event::LoadRom(path));
			self.send_event(ch8_core::Event::ChangeRunning(true));
		}
	}

	fn check_gui_error(&mut self, ctx: &Context) {
		if let Some(error) = &self.gui_error {
			if self.show_error_window(ctx, error) {
				self.gui_error = None;
			}
		}
	}

	fn error_occurred(&mut self) -> bool {
		self.core().error.is_some() || self.gui_error.is_some()
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

	fn send_event(&mut self, event: ch8_core::Event) {
		match self.emu_core.send_event(event) {
			Ok(_) => {}
			Err(e) => {
				error!("Error sending event ({})", e);

				if self.core().error.is_some() {
					//If the core already reported an error, it will be caught sometime this frame and handled.
					//That means this error can be ignored.
					return;
				}

				panic!("{}", e);
			}
		}
	}

	fn core(&mut self) -> &CoreState {
		self.emu_core.get_state()
	}

	fn send_keys_to_core(&mut self, ctx: &Context) {
		let keys: [bool; 16] = (0..16)
			.map(|i| self.keymap[i].down(ctx.input()))
			.collect::<Vec<bool>>()
			.try_into()
			.expect("Shouldn't fail because the mapped range contains 16 elements");

		//TODO Don't send event if keys down haven't changed
		self.send_event(ch8_core::Event::KeysDown(keys));
	}
}

impl eframe::App for Gui {
	fn update(&mut self, ctx: &Context, frame: &mut Frame) {
		self.add_side_menu(ctx, frame);

		//Setup has to be called after `add_side_menu()`
		//because this sets `self.side_menu_width` to the correct value
		if self.first_frame {
			self.first_frame = false;

			self.setup(ctx, frame);
		}

		self.add_game_screen(ctx);

		self.send_keys_to_core(ctx);

		self.check_core_error(ctx);
		self.check_gui_error(ctx);

		self.update_scale(ctx);
	}
}
