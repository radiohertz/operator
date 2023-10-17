mod engine;
mod log;
mod service;

fn main() {
    // setup logging
    log::init().unwrap();

    // fetch all the services files
    let services = service::Service::read_service_files().unwrap();

    // create an engine
    let engine = engine::Engine::new(services);

    engine.run();
}
