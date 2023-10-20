/// Directory where the service files are located.
///
/// This can be set by the `OP_SERVICE_DIR` env var.
pub fn op_service_dir() -> String {
    std::env::var("OP_SERVICE_DIR").unwrap_or_else(|_| "/tmp/op".to_string())
}

/// Directory where the log files are located.
///
/// This can be set by the `OP_SERVICE_LOG_DIR` env var.
pub fn op_service_log_dir() -> String {
    std::env::var("OP_SERVICE_LOG_DIR").unwrap_or_else(|_| "/tmp/oplogs".to_string())
}
