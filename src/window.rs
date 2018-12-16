#[cfg(feature = "gl")]
use gfx_hal::format::Format;
use winit::EventsLoop;
#[cfg(not(feature = "gl"))]
use winit::{
	Event,
	Window as BackWindow,
	WindowBuilder,
	WindowEvent,
};

#[cfg(feature = "gl")]
use crate::gfx_back::{
	config_context,
	glutin::*,
	Backend,
	Surface,
};

pub struct Window {
	events_loop: EventsLoop,
	dims: (u32, u32),
	#[cfg(not(feature = "gl"))]
	window: BackWindow,
	#[cfg(feature = "gl")]
	pub(crate) surface: Option<<Backend as gfx_hal::Backend>::Surface>,
}

impl Window {
	#[cfg(not(feature = "gl"))]
	pub fn new(dims: (u32, u32)) -> Self {
		let events_loop = EventsLoop::new();
		let window = Self::make_builder(dims).build(&events_loop).unwrap();
		Window {
			events_loop,
			dims,
			window,
		}
	}

	pub fn width(&self) -> u32 { self.dims.0 }

	pub fn height(&self) -> u32 { self.dims.1 }

	#[cfg(feature = "gl")]
	pub fn new(dims: (u32, u32)) -> Self {
		let events_loop = EventsLoop::new();
		let wb = Self::make_builder(dims);
		let window = {
			let builder =
				config_context(ContextBuilder::new(), Format::Rgba8Srgb, None).with_vsync(true);
			GlWindow::new(wb, builder, &events_loop).unwrap()
		};
		let surface = Some(Surface::from_window(window));
		Window {
			events_loop,
			dims,
			surface,
		}
	}

	fn make_builder(dims: (u32, u32)) -> WindowBuilder {
		WindowBuilder::new()
			.with_title("Vilkiss")
			.with_dimensions(dims.into())
			.with_decorations(true)
	}

	#[cfg(not(feature = "gl"))]
	pub fn window(&self) -> &BackWindow { &self.window }

	#[cfg(not(feature = "gl"))]
	pub fn resize(&mut self, dims: (u32, u32)) {
		self.dims = dims;
		self.window.set_inner_size(dims.into());
	}

	pub fn poll_events(&mut self, mut f: impl FnMut(Event)) {
		let events_loop = &mut self.events_loop;
		let mut new_dims = None;
		events_loop.poll_events(|event| {
			if let Event::WindowEvent { event, .. } = &event {
				if let WindowEvent::Resized(size) = &event {
					new_dims = Some((size.width as u32, size.height as u32))
				}
			}
			f(event);
		});
		new_dims.map(|nd| self.dims = nd);
	}
}
