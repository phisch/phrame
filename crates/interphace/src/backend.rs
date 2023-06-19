use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        xdg::{
            window::{Window as XdgWindow, WindowDecorations, WindowHandler},
            XdgShell,
        },
    },
};
use wayland_client::{
    globals::GlobalList,
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
    Connection, QueueHandle,
};

use crate::window::Window;

pub struct Backend {
    queue_handle: QueueHandle<Backend>,
    compositor_state: CompositorState,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    xdg_shell: XdgShell,
    layer_shell: LayerShell,
    windows: Vec<Window>,
}

impl Backend {
    pub fn new(global_list: GlobalList, queue_handle: QueueHandle<Backend>) -> Self {
        let compositor_state =
            CompositorState::bind(&global_list, &queue_handle).expect("Compositor not available");

        let registry_state = RegistryState::new(&global_list);
        let seat_state = SeatState::new(&global_list, &queue_handle);
        let output_state = OutputState::new(&global_list, &queue_handle);

        let xdg_shell =
            XdgShell::bind(&global_list, &queue_handle).expect("Xdg shell not available");
        let layer_shell =
            LayerShell::bind(&global_list, &queue_handle).expect("Layer shell not available");

        Self {
            queue_handle,
            compositor_state,
            registry_state,
            seat_state,
            output_state,
            xdg_shell,
            layer_shell,
            windows: Vec::new(),
        }
    }

    pub fn create_surface(&self) -> WlSurface {
        self.compositor_state.create_surface(&self.queue_handle)
    }

    pub fn create_xdg_window(&self, surface: WlSurface) -> XdgWindow {
        self.xdg_shell
            .create_window(surface, WindowDecorations::None, &self.queue_handle)
    }

    pub fn add_window(&mut self, window: Window) {
        self.windows.push(window);
    }
}

delegate_xdg_shell!(Backend);
delegate_xdg_window!(Backend);
impl WindowHandler for Backend {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &XdgWindow) {
        println!("Window wants to close");
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        window: &XdgWindow,
        configure: smithay_client_toolkit::shell::xdg::window::WindowConfigure,
        _serial: u32,
    ) {
        // find thew window that has the given xdg window and call draw on it
        self.windows
            .iter_mut()
            .find(|w| &w.xdg_window == window)
            .map(|w| w.draw(qh));


        // call draw on each window
        //self.windows.iter_mut().for_each(|w| w.draw(qh));
        println!("Window configured to: {:?}", configure);
    }
}

delegate_registry!(Backend);
impl ProvidesRegistryState for Backend {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Backend);
impl CompositorHandler for Backend {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _time: u32,
    ) {
        //self.draw(&qh);
    }
}

delegate_output!(Backend);
impl OutputHandler for Backend {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wayland_client::protocol::wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wayland_client::protocol::wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wayland_client::protocol::wl_output::WlOutput,
    ) {
    }
}

delegate_seat!(Backend);
impl SeatHandler for Backend {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

delegate_layer!(Backend);
impl LayerShellHandler for Backend {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        _configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
    }
}
