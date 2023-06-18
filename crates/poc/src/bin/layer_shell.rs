use std::{convert::TryInto, num::NonZeroU32};

use glow::HasContext;
use glutin::{
    api::egl::{self, context::PossiblyCurrentContext},
    config::{ConfigSurfaceTypes, GetGlConfig},
    display::GetGlDisplay,
    prelude::{GlConfig, GlDisplay, NotCurrentGlContextSurfaceAccessor},
    surface::{GlSurface, WindowSurface},
};
use poc::renderer::render_frame;
use skia_safe::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, DirectContext},
    Surface as SkiaSurface,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT, BTN_RIGHT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    }
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Proxy, QueueHandle,
};
use xkbcommon::xkb::keysyms;

fn main() {
    env_logger::init();

    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let queue_handle = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &queue_handle).expect("wl_compositor is not available");
    let layer_shell = LayerShell::bind(&globals, &queue_handle).expect("layer shell is not available");
    
    let surface = compositor.create_surface(&queue_handle);
    
    let layer = layer_shell.create_layer_surface(&queue_handle, surface, Layer::Top, Some("simple_layer"), None);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(1000, 100);
    layer.set_anchor(Anchor::BOTTOM);
    layer.set_exclusive_zone(50);

    let region = Region::new(&compositor).expect("Failed to create region");
    region.add(0, 0, 200, 500);
    region.add(0, 0, 800, 200);
    layer.set_input_region(Some(region.wl_region()));
    layer.commit();


    let (context, surface) = init_egl(layer.wl_surface(), 1000, 1000);
    
    let context = context.make_current(&surface).unwrap();

    let mut gr_context = DirectContext::new_gl(None, None).unwrap();
    

    let skia_surface = create_surface(
        &surface,
        context.config().num_samples().into(),
        context.config().stencil_size().into(),
        &FramebufferInfo {
            fboid: Default::default(),
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
        },
        &mut gr_context,
    );


    let mut simple_layer = SimpleLayer {
        // Seats and outputs may be hotplugged at runtime, therefore we need to setup a registry state to
        // listen for seats and outputs.
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &queue_handle),
        output_state: OutputState::new(&globals, &queue_handle),

        exit: false,
        first_configure: true,
        width: 256,
        height: 256,
        shift: None,
        layer,
        keyboard: None,
        keyboard_focus: false,
        pointer: None,
        skia_surface: skia_surface,
        context: context,
        surface: surface,
    };

    // We don't draw immediately, the configure will notify us when to first draw.
    loop {
        event_queue.blocking_dispatch(&mut simple_layer).unwrap();

        if simple_layer.exit {
            println!("exiting example");
            break;
        }
    }
}

fn create_surface(
    surface: &glutin::api::egl::surface::Surface<WindowSurface>,
    sample_count: usize,
    stencil_bits: usize,
    fb_info: &FramebufferInfo,
    gr_context: &mut DirectContext,
) -> SkiaSurface {
    let backend_render_target = BackendRenderTarget::new_gl(
        (
            surface
                .width()
                .expect("surface had no width")
                .try_into()
                .unwrap(),
            surface
                .height()
                .expect("surface had no height")
                .try_into()
                .unwrap(),
        ),
        sample_count,
        stencil_bits,
        *fb_info,
    );
    SkiaSurface::from_backend_render_target(
        gr_context,
        &backend_render_target,
        skia_safe::gpu::SurfaceOrigin::BottomLeft,
        skia_safe::ColorType::RGBA8888,
        None,
        None,
    )
    .expect("Could not create skia surface")
}

fn init_egl(
    surface: &wl_surface::WlSurface,
    width: u32,
    height: u32,
) -> (
    egl::context::NotCurrentContext,
    egl::surface::Surface<glutin::surface::WindowSurface>,
) {
    let mut display_handle = raw_window_handle::WaylandDisplayHandle::empty();
    display_handle.display = surface
        .backend()
        .upgrade()
        .expect("Connection has been closed")
        .display_ptr() as *mut _;
    let display_handle = raw_window_handle::RawDisplayHandle::Wayland(display_handle);

    let mut window_handle = raw_window_handle::WaylandWindowHandle::empty();
    window_handle.surface = surface.id().as_ptr() as *mut _;
    let window_handle = raw_window_handle::RawWindowHandle::Wayland(window_handle);

    // Initialize the EGL Wayland platform
    //
    // SAFETY: The connection is valid.
    let display = unsafe { egl::display::Display::new(display_handle) }
        .expect("Failed to initialize Wayland EGL platform");

    // Find a suitable config for the window.
    let config_template = glutin::config::ConfigTemplateBuilder::default()
        .compatible_with_native_window(window_handle)
        .with_surface_type(ConfigSurfaceTypes::WINDOW)
        .with_api(
            glutin::config::Api::GLES2 | glutin::config::Api::GLES3 | glutin::config::Api::OPENGL,
        )
        .build();
    let config = unsafe { display.find_configs(config_template) }
        .unwrap()
        .next()
        .expect("No available configs");

    let gl_attrs = glutin::context::ContextAttributesBuilder::default()
        .with_context_api(glutin::context::ContextApi::OpenGl(None))
        .build(Some(window_handle));
    let gles_attrs = glutin::context::ContextAttributesBuilder::default()
        .with_context_api(glutin::context::ContextApi::Gles(None))
        .build(Some(window_handle));

    // Create a context, trying OpenGL and then Gles.
    let context = unsafe { display.create_context(&config, &gl_attrs) }
        .or_else(|_| unsafe { display.create_context(&config, &gles_attrs) })
        .expect("Failed to create context");

    let surface_attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::default()
        .build(
            window_handle,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
    let surface = unsafe { display.create_window_surface(&config, &surface_attrs) }
        .expect("Failed to create surface");

    (context, surface)
}

struct SimpleLayer {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,

    exit: bool,
    first_configure: bool,
    width: u32,
    height: u32,
    shift: Option<u32>,
    layer: LayerSurface,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_focus: bool,
    pointer: Option<wl_pointer::WlPointer>,
    skia_surface: SkiaSurface,
    context: PossiblyCurrentContext,
    surface: egl::surface::Surface<glutin::surface::WindowSurface>,
}

impl CompositorHandler for SimpleLayer {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.draw();
    }
}

impl OutputHandler for SimpleLayer {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for SimpleLayer {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if configure.new_size.0 == 0 || configure.new_size.1 == 0 {
            self.width = 256;
            self.height = 256;
        } else {
            self.width = configure.new_size.0;
            self.height = configure.new_size.1;
        }

        // Initiate the first draw.
        if self.first_configure {
            self.first_configure = false;
            self.draw();
        }
    }
}

impl SeatHandler for SimpleLayer {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            println!("Set keyboard capability");
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to create keyboard");
            self.keyboard = Some(keyboard);
        }

        if capability == Capability::Pointer && self.pointer.is_none() {
            println!("Set pointer capability");
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_some() {
            println!("Unset keyboard capability");
            self.keyboard.take().unwrap().release();
        }

        if capability == Capability::Pointer && self.pointer.is_some() {
            println!("Unset pointer capability");
            self.pointer.take().unwrap().release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for SimpleLayer {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        keysyms: &[u32],
    ) {
        if self.layer.wl_surface() == surface {
            println!("Keyboard focus on window with pressed syms: {keysyms:?}");
            self.keyboard_focus = true;
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.layer.wl_surface() == surface {
            println!("Release keyboard focus on window");
            self.keyboard_focus = false;
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        println!("Key press: {event:?}");
        // press 'esc' to exit
        if event.keysym == keysyms::KEY_Escape {
            self.exit = true;
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        println!("Key release: {event:?}");
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
    ) {
        println!("Update modifiers: {modifiers:?}");
    }
}

impl PointerHandler for SimpleLayer {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            // Ignore events for other surfaces
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                Enter { .. } => {
                    println!("Pointer entered @{:?}", event.position);
                }
                Leave { .. } => {
                    println!("Pointer left");
                }
                Motion { .. } => {}
                Press { button, .. } => {
                    if button == BTN_LEFT {
                        self.layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                        self.layer.commit();
                    }
                    if button == BTN_RIGHT {
                        self.layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
                        self.layer.commit();
                    }
                    println!("Press {:x} @ {:?}", button, event.position);
                    self.shift = self.shift.xor(Some(0));
                }
                Release { button, .. } => {
                    println!("Release {:x} @ {:?}", button, event.position);
                }
                Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    println!("Scroll H:{horizontal:?}, V:{vertical:?}");
                }
            }
        }
    }
}


impl SimpleLayer {
    pub fn draw(&mut self) {
        let canvas = self.skia_surface.canvas();

        let now = std::time::Instant::now();

        let tint = skia_safe::Color::from_argb(190, 0, 0, 0);
        canvas.clear(tint);
        render_frame(1, 12, 60, canvas);

        let elapsed = now.elapsed();
        let nanos = elapsed.as_secs() * 1_000_000_000 + elapsed.subsec_nanos() as u64;
        println!("rendering took {} ns", nanos);

        self.skia_surface.flush_and_submit();
        self.surface.swap_buffers(&self.context).unwrap();
    }
}

delegate_compositor!(SimpleLayer);
delegate_output!(SimpleLayer);

delegate_seat!(SimpleLayer);
delegate_keyboard!(SimpleLayer);
delegate_pointer!(SimpleLayer);

delegate_layer!(SimpleLayer);

delegate_registry!(SimpleLayer);

impl ProvidesRegistryState for SimpleLayer {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
