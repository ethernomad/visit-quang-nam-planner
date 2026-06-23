mod app;
mod components;
mod domain;
mod ingest;
mod retrieval;
mod server;

fn main() {
    dioxus::launch(app::App);
}
