use skia_safe::{Color, Paint};
use smithay_client_toolkit::{shell::{xdg::{window::Window as XdgWindow}, WaylandSurface}};
use wayland_client::{QueueHandle};

use crate::{graphics_context::GraphicsContext, application::Application, backend::Backend};

pub struct Window {
    graphics_context: GraphicsContext,
    pub xdg_window: XdgWindow
}

impl Window {
    pub fn new(application: &Application) -> Self {
        let wl_surface = application.backend.create_surface();
        let xdg_window = application.backend.create_xdg_window(wl_surface);
        xdg_window.set_title("Interphace");
        xdg_window.commit();

        let graphics_context = GraphicsContext::new(&xdg_window);

        Self {
            graphics_context,
            xdg_window
        }
    }
}

impl Window {
    pub fn draw(&mut self, qh: &QueueHandle<Backend>) {
        self.graphics_context.make_current();
        
        println!("Drawing window");
        let canvas = self.graphics_context.skia_surface.canvas();

        let mut paint = Paint::default();
        paint.set_color(Color::from_argb(150, 80, 10, 200));

        canvas.clear(Color::from_argb(190, 0, 0, 0));
        canvas.draw_circle((50.0, 50.0), 20.0, &paint);


        //self.window.wl_surface().frame(qh, self.window.wl_surface().clone());
        self.graphics_context.skia_surface.flush_and_submit();
        self.graphics_context.swap_buffers();
    }
}