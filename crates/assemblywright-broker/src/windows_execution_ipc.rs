use assemblywright_protocol::windows_execution_pipe::{
    serve_once, transact, WindowsExecutionPipeError,
};

pub fn serve_master_once(
    pipe_name: &str,
    expected_master_service_sid: &str,
    handler: impl FnOnce(&[u8]) -> Result<Vec<u8>, WindowsExecutionPipeError>,
) -> Result<(), WindowsExecutionPipeError> {
    serve_once(pipe_name, expected_master_service_sid, handler)
}

pub fn transact_executor(
    pipe_name: &str,
    expected_executor_service_sid: &str,
    request: &[u8],
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    transact(pipe_name, expected_executor_service_sid, request)
}
