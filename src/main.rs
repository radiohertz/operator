mod engine;
mod log;
mod service;

fn main() {
    // setup logging
    log::init().unwrap();

    // create an engine
    let engine = engine::Engine::new();

    engine.run();
}
