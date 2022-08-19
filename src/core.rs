use eframe::egui;
use log::{error, trace, warn};
use pixel_buf::{PixelBuf, Rgba};
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::time::Duration;
use std::{fmt, fs, thread};

const FPS: f64 = 60.0;
pub const NAME: &str = "Chip-8 Emulator";
pub const BASE_WIDTH: usize = 64;
pub const BASE_HEIGHT: usize = 32;
pub const DEFAULT_SCALE: f32 = 4.0;

#[derive(Debug)]
pub enum Event {
	ChangeRunning(bool),
	StepFrame,
	LoadRom(PathBuf),
	ChangeOpcodesPerFrame(u32),
	Exit,
}

impl fmt::Display for Event {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		if let Event::LoadRom(path) = self {
			write!(f, "LoadRom({})", path.display())
		} else {
			write!(f, "{:?}", self)
		}
	}
}

#[derive(Clone)]
pub enum ErrorKind {
	InvalidOpcode {
		opcode: u16,
		address: u16,
	},
	InvalidReturn {
		address: u16,
	},
	RomTooLarge {
		path: PathBuf,
		size: usize,
		allowed: usize,
	},
	InvalidRom {
		path: PathBuf,
		specific_error: String,
	},
}

impl fmt::Display for ErrorKind {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			ErrorKind::InvalidOpcode { opcode, address } => {
				write!(
					f,
					"Invalid opcode: '{:#06X}' at PC: '{:#06X}'",
					opcode, address
				)
			}
			ErrorKind::InvalidReturn { address } => {
				write!(f, "Invalid return at PC: '{:#06X}'", address)
			}
			ErrorKind::RomTooLarge {
				path,
				size,
				allowed,
			} => {
				write!(
					f,
					"ROM '{}' is too large: '{}' bytes, allowed: '{}' bytes",
					path.to_string_lossy(),
					size,
					allowed
				)
			}
			ErrorKind::InvalidRom {
				path,
				specific_error,
			} => {
				write!(
					f,
					"Invalid ROM '{}': '{}'",
					path.to_string_lossy(),
					specific_error
				)
			}
		}
	}
}

#[derive(Clone)]
pub struct CoreState {
	pub image: PixelBuf,
	pub current_frame: u32,
	pub actual_frame_time: Duration,
	pub frame_time_with_sleep: Duration,
	pub fps: f64,
	pub running: bool,
	pub step_frame: bool,
	pub error: Option<ErrorKind>,
	pub memory: [u8; 4096],
	///V0-VF
	pub v_registers: [u8; 16],
	///Address register - actually 12 bits
	pub i_register: u16,
	pub program_counter: u16,
	pub call_stack: Vec<u16>,
	pub delay_timer: u8,
	pub sound_timer: u8,
	pub key_map: [egui::Key; 16],
	pub rom_name: Option<String>,
	pub rom_size: Option<usize>,
	pub opcodes_per_frame: u32,
	pub exit_requested: bool,
}

impl CoreState {
	pub fn new(image: PixelBuf) -> Self {
		//TODO Get keymap from GUI
		let key_map = [
			egui::Key::Num0,
			egui::Key::Num1,
			egui::Key::Num2,
			egui::Key::Num3,
			egui::Key::Num4,
			egui::Key::Num5,
			egui::Key::Num6,
			egui::Key::Num7,
			egui::Key::Num8,
			egui::Key::Num9,
			egui::Key::A,
			egui::Key::B,
			egui::Key::C,
			egui::Key::D,
			egui::Key::E,
			egui::Key::F,
		];

		Self {
			image,
			current_frame: 0,
			actual_frame_time: Duration::new(0, 0),
			frame_time_with_sleep: Duration::new(0, 0),
			fps: 0.0,
			running: false,
			step_frame: false,
			error: None,
			memory: [0; 4096],
			v_registers: [0; 16],
			i_register: 0,
			//Start PC at 512 because the lower 512 bytes were reserved
			//for the interpreter on original hardware
			program_counter: 512,
			call_stack: vec![],
			delay_timer: 0,
			sound_timer: 0,
			key_map,
			rom_name: None,
			rom_size: None,
			opcodes_per_frame: 20,
			exit_requested: false,
		}
	}
}

pub struct Core {
	ctx: egui::Context,
	state: CoreState,
	sleep_error_millis: f64,
	state_updater: single_value_channel::Updater<CoreState>,
	events: crossbeam_channel::Receiver<Event>,
}

impl Core {
	pub fn create_and_run(
		ctx: egui::Context,
	) -> (
		single_value_channel::Receiver<CoreState>,
		crossbeam_channel::Sender<Event>,
	) {
		//TODO have better starting screen
		let state = CoreState::new(PixelBuf::new([BASE_WIDTH, BASE_HEIGHT]));
		let (state_receiver, state_updater) =
			single_value_channel::channel_starting_with(state.clone());

		let (event_sender, event_receiver) = crossbeam_channel::unbounded();

		let mut core = Self {
			ctx,
			state,
			sleep_error_millis: 0.0,
			state_updater,
			events: event_receiver,
		};

		core.initialise();

		thread::spawn(move || {
			core.run();
		});

		(state_receiver, event_sender)
	}

	fn initialise(&mut self) {
		self.load_font();
	}

	fn load_font(&mut self) {
		let font = [
			0b01100000, 0b11010000, 0b10010000, 0b10110000, 0b01100000, // 0
			0b00100000, 0b01100000, 0b00100000, 0b00100000, 0b01110000, // 1
			0b01100000, 0b10010000, 0b00100000, 0b01000000, 0b11110000, // 2
			0b01100000, 0b10010000, 0b00100000, 0b10010000, 0b01100000, // 3
			0b00100000, 0b01100000, 0b10100000, 0b11110000, 0b00100000, // 4
			0b11110000, 0b10000000, 0b11100000, 0b00010000, 0b11100000, // 5
			0b01100000, 0b10000000, 0b11100000, 0b10010000, 0b01100000, // 6
			0b11110000, 0b00010000, 0b00100000, 0b01000000, 0b01000000, // 7
			0b01100000, 0b10010000, 0b01100000, 0b10010000, 0b01100000, // 8
			0b01100000, 0b10010000, 0b01110000, 0b00010000, 0b01100000, // 9
			0b01100000, 0b10010000, 0b11110000, 0b10010000, 0b10010000, // A
			0b11100000, 0b10010000, 0b11100000, 0b10010000, 0b11100000, // B
			0b01100000, 0b10000000, 0b10000000, 0b10000000, 0b01100000, // C
			0b11100000, 0b10010000, 0b10010000, 0b10010000, 0b11100000, // D
			0b11110000, 0b10000000, 0b11100000, 0b10000000, 0b11110000, // E
			0b11110000, 0b10000000, 0b11100000, 0b10000000, 0b10000000, // F
		];

		self.state.memory[0..font.len()].copy_from_slice(&font);
	}

	fn load_game(&mut self, path: PathBuf) {
		let rom = match fs::read(&path) {
			Ok(rom) => rom,
			Err(e) => {
				self.core_error(ErrorKind::InvalidRom {
					path,
					specific_error: e.to_string(),
				});
				return;
			}
		};
		trace!("Loading ROM: {}", path.display());

		let file_name = match path.file_name() {
			Some(path) => path.to_string_lossy().to_string(),
			None => {
				warn!("Filename of path {} cannot be displayed", path.display());
				"<Filename cannot be displayed>".into()
			}
		};

		self.state.rom_name = Some(file_name);
		self.state.rom_size = Some(rom.len());

		//The lower 512 bytes were reserved for the interpreter on original hardware
		if rom.len() > 4096 - 512 {
			self.core_error(ErrorKind::RomTooLarge {
				path,
				size: rom.len(),
				allowed: 4096 - 512,
			});
			return;
		}

		self.state.memory[512..(rom.len() + 512)].copy_from_slice(&rom);
		trace!("ROM loaded");
	}

	pub fn run(&mut self) {
		loop {
			if self.should_exit() {
				return;
			}

			let start_of_frame = std::time::Instant::now();

			self.handle_events();

			let running = self.state.running;
			let step_frame = self.state.step_frame;

			if running || step_frame {
				self.state.step_frame = false;

				self.step_frame();
				if self.should_exit() {
					return;
				}

				self.update_gui();

				self.state.current_frame += 1;
			}

			trace!(
				"Frame {} -------------------------------------------",
				self.state.current_frame
			);

			//Limit the thread to 60 fps when the core is not running or frame stepping is used
			//Otherwise limit to the configured fps
			let desired_fps = if step_frame || !running { 60.0 } else { FPS };
			trace!("Desired FPS: {}", desired_fps);

			let actual_frame_time = start_of_frame.elapsed();
			let elapsed_millis = actual_frame_time.as_secs_f64() * 1000.0;
			trace!("Actual frame time: {}ms", elapsed_millis);

			self.limit_speed(desired_fps, elapsed_millis);

			let frame_time_with_sleep = start_of_frame.elapsed();
			trace!(
				"Frame time with sleep: {}ms",
				frame_time_with_sleep.as_secs_f64() * 1000.0
			);

			let fps = 1000.0 / (frame_time_with_sleep.as_secs_f64() * 1000.0);
			trace!("FPS: {}", fps);

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

	fn handle_events(&mut self) {
		let mut event_handled = false;

		while let Ok(event) = self.events.try_recv() {
			trace!("Handling event: {}", event);

			match event {
				Event::ChangeRunning(running) => {
					self.state.running = running;
				}
				Event::StepFrame => {
					self.state.step_frame = true;
				}
				Event::LoadRom(path) => {
					self.load_game(path);
				}
				Event::ChangeOpcodesPerFrame(opcodes_per_frame) => {
					self.state.opcodes_per_frame = opcodes_per_frame;
				}
				Event::Exit => {
					self.state.running = false;
					self.state.exit_requested = true;
				}
			}

			event_handled = true;
		}

		//Only update GUI if an event was handled to lower CPU usage
		if event_handled {
			self.update_gui();
		}
	}

	fn should_exit(&self) -> bool {
		self.state.error.is_some() || self.state.exit_requested
	}

	fn limit_speed(&mut self, desired_fps: f64, elapsed_millis: f64) {
		let min_frame_time_millis = 1000.0 / desired_fps;
		trace!("Min frame time: {}ms", min_frame_time_millis);

		let sleep_needed_millis = min_frame_time_millis - elapsed_millis + self.sleep_error_millis;

		if sleep_needed_millis > 0.0 {
			trace!("Sleeping for {}ms", sleep_needed_millis);

			let time_before_sleep = std::time::Instant::now();
			let duration = Duration::from_secs_f64(sleep_needed_millis / 1000.0);
			spin_sleep::sleep(duration);

			let millis_slept = time_before_sleep.elapsed().as_secs_f64() * 1000.0;
			trace!("Slept for {}ms", millis_slept);

			//Calculate the error from sleeping too much/not enough
			self.sleep_error_millis = sleep_needed_millis - millis_slept;
			trace!("Sleep error: {}ms", self.sleep_error_millis);
		} else {
			self.sleep_error_millis = 0.0;
			trace!("Resetting sleep error");
		}
	}

	fn step_frame(&mut self) {
		for _ in 0..self.state.opcodes_per_frame {
			self.execute_opcode();

			if self.should_exit() {
				return;
			}
		}
		self.update_timers();
	}

	fn update_timers(&mut self) {
		if self.state.delay_timer > 0 {
			self.state.delay_timer -= 1;
		}
		//TODO Play sound when sound timer is > 0
		if self.state.sound_timer > 0 {
			self.state.sound_timer -= 1;
		}
	}

	fn execute_opcode(&mut self) {
		let opcode = self.read_16bit_immediate();
		trace!(
			"Opcode: {:#06X} at {:#06X}",
			opcode,
			self.state.program_counter - 2
		);

		let first_nibble = (opcode & 0xF000) >> 12;

		match first_nibble {
			0x0 => self.execute_opcode_0(opcode),
			0x1 => self.execute_opcode_1(opcode),
			0x2 => self.execute_opcode_2(opcode),
			0x3 => self.execute_opcode_3(opcode),
			0x4 => self.execute_opcode_4(opcode),
			0x5 => self.execute_opcode_5(opcode),
			0x6 => self.execute_opcode_6(opcode),
			0x7 => self.execute_opcode_7(opcode),
			0x8 => self.execute_opcode_8(opcode),
			0x9 => self.execute_opcode_9(opcode),
			0xA => self.execute_opcode_a(opcode),
			0xB => self.execute_opcode_b(opcode),
			0xC => self.execute_opcode_c(opcode),
			0xD => self.execute_opcode_d(opcode),
			0xE => self.execute_opcode_e(opcode),
			0xF => self.execute_opcode_f(opcode),
			_ => unreachable!(),
		}
	}

	fn execute_opcode_0(&mut self, opcode: u16) {
		match opcode {
			0x00E0 => {
				//0x00E0 - Clear the display
				self.state.image.clear(Rgba::BLACK);
			}
			0x00EE => {
				//0x00EE: Return from a subroutine
				match self.state.call_stack.pop() {
					Some(pc) => self.state.program_counter = pc,
					None => self.core_error(ErrorKind::InvalidReturn {
						address: self.state.program_counter - 2,
					}),
				};
			}
			_ => {
				//Ox0NNN: Calls RCA 1802 program at address NNN
				//This opcode is ignored on modern interpreters
			}
		}
	}

	fn execute_opcode_1(&mut self, opcode: u16) {
		//0x1NNN: Jump to address NNN
		let nnn = opcode & 0x0FFF;
		self.state.program_counter = nnn;
	}

	fn execute_opcode_2(&mut self, opcode: u16) {
		//0x2NNN: Call subroutine at NNN
		let nnn = opcode & 0x0FFF;

		self.state.call_stack.push(self.state.program_counter);
		self.state.program_counter = nnn;
	}

	fn execute_opcode_3(&mut self, opcode: u16) {
		//0x3XNN: Skip next instruction if VX equals NN
		let x = (opcode & 0x0F00) >> 8;
		let nn = (opcode & 0x00FF) as u8;

		if self.state.v_registers[x as usize] == nn {
			self.skip_opcode();
		}
	}

	fn execute_opcode_4(&mut self, opcode: u16) {
		//0x4XNN: Skip next instruction if VX doesn't equal NN
		let x = (opcode & 0x0F00) >> 8;
		let nn = (opcode & 0x00FF) as u8;

		if self.state.v_registers[x as usize] != nn {
			self.skip_opcode();
		}
	}

	fn execute_opcode_5(&mut self, opcode: u16) {
		//0x5XY0: Skip next instruction if VX equals VY
		let last_nibble = opcode & 0x000F;
		if last_nibble != 0x0 {
			self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			});
		}

		let x = (opcode & 0x0F00) >> 8;
		let y = (opcode & 0x00F0) >> 4;

		if self.state.v_registers[x as usize] == self.state.v_registers[y as usize] {
			self.skip_opcode();
		}
	}

	fn execute_opcode_6(&mut self, opcode: u16) {
		//0x6XNN: Set VX to NN
		let x = (opcode & 0x0F00) >> 8;
		let nn = (opcode & 0x00FF) as u8;

		self.state.v_registers[x as usize] = nn;
	}

	fn execute_opcode_7(&mut self, opcode: u16) {
		//0x7XNN: Add NN to VX
		let x = (opcode & 0x0F00) >> 8;
		let nn = (opcode & 0x00FF) as u8;

		self.state.v_registers[x as usize] = self.state.v_registers[x as usize].wrapping_add(nn);
	}

	fn execute_opcode_8(&mut self, opcode: u16) {
		let last_nibble = opcode & 0x000F;

		match last_nibble {
			0x0 => {
				//0x8XY0: Set VX to VY
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] = self.state.v_registers[y as usize];
			}
			0x1 => {
				//0x8XY1: Set VX to VX | VY, reset VF to 0
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] |= self.state.v_registers[y as usize];
				self.state.v_registers[0xF] = 0;
			}
			0x2 => {
				//0x8XY2: Set VX to VX & VY reset VF to 0
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] &= self.state.v_registers[y as usize];
				self.state.v_registers[0xF] = 0;
			}
			0x3 => {
				//0x8XY3: Set VX to VX ^ VY reset VF to 0
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] ^= self.state.v_registers[y as usize];
				self.state.v_registers[0xF] = 0;
			}
			0x4 => {
				//0x8XY4: Add VY to VX. Set VF to 1 if there's a carry, 0 otherwise.
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				let (result, carry) = self.state.v_registers[x as usize]
					.overflowing_add(self.state.v_registers[y as usize]);

				self.state.v_registers[x as usize] = result;
				self.state.v_registers[0xF] = if carry { 1 } else { 0 };
			}
			0x5 => {
				//0x8XY5: Subtract VY from VX. Set VF to 0 if there's a borrow, 1 otherwise.
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				let (result, borrow) = self.state.v_registers[x as usize]
					.overflowing_sub(self.state.v_registers[y as usize]);

				self.state.v_registers[x as usize] = result;
				self.state.v_registers[0xF] = if borrow { 0 } else { 1 };
			}
			0x6 => {
				//0x8XY6: Store the least significant bit of VX in VF and then shift VX to the right by 1.
				let x = (opcode & 0x0F00) >> 8;

				let lsb = self.state.v_registers[x as usize] & 0x1;
				self.state.v_registers[x as usize] >>= 1;
				self.state.v_registers[0xF] = lsb;
			}
			0x7 => {
				//0x8XY7: Set VX to VY minus VX. Set VF to 0 if there's a borrow, 1 otherwise.
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				let (result, borrow) = self.state.v_registers[y as usize]
					.overflowing_sub(self.state.v_registers[x as usize]);

				self.state.v_registers[x as usize] = result;
				self.state.v_registers[0xF] = if borrow { 0 } else { 1 };
			}
			0xE => {
				//0x8XYE: Store the most significant bit of VX in VF and then shift VX to the left by 1.
				let x = (opcode & 0x0F00) >> 8;

				let msb = (self.state.v_registers[x as usize] >> 7) & 0x1;
				self.state.v_registers[x as usize] <<= 1;
				self.state.v_registers[0xF] = msb;
			}
			_ => self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			}),
		}
	}

	fn execute_opcode_9(&mut self, opcode: u16) {
		//0x9XY0: Skip next instruction if VX doesn't equal VY
		let last_nibble = opcode & 0x000F;
		if last_nibble != 0x0 {
			self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			});
		}

		let x = (opcode & 0x0F00) >> 8;
		let y = (opcode & 0x00F0) >> 4;

		if self.state.v_registers[x as usize] != self.state.v_registers[y as usize] {
			self.skip_opcode();
		}
	}

	fn execute_opcode_a(&mut self, opcode: u16) {
		//0xANNN: Set I to the address NNN
		let address = opcode & 0x0FFF;
		self.state.i_register = address;
	}

	fn execute_opcode_b(&mut self, opcode: u16) {
		//0xBNNN: Jump to address NNN plus V0
		let address = opcode & 0x0FFF;
		self.state.program_counter = self.state.v_registers[0x0] as u16 + address;
	}

	fn execute_opcode_c(&mut self, opcode: u16) {
		//0xCXNN: Set VX to a random number with a mask of NN
		let x = (opcode & 0x0F00) >> 8;
		let mask = (opcode & 0x00FF) as u8;

		self.state.v_registers[x as usize] = rand::random::<u8>() & mask;
	}

	fn execute_opcode_d(&mut self, opcode: u16) {
		//0xDXYN: Draw a sprite at coordinate (VX, VY) that has a width of 8 pixels and a height of N pixels.
		//Each row is read starting from memory location I; The value of I does not change after the execution of this instruction.
		//VF is set to 1 if any screen pixels are flipped from set to unset when the sprite is drawn, and to 0 if that does not happen

		let (x, y, height) = {
			let x = (opcode & 0x0F00) >> 8;
			let y = (opcode & 0x00F0) >> 4;
			let n = opcode & 0x000F;

			(
				self.state.v_registers[x as usize] as usize,
				self.state.v_registers[y as usize] as usize,
				n as usize,
			)
		};

		self.state.v_registers[0xF] = 0;

		for row in 0..height {
			let raw_byte = self.state.memory[self.state.i_register as usize + row];

			for col in 0..=7 {
				let x = (x % BASE_WIDTH) + col;
				let y = (y % BASE_HEIGHT) + row;

				if x > BASE_WIDTH - 1 || y > BASE_HEIGHT - 1 {
					continue;
				}

				let pixel_value = (raw_byte >> (7 - col)) & 0x1;
				let old_pixel_value = if self.state.image[(x, y)] == Rgba::WHITE {
					1
				} else {
					0
				};
				self.state.image[(x, y)] = if (pixel_value ^ old_pixel_value) == 1 {
					Rgba::WHITE
				} else {
					//Set VF to 1 if any screen pixels are flipped from set to unset when the sprite is drawn, and to 0 if that does not happen
					if old_pixel_value == 1 {
						self.state.v_registers[0xF] = 1;
					}

					Rgba::BLACK
				};
			}
		}
	}

	fn execute_opcode_e(&mut self, opcode: u16) {
		let lower_byte = opcode & 0x00FF;
		let x = (opcode & 0x0F00) >> 8;

		match lower_byte {
			0x9E => {
				//0xEX9E: Skip next instruction if the key stored in VX is pressed.
				let key = self.state.v_registers[x as usize];
				if self.is_key_down(key) {
					self.skip_opcode();
				}
			}
			0xA1 => {
				//0xEXA1: Skip next instruction if the key stored in VX isn't pressed.
				let key = self.state.v_registers[x as usize];
				if !self.is_key_down(key) {
					self.skip_opcode();
				}
			}
			_ => self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			}),
		}
	}

	fn execute_opcode_f(&mut self, opcode: u16) {
		let lower_byte = opcode & 0x00FF;
		let x = (opcode & 0x0F00) >> 8;

		match lower_byte {
			0x07 => {
				//0xFX07: Set VX to the value of the delay timer.
				self.state.v_registers[x as usize] = self.state.delay_timer;
			}
			0x0A => {
				//0xFX0A: Wait for a key press, then store the value of the key in VX.
				let key = self.wait_for_key_press();
				self.state.v_registers[x as usize] = key;
			}
			0x15 => {
				//0xFX15: Set the delay timer to VX.
				self.state.delay_timer = self.state.v_registers[x as usize];
			}
			0x18 => {
				//0xFX18: Set the sound timer to VX.
				self.state.sound_timer = self.state.v_registers[x as usize];
			}
			0x1E => {
				//0xFX1E: Add VX to I.
				self.state.i_register += self.state.v_registers[x as usize] as u16;
			}
			0x29 => {
				//0xFX29: Set I to the location of the sprite for the character in VX.
				//Characters 0-F (in hexadecimal) are represented by a 4x5 font.
				self.state.i_register = self.state.v_registers[x as usize] as u16 * 5;
			}
			0x33 => {
				//0xFX33: Store the Binary-coded decimal representation of VX at the addresses I, I+1, and I+2.
				let vx = self.state.v_registers[x as usize];
				let hundreds = vx / 100;
				let tens = (vx % 100) / 10;
				let ones = vx % 10;

				self.write_mem(self.state.i_register, hundreds);
				self.write_mem(self.state.i_register + 1, tens);
				self.write_mem(self.state.i_register + 2, ones);
			}
			0x55 => {
				//0xFX55: Store V0 to VX in memory starting at address I.
				for i in 0..=x {
					self.write_mem(self.state.i_register, self.state.v_registers[i as usize]);
					self.state.i_register += 1;
				}
			}
			0x65 => {
				//0xFX65: Read V0 to VX from memory starting at address I.
				for i in 0..=x {
					self.state.v_registers[i as usize] = self.read_mem(self.state.i_register);
					self.state.i_register += 1;
				}
			}
			_ => self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			}),
		}
	}

	fn wait_for_key_press(&self) -> u8 {
		//Update the GUI before waiting for a key press,
		//as it can take a while and the latest frame should be visible
		self.update_gui();

		//FIXME Doesnt pass test
		loop {
			for key in 0..=0xF {
				let egui_key = self.state.key_map[key as usize];

				if self.ctx.input().key_released(egui_key) {
					return key;
				}
			}
		}
	}

	fn is_key_down(&self, key: u8) -> bool {
		if key > 0xF {
			//Maybe error instead of returning false?
			return false;
		}

		let egui_key = self.state.key_map[key as usize];
		self.ctx.input().keys_down.contains(&egui_key)
	}

	#[inline]
	fn update_gui(&self) {
		//FIXME Panics sometimes even though receiver wasn't dropped?
		self.state_updater.update(self.state.clone()).unwrap();
		self.ctx.request_repaint();
	}

	#[inline]
	fn skip_opcode(&mut self) {
		self.state.program_counter += 2;
	}

	#[inline]
	fn core_error(&mut self, error: ErrorKind) {
		error!("Core error: {}", error);

		self.state.error = Some(error);
		self.update_gui();
	}

	#[inline]
	fn write_mem(&mut self, address: u16, value: u8) {
		self.state.memory[address as usize] = value;
	}

	#[inline]
	fn read_mem(&self, address: u16) -> u8 {
		self.state.memory[address as usize]
	}

	#[inline]
	fn read_8bit_immediate(&mut self) -> u8 {
		self.state.program_counter += 1;
		self.read_mem(self.state.program_counter - 1)
	}

	#[inline]
	fn read_16bit_immediate(&mut self) -> u16 {
		let hi = self.read_8bit_immediate();
		let lo = self.read_8bit_immediate();
		(hi as u16) << 8 | lo as u16
	}
}
