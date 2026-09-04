use assemblywright_protocol::windows_execution_pipe::{transact, WindowsExecutionPipeError};
use std::time::Duration;

pub fn transact_service(
    pipe_name: &str,
    expected_service_sid: &str,
    request: &[u8],
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    transact(pipe_name, expected_service_sid, request)
}

#[doc(hidden)]
pub fn transact_service_with_write_delay_for_native_test(
    pipe_name: &str,
    expected_service_sid: &str,
    request: &[u8],
    write_delay: Duration,
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    assemblywright_protocol::windows_execution_pipe::transact_with_write_delay_for_native_test(
        pipe_name,
        expected_service_sid,
        request,
        write_delay,
    )
}
