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
	pub memory: [u8; 4096],
	///V0-VF
	pub v_registers: [u8; 16],
	///Address register - actually 12 bits
	pub i_register: u16,
	pub program_counter: u16,
	pub call_stack: Vec<u16>,
	pub delay_timer: u8,
	pub sound_timer: u8,
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
}

pub struct Core {
	ctx: egui::Context,
	state: CoreState,
	sleep_error_millis: f64,
	state_updater: single_value_channel::Updater<CoreState>,
	events: EventReader<Event, DefaultSettings>,
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
		self.execute_opcode();
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
			0x00E0 => todo!(), //Clear the screen
			0x00EE => {
				//0x00EE: Return from a subroutine
				match self.state.call_stack.pop() {
					Some(pc) => self.state.program_counter = pc,
					None => self.invalid_return(),
				};
			}
			_ => self.invalid_opcode(opcode),
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
			self.invalid_opcode(opcode);
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
			_ => self.invalid_opcode(opcode),
		}
	}

	fn execute_opcode_9(&mut self, opcode: u16) {
		//0x9XY0: Skip next instruction if VX doesn't equal VY
		let last_nibble = opcode & 0x000F;
		if last_nibble != 0x0 {
			self.invalid_opcode(opcode);
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

	fn execute_opcode_d(&mut self, _opcode: u16) {
		//0xDXYN: Draw a sprite at coordinate (VX, VY) that has a width of 8 pixels and a height of N pixels.
		//Each row is read starting from memory location I; The value of I does not change after the execution of this instruction.
		//VF is set to 1 if any screen pixels are flipped from set to unset when the sprite is drawn, and to 0 if that does not happen
		todo!();
	}

	fn execute_opcode_e(&mut self, opcode: u16) {
		let lower_byte = opcode & 0x00FF;
		let x = (opcode & 0x0F00) >> 8;

		match lower_byte {
			0x9E => {
				//0xEX9E: Skip next instruction if the key stored in VX is pressed.
				todo!();
			}
			0xA1 => {
				//0xEXA1: Skip next instruction if the key stored in VX isn't pressed.
				todo!();
			}
			_ => self.invalid_opcode(opcode),
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
				todo!();
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
				todo!();
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
			_ => self.invalid_opcode(opcode),
		}
	}

	#[inline]
	fn skip_opcode(&mut self) {
		self.state.program_counter += 2;
	}

	#[inline]
	fn invalid_opcode(&self, opcode: u16) {
		//TODO Show error in gui but keep running
		panic!(
			"Invalid opcode: '{:#X}' at PC: '{:#X}'",
			opcode,
			self.state.program_counter - 2
		);
	}

	#[inline]
	fn invalid_return(&self) {
		//TODO Show error in gui but keep running
		panic!(
			"Invalid return at PC: '{:#X}'",
			self.state.program_counter - 2
		);
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
		let lo = self.read_8bit_immediate();
		let hi = self.read_8bit_immediate();
		(hi as u16) << 8 | lo as u16
	}
}
