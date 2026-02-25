use gpui::*;

struct App;

impl Render for App {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .child("Hello from GPUI")
    }
}

fn main() {
    Application::new().run(|cx: &mut AppContext| {
        cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| App));
    });
}
