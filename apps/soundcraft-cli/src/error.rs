use std::process::ExitCode;

pub(crate) fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("soundcraft-cli: {msg}");
    ExitCode::FAILURE
}
