mod app;
mod components;
mod copies;
mod server;
mod util;

fn main() {
    dioxus::launch(app::App);
}
