use super::error::fail;
use super::session::warn_missing;
use serde_json::json;
use soundcraft_engine::Engine;
use std::io::Write;
use std::process::ExitCode;

pub(crate) fn info(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else { return fail("info FILE") };
    if path.ends_with(".scraft") {
        let mut e = Engine::default();
        match soundcraft_engine::io::open_session(&mut e, path) {
            Ok(missing) => warn_missing(&missing),
            Err(err) => return fail(err),
        }
        outln!("{}", soundcraft_engine::inspect::session_text(&e));
        return ExitCode::SUCCESS;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return fail(format!("{path}: {e}")),
    };
    let ext = std::path::Path::new(path).extension().and_then(std::ffi::OsStr::to_str);
    match soundcraft_audio_io::probe(&bytes, ext) {
        Ok(i) => {
            outln!("{}", serde_json::to_string_pretty(&json!({"format": i.format, "sample_format": i.sample_format, "sample_rate": i.sample_rate, "channels": i.channels, "frames": i.frames, "seconds": i.duration_secs(), "bwf": i.bwf})).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(e) => fail(format!("{path}: {e}")),
    }
}
