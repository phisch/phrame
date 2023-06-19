use std::num::NonZeroU32;

use glutin::{
    api::egl::{context::PossiblyCurrentContext, display::Display, surface::Surface},
    config::{Api, ConfigSurfaceTypes, ConfigTemplateBuilder, GetGlConfig},
    context::{ContextApi, ContextAttributesBuilder},
    prelude::{
        GlConfig, GlDisplay, NotCurrentGlContextSurfaceAccessor,
        PossiblyCurrentContextGlSurfaceAccessor,
    },
    surface::{GlSurface, SurfaceAttributesBuilder, WindowSurface},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use skia_safe::{
    gpu::{
        gl::{Format, FramebufferInfo},
        BackendRenderTarget, DirectContext, SurfaceOrigin,
    },
    ColorType, Surface as SkiaSurface,
};
use smithay_client_toolkit::shell::{xdg::window::Window, WaylandSurface};
use wayland_client::{protocol::wl_surface::WlSurface, Proxy};

pub struct GraphicsContext {
    possibly_current_context: PossiblyCurrentContext,
    pub window_surface: Surface<WindowSurface>,
    pub skia_surface: SkiaSurface,
}

impl GraphicsContext {
    pub fn new(xdg_window: &Window) -> Self {
        let (possibly_current_context, window_surface) =
            initialize_gl_context(xdg_window.wl_surface());

        println!("GL context initialized");
        let skia_surface = initialize_skia(&window_surface, &possibly_current_context);

        Self {
            possibly_current_context,
            window_surface,
            skia_surface,
        }
    }

    pub fn make_current(&mut self) {
        self.possibly_current_context
            .make_current(&self.window_surface)
            .expect("Failed to make context current");
    }
}

fn initialize_skia(
    window_surface: &Surface<WindowSurface>,
    possibly_current_context: &PossiblyCurrentContext,
) -> SkiaSurface {
    let mut gr_direct_context =
        DirectContext::new_gl(None, None).expect("Failed to create Skia DirectContext");

    let width = window_surface.width().expect("Window surface has no width");
    let height = window_surface
        .height()
        .expect("Window surface has no height");

    let sample_count = possibly_current_context.config().num_samples();
    let stencil_bits = possibly_current_context.config().stencil_size();

    let framebuffer_info = FramebufferInfo {
        fboid: Default::default(),
        format: Format::RGBA8.into(),
    };

    let gr_backend_render_target = BackendRenderTarget::new_gl(
        (width as i32, height as i32),
        sample_count as usize,
        stencil_bits as usize,
        framebuffer_info,
    );

    SkiaSurface::from_backend_render_target(
        &mut gr_direct_context,
        &gr_backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
    .expect("Failed to create Skia surface")
}

fn initialize_gl_context(
    wl_surface: &WlSurface,
) -> (PossiblyCurrentContext, Surface<WindowSurface>) {
    let mut wayland_display_handle = WaylandDisplayHandle::empty();
    wayland_display_handle.display = wl_surface
        .backend()
        .upgrade()
        .expect("Connection has been closed")
        .display_ptr() as *mut _;
    let raw_display_handle = RawDisplayHandle::Wayland(wayland_display_handle);

    let mut wayland_window_handle = WaylandWindowHandle::empty();
    wayland_window_handle.surface = wl_surface.id().as_ptr() as *mut _;
    let raw_window_handle = RawWindowHandle::Wayland(wayland_window_handle);

    let display = unsafe { Display::new(raw_display_handle) }
        .expect("Failed to initialize Wayland EGL platform");

    let config_template = ConfigTemplateBuilder::default()
        .compatible_with_native_window(raw_window_handle)
        .with_surface_type(ConfigSurfaceTypes::WINDOW)
        .with_api(Api::GLES2 | Api::GLES3 | Api::OPENGL)
        .build();

    let display_config = unsafe { display.find_configs(config_template) }
        .unwrap()
        .next()
        .expect("No available configs");

    let gl_context_attributes = ContextAttributesBuilder::default()
        .with_context_api(ContextApi::OpenGl(None))
        .build(Some(raw_window_handle));

    let gles_context_attributes = ContextAttributesBuilder::default()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));

    let not_current_context =
        unsafe { display.create_context(&display_config, &gl_context_attributes) }
            .or_else(|_| unsafe {
                display.create_context(&display_config, &gles_context_attributes)
            })
            .expect("Failed to create context");

    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::default().build(
        raw_window_handle,
        NonZeroU32::new(100).unwrap(),
        NonZeroU32::new(100).unwrap(),
    );

    let window_surface =
        unsafe { display.create_window_surface(&display_config, &surface_attributes) }
            .expect("Failed to create surface");

    let possibly_current_context = not_current_context
        .make_current(&window_surface)
        .expect("Failed to make context current");

    (possibly_current_context, window_surface)
}

impl GraphicsContext {
    pub fn swap_buffers(&mut self) {
        self.window_surface
            .swap_buffers(&self.possibly_current_context)
            .expect("Failed to swap buffers");
    }
}
