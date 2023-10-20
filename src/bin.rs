use operator::{engine::Engine, log};

fn main() {
    // setup logging
    log::init().unwrap();

    // create an engine
    let mut engine = Engine::new();
    engine.run();
}
