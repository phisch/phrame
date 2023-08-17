use wayland_client::{globals::registry_queue_init, Connection, EventQueue};

use crate::{backend::Backend, window::Window};

pub struct Application {
    pub backend: Backend,
    event_queue: EventQueue<Backend>
}

impl Application {
    pub fn new() -> Self {
        let connection = Connection::connect_to_env().expect("Failed to connect to Wayland server");

        let (global_list, mut event_queue) = registry_queue_init(&connection).unwrap();

        let backend = Backend::new(global_list, event_queue.handle());
        
        Self {
            backend,
            event_queue
        }
    }

    pub fn run(&mut self) {
        loop {
            self.event_queue
                .blocking_dispatch(&mut self.backend)
                .unwrap();
        }
    }

    pub fn create_window(&mut self) -> &Window {
        self.backend.create_window()
    }
}
