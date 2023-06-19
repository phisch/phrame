use interphace::{application::{Application}, window::Window};

fn main() {
    let mut application = Application::new();
    let window = Window::new(&application);
    application.backend.add_window(window);
    
    let window2 = Window::new(&application);
    application.backend.add_window(window2);

    let window3 = Window::new(&application);
    application.backend.add_window(window3);

    application.run();
}
