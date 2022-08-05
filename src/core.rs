use eframe::egui;
use pixel_buf::{PixelBuf, Rgba};
use rc_event_queue::spmc::{DefaultSettings, EventQueue, EventReader};
use rc_event_queue::LendingIterator;
use std::time::Duration;
use std::{fmt, fs, thread};

const FPS: f64 = 60.0;
pub const NAME: &str = "Chip-8 Emulator";
pub const BASE_WIDTH: usize = 80;
pub const BASE_HEIGHT: usize = 40;
pub const INITIAL_SCALE: f32 = 10.0;
pub const MAX_SCALE: f32 = 40.0;

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
pub enum ErrorKind {
	InvalidOpcode {
		opcode: u16,
		address: u16,
	},
	InvalidReturn {
		address: u16,
	},
	RomTooLarge {
		name: String,
		size: usize,
		allowed: usize,
	},
}

impl fmt::Display for ErrorKind {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
				name,
				size,
				allowed,
			} => {
				write!(
					f,
					"ROM '{}' is too large: '{}' bytes, allowed: '{}' bytes",
					name, size, allowed
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
	pub config: Config,
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
	pub key_map: std::collections::HashMap<u8, egui::Key>,
}

impl CoreState {
	pub fn new(image: PixelBuf) -> Self {
		//TODO Get keymap from GUI
		let mut key_map = std::collections::HashMap::new();
		key_map.insert(0x0, egui::Key::Num0);
		key_map.insert(0x1, egui::Key::Num1);
		key_map.insert(0x2, egui::Key::Num2);
		key_map.insert(0x3, egui::Key::Num3);
		key_map.insert(0x4, egui::Key::Num4);
		key_map.insert(0x5, egui::Key::Num5);
		key_map.insert(0x6, egui::Key::Num6);
		key_map.insert(0x7, egui::Key::Num7);
		key_map.insert(0x8, egui::Key::Num8);
		key_map.insert(0x9, egui::Key::Num9);
		key_map.insert(0xA, egui::Key::A);
		key_map.insert(0xB, egui::Key::B);
		key_map.insert(0xC, egui::Key::C);
		key_map.insert(0xD, egui::Key::D);
		key_map.insert(0xE, egui::Key::E);
		key_map.insert(0xF, egui::Key::F);

		Self {
			image,
			current_frame: 0,
			actual_frame_time: Duration::new(0, 0),
			frame_time_with_sleep: Duration::new(0, 0),
			fps: 0.0,
			config: Config::default(),
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
		}
	}
}

pub struct Core {
	ctx: egui::Context,
	state: CoreState,
	sleep_error_millis: f64,
	state_updater: single_value_channel::Updater<CoreState>,
	events: EventReader<Event, DefaultSettings>,
}

impl Core {
	pub fn create_and_run(
		ctx: egui::Context,
	) -> (single_value_channel::Receiver<CoreState>, EventQueue<Event>) {
		//TODO have better starting screen
		let state = CoreState::new(PixelBuf::new([BASE_WIDTH, BASE_HEIGHT]));
		let (state_receiver, state_updater) =
			single_value_channel::channel_starting_with(state.clone());

		let mut event_sender = EventQueue::<Event>::new();
		let event_reader = EventReader::new(&mut event_sender);

		let mut core = Self {
			ctx,
			state,
			sleep_error_millis: 0.0,
			state_updater,
			events: event_reader,
		};

		core.initialise();

		thread::spawn(move || {
			core.run();
		});

		(state_receiver, event_sender)
	}

	fn initialise(&mut self) {
		self.load_game();
		self.load_font();
	}

	fn load_font(&mut self) {
		//0
		self.state.memory[0] = 0b01100000;
		self.state.memory[1] = 0b11010000;
		self.state.memory[2] = 0b10010000;
		self.state.memory[3] = 0b10110000;
		self.state.memory[4] = 0b01100000;

		//1
		self.state.memory[5] = 0b00100000;
		self.state.memory[6] = 0b01100000;
		self.state.memory[7] = 0b00100000;
		self.state.memory[8] = 0b00100000;
		self.state.memory[9] = 0b01110000;

		//2
		self.state.memory[10] = 0b01100000;
		self.state.memory[11] = 0b10010000;
		self.state.memory[12] = 0b00100000;
		self.state.memory[13] = 0b01000000;
		self.state.memory[14] = 0b11110000;

		//3
		self.state.memory[15] = 0b01100000;
		self.state.memory[16] = 0b10010000;
		self.state.memory[17] = 0b00100000;
		self.state.memory[18] = 0b10010000;
		self.state.memory[19] = 0b01100000;

		//4
		self.state.memory[20] = 0b00100000;
		self.state.memory[21] = 0b01100000;
		self.state.memory[22] = 0b10100000;
		self.state.memory[23] = 0b11110000;
		self.state.memory[24] = 0b00100000;

		//5
		self.state.memory[25] = 0b11110000;
		self.state.memory[26] = 0b10000000;
		self.state.memory[27] = 0b11100000;
		self.state.memory[28] = 0b00010000;
		self.state.memory[29] = 0b11100000;

		//6
		self.state.memory[30] = 0b01100000;
		self.state.memory[31] = 0b10000000;
		self.state.memory[32] = 0b11100000;
		self.state.memory[33] = 0b10010000;
		self.state.memory[34] = 0b01100000;

		//7
		self.state.memory[35] = 0b11110000;
		self.state.memory[36] = 0b00010000;
		self.state.memory[37] = 0b00100000;
		self.state.memory[38] = 0b01000000;
		self.state.memory[39] = 0b01000000;

		//8
		self.state.memory[40] = 0b01100000;
		self.state.memory[41] = 0b10010000;
		self.state.memory[42] = 0b01100000;
		self.state.memory[43] = 0b10010000;
		self.state.memory[44] = 0b01100000;

		//9
		self.state.memory[45] = 0b01100000;
		self.state.memory[46] = 0b10010000;
		self.state.memory[47] = 0b01110000;
		self.state.memory[48] = 0b00010000;
		self.state.memory[49] = 0b01100000;

		//A
		self.state.memory[50] = 0b01100000;
		self.state.memory[51] = 0b10010000;
		self.state.memory[52] = 0b11110000;
		self.state.memory[53] = 0b10010000;
		self.state.memory[54] = 0b10010000;

		//B
		self.state.memory[55] = 0b11100000;
		self.state.memory[56] = 0b10010000;
		self.state.memory[57] = 0b11100000;
		self.state.memory[58] = 0b10010000;
		self.state.memory[59] = 0b11100000;

		//C
		self.state.memory[60] = 0b01100000;
		self.state.memory[61] = 0b10000000;
		self.state.memory[62] = 0b10000000;
		self.state.memory[63] = 0b10000000;
		self.state.memory[64] = 0b01100000;

		//D
		self.state.memory[65] = 0b11100000;
		self.state.memory[66] = 0b10010000;
		self.state.memory[67] = 0b10010000;
		self.state.memory[68] = 0b10010000;
		self.state.memory[69] = 0b11100000;

		//E
		self.state.memory[70] = 0b11110000;
		self.state.memory[71] = 0b10000000;
		self.state.memory[72] = 0b11100000;
		self.state.memory[73] = 0b10000000;
		self.state.memory[74] = 0b11110000;

		//F
		self.state.memory[75] = 0b11110000;
		self.state.memory[76] = 0b10000000;
		self.state.memory[77] = 0b11100000;
		self.state.memory[78] = 0b10000000;
		self.state.memory[79] = 0b10000000;
	}

	fn load_game(&mut self) {
		//TODO Get rom path from GUI
		let path = "roms/demos/Trip8 Demo (2008) [Revival Studios].ch8";
		//let path = "roms/games/Pong (1 player).ch8";
		let rom = fs::read(path).unwrap();

		//The lower 512 bytes were reserved for the interpreter on original hardware
		if rom.len() > 4096 - 512 {
			self.core_error(ErrorKind::RomTooLarge {
				name: path.to_string(),
				size: rom.len(),
				allowed: 4096 - 512,
			});
		}

		self.state.memory[512..(rom.len() + 512)].copy_from_slice(&rom);
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
				if self.state.error.is_some() {
					return;
				}

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

	fn handle_events(&mut self) {
		let mut update_gui = false;

		while let Some(event) = self.events.iter().next() {
			match event {
				Event::ChangeRunning(running) => {
					self.state.config.running = *running;

					//Update GUI to accurately display running state
					update_gui = true;
				}
				Event::StepFrame => {
					self.state.config.step_frame = true;
				}
			}
		}

		//Only update GUI when necessary to not waste CPU time
		if update_gui {
			self.update_gui();
		}
	}

	fn limit_speed(&mut self, desired_fps: f64, elapsed_millis: f64) {
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

	fn step_frame(&mut self) {
		for _ in 0..20 {
			self.execute_opcode();

			if self.state.error.is_some() {
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
			_ => self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			}),
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
				//0x8XY1: Set VX to VX | VY
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] |= self.state.v_registers[y as usize];
			}
			0x2 => {
				//0x8XY2: Set VX to VX & VY
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] &= self.state.v_registers[y as usize];
			}
			0x3 => {
				//0x8XY3: Set VX to VX ^ VY
				let x = (opcode & 0x0F00) >> 8;
				let y = (opcode & 0x00F0) >> 4;

				self.state.v_registers[x as usize] ^= self.state.v_registers[y as usize];
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

				self.state.v_registers[0xF] = self.state.v_registers[x as usize] & 0x1;
				self.state.v_registers[x as usize] >>= 1;
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

				self.state.v_registers[0xF] = (self.state.v_registers[x as usize] >> 7) & 0x1;
				self.state.v_registers[x as usize] <<= 1;
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
				self.state.v_registers[x as usize],
				self.state.v_registers[y as usize],
				n as u8,
			)
		};

		self.state.v_registers[0xF] = 0;

		for row in 0..height {
			let raw_byte = self.state.memory[self.state.i_register as usize + row as usize];

			for col in 0..=7 {
				let x = (x + col) as usize;
				let y = (y + row) as usize;
				let pixel_value = (raw_byte >> (7 - col)) & 0x1;
				let old_pixel_value = if self.state.image[(x, y)] == Rgba::WHITE {
					1
				} else {
					0
				};
				self.state.image[(x, y)] = if (pixel_value ^ old_pixel_value) == 1 {
					Rgba::WHITE
				} else {
					Rgba::BLACK
				};

				//Set VF to 1 if any screen pixels are flipped from set to unset when the sprite is drawn, and to 0 if that does not happen
				if pixel_value != old_pixel_value {
					self.state.v_registers[0xF] = 1;
				}
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
					self.write_mem(
						self.state.i_register + i,
						self.state.v_registers[i as usize],
					);
				}
			}
			0x65 => {
				//0xFX65: Read V0 to VX from memory starting at address I.
				for i in 0..=x {
					self.state.v_registers[i as usize] = self.read_mem(self.state.i_register + i);
				}
			}
			_ => self.core_error(ErrorKind::InvalidOpcode {
				opcode,
				address: self.state.program_counter - 2,
			}),
		}
	}

	fn wait_for_key_press(&self) -> u8 {
		loop {
			for key in 0..=0xF {
				if self.is_key_down(key) {
					return key;
				}
			}
		}
	}

	fn is_key_down(&self, key: u8) -> bool {
		if let Some(key_value) = self.state.key_map.get_key_value(&key) {
			self.ctx.input().keys_down.contains(key_value.1)
		} else {
			//Maybe error instead of returning false?
			false
		}
	}

	#[inline]
	fn update_gui(&self) {
		self.state_updater.update(self.state.clone()).unwrap();
		self.ctx.request_repaint();
	}

	#[inline]
	fn skip_opcode(&mut self) {
		self.state.program_counter += 2;
	}

	#[inline]
	fn core_error(&mut self, error: ErrorKind) {
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
