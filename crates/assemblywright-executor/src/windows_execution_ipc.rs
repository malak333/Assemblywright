use assemblywright_protocol::windows_execution_pipe::{serve_once, WindowsExecutionPipeError};

pub fn serve_broker_once(
    pipe_name: &str,
    expected_broker_service_sid: &str,
    handler: impl FnOnce(&[u8]) -> Result<Vec<u8>, WindowsExecutionPipeError>,
) -> Result<(), WindowsExecutionPipeError> {
    serve_once(pipe_name, expected_broker_service_sid, handler)
}
