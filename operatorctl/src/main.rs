fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        panic!("Invalid args");
    }

    let socket = operator::ipc::IPCStream::connect("/tmp/operator.sock").unwrap();
    socket
        .write(&operator::ipc::IPCMessage::Status {
            name: args[1].to_string(),
        })
        .unwrap();

    let data = socket.read().unwrap();
    println!("{data:?}");
}
