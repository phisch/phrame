mod renderer;

use std::{ffi::CString, num::NonZeroU32};

use glow::{HasContext, Context};
use skia_safe::{
    gpu::{gl::{FramebufferInfo, Format}, BackendRenderTarget, DirectContext},
    Surface as SkiaSurface,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_seat, delegate_xdg_shell,
    delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::xdg::{
        window::{Window, WindowConfigure, WindowHandler, XdgWindowState},
        XdgShellState,
    },
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_seat, wl_surface},
    Connection, Proxy, QueueHandle,
};

use glutin::{
    api::egl::{self, context::PossiblyCurrentContext},
    config::{ConfigSurfaceTypes, GetGlConfig},
    display::GetGlDisplay,
    prelude::*,
    surface::WindowSurface,
};

fn main() {
    env_logger::init();

    let conn = Connection::connect_to_env().expect("could not connect to wayland server");
    let (global_list, mut event_queue) = registry_queue_init(&conn).expect("failed to initialize registry");
    let queue_handle = event_queue.handle();

    let registry_state = RegistryState::new(&global_list);
    let compositor_state = CompositorState::bind(&global_list, &queue_handle).expect("wl_compositor not available");
    let output_state = OutputState::new(&global_list, &queue_handle);
    let seat_state = SeatState::new(&global_list, &queue_handle);
    let xdg_shell_state = XdgShellState::bind(&global_list, &queue_handle).expect("xdg shell not available");
    let mut xdg_window_state = XdgWindowState::bind(&global_list, &queue_handle);

    let surface = compositor_state.create_surface(&queue_handle);

    let window = Window::builder()
        .title("An EGL wayland window")
        .app_id("io.github.phisch.client-toolkit-skia-egl")
        .map(
            &queue_handle,
            &xdg_shell_state,
            &mut xdg_window_state,
            surface,
        )
        .unwrap();

    let (context, surface) = init_egl(window.wl_surface(), 1280, 720);
    let context = context.make_current(&surface).unwrap();

    let glow = unsafe {
        Context::from_loader_function(|name| {
            // TODO: When glow updates, the CString conversion can be removed.
            let name = CString::new(name).unwrap();
            context.display().get_proc_address(name.as_c_str())
        })
    };

    let mut gr_context = DirectContext::new_gl(None, None).unwrap();

    let fb_info = FramebufferInfo {
        fboid: unsafe { glow.get_parameter_i32(glow::FRAMEBUFFER_BINDING) }
            .try_into()
            .unwrap(),
        format: Format::RGBA8.into(),
    };

    let skia_surface = create_surface(
        &surface,
        context.config().num_samples().into(),
        context.config().stencil_size().into(),
        &fb_info,
        &mut gr_context,
    );

    let mut example = EglExample {
        registry_state,
        compositor_state,
        output_state,
        seat_state,
        xdg_shell_state,
        xdg_window_state,
        exit: false,
        width: 300,
        height: 200,
        context,
        surface,
        //glow,
        skia_surface,
    };

    loop {
        event_queue.blocking_dispatch(&mut example).unwrap();

        if example.exit {
            println!("exiting example");
            break;
        }
    }
}

struct EglExample {
    registry_state: RegistryState,
    compositor_state: CompositorState,
    output_state: OutputState,
    seat_state: SeatState,
    xdg_shell_state: XdgShellState,
    xdg_window_state: XdgWindowState,

    exit: bool,
    width: i32,
    height: i32,
    context: PossiblyCurrentContext,
    surface: egl::surface::Surface<glutin::surface::WindowSurface>,
    //glow: Option<glow::Context>,
    skia_surface: SkiaSurface,
}

impl EglExample {
    pub fn resize(&mut self) {

        self.surface.resize(
            &self.context,
            NonZeroU32::new(self.width as u32).unwrap(),
            NonZeroU32::new(self.height as u32).unwrap(),
        );
    }

    pub fn draw(&mut self) {
        
        let canvas = self.skia_surface.canvas();

        // get current time in nanoseconds
        let now = std::time::Instant::now();

        // get a half transparent green:
        let green = skia_safe::Color::from_argb(128, 0, 0, 255);

        canvas.clear(green);
        renderer::render_frame(1, 12, 60, canvas);

        // log time difference
        let elapsed = now.elapsed();
        let nanos = elapsed.as_secs() * 1_000_000_000 + elapsed.subsec_nanos() as u64;
        println!("rendering took {} ns", nanos);

        self.skia_surface.flush_and_submit();

        self.surface.swap_buffers(&self.context).unwrap();
    }
}

/*
fn new_skia_surface(context: &PossiblyCurrentContext) -> skia_safe::Surface {
    let mut gr_context = skia_safe::gpu::DirectContext::new_gl(None, None).unwrap();
    let fb_info = FramebufferInfo {
        fboid: unsafe { glow.get_parameter_i32(glow::FRAMEBUFFER_BINDING) }
            .try_into()
            .unwrap(),
        format: skia_safe::gpu::gl::Format::RGBA8.into(),
    };

    let skia_surface = create_surface(
        &surface,
        context.config().num_samples().into(),
        context.config().stencil_size().into(),
        &fb_info,
        &mut gr_context,
    );
}
*/

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
    .unwrap()
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

impl CompositorHandler for EglExample {
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
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
}

impl OutputHandler for EglExample {
    fn output_state(&mut self) -> &mut smithay_client_toolkit::output::OutputState {
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

impl SeatHandler for EglExample {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        _capability: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _capability: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl WindowHandler for EglExample {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let (width, height) = configure.new_size.unwrap_or((1280, 800));

        self.width = width as i32;
        self.height = height as i32;
        self.resize();
        self.draw();
    }
}

delegate_compositor!(EglExample);
delegate_output!(EglExample);
delegate_seat!(EglExample);
delegate_xdg_shell!(EglExample);
delegate_xdg_window!(EglExample);
delegate_registry!(EglExample);

impl ProvidesRegistryState for EglExample {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}
