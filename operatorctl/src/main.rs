use clap::{Parser, Subcommand};
use colored::*;
use operator::{
    ipc::{IPCMessage, IPCStream},
    service::ServiceStatus,
};

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// check the status of a service
    Status { name: String },
    /// Stop a service by name
    Stop { name: String },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Status { name }) => {
            let socket = sock();

            socket
                .write(&operator::ipc::IPCMessage::Status {
                    name: name.to_string(),
                })
                .unwrap();

            let data = socket.read().unwrap();
            match data {
                IPCMessage::StatusResponse(Some((pid, status))) => {
                    println!("{}", format!("{name}.service").green());
                    println!("{}", format!("pid: {pid}").green());
                    let status = match status {
                        ServiceStatus::Running => "running".green(),
                        ServiceStatus::Stopped => "stopped".red(),
                        _ => "unknow".red(),
                    };
                    println!("{}", format!("status: {}", status).green());
                }
                IPCMessage::StatusResponse(None) => {
                    println!("{}", format!("no {name} service found.").red());
                }
                _ => {}
            };
        }
        Some(Command::Stop { name }) => {
            let socket = sock();

            socket
                .write(&operator::ipc::IPCMessage::Stop {
                    name: name.to_string(),
                })
                .unwrap();

            println!("{}", format!("Stop command has been sent to operator. Please check the status using `operatorctl status {name}`").green());
        }
        None => {}
    }
}

fn sock() -> IPCStream {
    operator::ipc::IPCStream::connect("/tmp/operator.sock").unwrap()
}
