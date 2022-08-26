use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{trace, warn};

const SINE_FREQUENCY: f32 = 440.0;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Event {
	ChangeEnabled(bool),
	ChangeSoundTimer(u8),
	ChangeFrequency(f32),
	ChangeVolume(f32),
}

pub fn create_and_run() -> (
	single_value_channel::Receiver<SoundState>,
	crossbeam_channel::Sender<Event>,
	Option<cpal::Stream>,
) {
	let state = SoundState {
		frequency: SINE_FREQUENCY,
		volume: 1.0,
	};

	let (state_receiver, state_updater) =
		single_value_channel::channel_starting_with(state.clone());

	let (event_sender, event_receiver) = crossbeam_channel::unbounded();

	let host = cpal::default_host();

	let device = match host.default_output_device() {
		Some(device) => device,
		None => {
			warn!("No audio output device found, disabling sound");
			return (state_receiver, event_sender, None);
		}
	};

	let (config, sample_format) = {
		let supported_config = match device.default_output_config() {
			Ok(config) => config,
			Err(err) => {
				warn!(
					"Error getting default audio output config, disabling sound: {:?}",
					err
				);
				return (state_receiver, event_sender, None);
			}
		};

		let sample_format = supported_config.sample_format();
		let config: cpal::StreamConfig = supported_config.into();

		(config, sample_format)
	};

	let sound = Sound {
		state,
		channels: config.channels as usize,
		sample_rate: config.sample_rate.0 as f32,
		sample_clock: 0.0,
		running: false,
		enabled: true,
		state_updater,
		events: event_receiver,
	};

	let stream = match sample_format {
		cpal::SampleFormat::F32 => run::<f32>(&device, &config, sound),
		cpal::SampleFormat::I16 => run::<i16>(&device, &config, sound),
		cpal::SampleFormat::U16 => run::<u16>(&device, &config, sound),
	};

	(state_receiver, event_sender, stream)
}

fn run<T: cpal::Sample>(
	device: &cpal::Device,
	config: &cpal::StreamConfig,
	mut sound: Sound,
) -> Option<cpal::Stream> {
	let stream = match device.build_output_stream(
		config,
		move |data: &mut [T], _: &cpal::OutputCallbackInfo| sound.write_data(data),
		|err| warn!("An error occurred in the audio stream: {:?}", err),
	) {
		Ok(stream) => stream,
		Err(err) => {
			warn!("Error creating audio stream, disabling audio: {:?}", err);
			return None;
		}
	};

	if let Err(err) = stream.play() {
		warn!("Error playing audio stream, disabling audio: {:?}", err);
		return None;
	}

	Some(stream)
}

struct Sound {
	state: SoundState,
	channels: usize,
	sample_rate: f32,
	sample_clock: f32,
	running: bool,
	enabled: bool,
	state_updater: single_value_channel::Updater<SoundState>,
	events: crossbeam_channel::Receiver<Event>,
}

#[derive(Clone)]
pub struct SoundState {
	pub frequency: f32,
	pub volume: f32,
}

impl Sound {
	fn write_data<T: cpal::Sample>(&mut self, output: &mut [T]) {
		self.handle_events();
		if !self.running || !self.enabled {
			for sample in output.iter_mut() {
				*sample = cpal::Sample::from(&0.0);
			}
			return;
		}

		for frame in output.chunks_mut(self.channels) {
			self.sample_clock = (self.sample_clock + 1.0) % self.sample_rate;
			let sample_f32 =
				(self.sample_clock * self.state.frequency * 2.0 * std::f32::consts::PI
					/ self.sample_rate)
					.sin() * (self.state.volume / 10.0);

			let sample_t = cpal::Sample::from(&sample_f32);
			for sample in frame.iter_mut() {
				*sample = sample_t;
			}
		}
	}

	fn handle_events(&mut self) {
		let mut event_handled = false;

		while let Ok(event) = self.events.try_recv() {
			trace!("Handling event: {:?}", event);

			match event {
				Event::ChangeEnabled(enabled) => {
					self.enabled = enabled;
				}
				Event::ChangeSoundTimer(sound_timer) => {
					self.running = sound_timer > 0;
				}
				Event::ChangeFrequency(frequency) => {
					self.state.frequency = frequency;
				}
				Event::ChangeVolume(volume) => {
					self.state.volume = volume;
				}
			}

			event_handled = true;
		}

		//Only update GUI if an event was handled to lower CPU usage
		if event_handled {
			//self.update_gui();
		}
	}
}
