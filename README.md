# Operator

simple service manager

# Capabilities

- [ ] Start a service
- [x] Stop a service 
- [x] Check the status of a service 
- [ ] Hot load new service 
- [ ] Hot reload service on service file change

# Services

Service files are toml files stored in directory set by `OP_SERVICE_DIR`
environment variable. The default directory is `/tmp/op`.

The format of a service file is the following.

Example service file `spotifyd.toml`

```toml
name = "spotifyd" # name of the service
executable = "/usr/bin/spotifyd" # path to the executable
args = ["--no-daemon"] # any cli args to the program
```

Logs files for the services are located at the dir set by `OP_SERVICE_LOG_DIR`
env var. The default directory is `/tmp/oplogs`.

# Tools 

Operator provides `operatorctl` to control the service manager.

Commands currently supported by `operatorctl` are: `stop`, `status`.

Check the status of a service

```shell
[dave@fink operator]$ operatorctl status spotifyd
spotifyd.service
pid: 73113
status: stopped
```

Stop a runnig service 

```shell
[dave@fink operator]$ operatorctl stop spotifyd
Stop command has been sent to operator. Please check the status using `operatorctl status spotifyd`
```

# Building 

```shell
git clone https://codeberg.org/evsky/operator.git
cd operator
cargo build --release
```
