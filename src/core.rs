use eframe::egui;
use pixel_buf::PixelBuf;
use rc_event_queue::spmc::{DefaultSettings, EventReader};
use rc_event_queue::LendingIterator;
use std::time::Duration;

const FPS: f64 = 60.0;
pub const NAME: &str = "Chip-8 Emulator";
pub const BASE_WIDTH: usize = 80;
pub const BASE_HEIGHT: usize = 40;
pub const INITIAL_SCALE: usize = 10;
pub const MAX_SCALE: usize = 20;

#[derive(Clone, PartialEq, Debug)]
pub struct Config {
	pub running: bool,
	pub step_frame: bool,
}

//Implementation may be expanded in the future to include non derivable defaults
#[allow(clippy::derivable_impls)]
impl Default for Config {
	fn default() -> Self {
		Self {
			running: false,
			step_frame: false,
		}
	}
}

pub enum Event {
	ChangeRunning(bool),
	StepFrame,
}

#[derive(Clone)]
pub struct CoreState {
	pub image: PixelBuf,
	pub current_frame: u32,
	pub actual_frame_time: Duration,
	pub frame_time_with_sleep: Duration,
	pub fps: f64,
	pub config: Config,
}

impl CoreState {
	pub fn new(image: PixelBuf) -> Self {
		Self {
			image,
			current_frame: 0,
			actual_frame_time: Duration::new(0, 0),
			frame_time_with_sleep: Duration::new(0, 0),
			fps: 0.0,
			config: Config::default(),
		}
	}
}

pub struct Core {
	ctx: egui::Context,
	state: CoreState,
	sleep_error_millis: f64,
	state_updater: single_value_channel::Updater<CoreState>,
	events: EventReader<Event, DefaultSettings>,
	memory: [u8; 4096],
	///V0-VF
	v_registers: [u8; 16],
	///Address register - actually 12 bits
	i_register: u16,
	program_counter: u16,
	call_stack: Vec<u16>,
	delay_timer: u8,
	sound_timer: u8,
}

impl Core {
	pub fn new(
		ctx: egui::Context,
		state_updater: single_value_channel::Updater<CoreState>,
		events: EventReader<Event, DefaultSettings>,
	) -> Self {
		let state = CoreState::new(PixelBuf::new_test_image([BASE_WIDTH, BASE_HEIGHT]));

		Self {
			ctx,
			state,
			sleep_error_millis: 0.0,
			state_updater,
			events,
			memory: [0; 4096],
			v_registers: [0; 16],
			i_register: 0,
			//Start PC at 512 because the lower 512 bytes were reserved
			//for the interpreter on original hardware
			program_counter: 512,
			call_stack: vec![],
			delay_timer: 0,
			sound_timer: 0,
		}
	}

	pub fn run(&mut self) {
		loop {
			let start_of_frame = std::time::Instant::now();

			self.handle_events();

			let running = self.state.config.running;
			let step_frame = self.state.config.step_frame;

			if running || step_frame {
				self.state.config.step_frame = false;

				self.step_frame();
				self.update_gui();

				self.state.current_frame += 1;
			}

			//Limit the thread to 60 fps when the core is not running or frame stepping is used
			//Otherwise limit to the configured fps
			let desired_fps = if step_frame || !running { 60.0 } else { FPS };

			let actual_frame_time = start_of_frame.elapsed();
			self.limit_speed(desired_fps, actual_frame_time.as_secs_f64() * 1000.0);

			let frame_time_with_sleep = start_of_frame.elapsed();
			let fps = 1000.0 / (frame_time_with_sleep.as_secs_f64() * 1000.0);

			if running {
				self.state.actual_frame_time = actual_frame_time;
				self.state.frame_time_with_sleep = frame_time_with_sleep;
				self.state.fps = fps;
			} else {
				self.state.actual_frame_time = Duration::new(0, 0);
				self.state.frame_time_with_sleep = Duration::new(0, 0);
				self.state.fps = 0.0;
			}
		}
	}

	fn update_gui(&self) {
		self.state_updater.update(self.state.clone()).unwrap();
		self.ctx.request_repaint();
	}

	fn handle_events(&mut self) {
		while let Some(event) = self.events.iter().next() {
			match event {
				Event::ChangeRunning(running) => {
					self.state.config.running = *running;
				}
				Event::StepFrame => {
					self.state.config.step_frame = true;
				}
			}
		}

		self.update_gui();
	}

	pub fn limit_speed(&mut self, desired_fps: f64, elapsed_millis: f64) {
		let min_frame_time_millis = 1000.0 / desired_fps;
		let sleep_needed_millis = min_frame_time_millis - elapsed_millis + self.sleep_error_millis;

		if sleep_needed_millis > 0.0 {
			let time_before_sleep = std::time::Instant::now();
			let duration = Duration::from_secs_f64(sleep_needed_millis / 1000.0);
			spin_sleep::sleep(duration);

			let millis_slept = time_before_sleep.elapsed().as_secs_f64() * 1000.0;

			//Calculate the error from sleeping too much/not enough
			self.sleep_error_millis = sleep_needed_millis - millis_slept;
		} else {
			self.sleep_error_millis = 0.0;
		}
	}

	pub fn step_frame(&mut self) {
		let image = &mut self.state.image;
		let size = image.get_size();

		for y in 0..size[1] {
			for x in 0..size[0] {
				let (new_x, new_y) = ((x + 1) % size[0], (y + 1) % size[1]);

				let pixel = image.get_pixel(new_x, new_y).clone();
				image.set_pixel(x, y, pixel);
			}
		}
	}
}
