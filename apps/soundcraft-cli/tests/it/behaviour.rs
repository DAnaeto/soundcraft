//! How `soundcraft-cli` parses its arguments and reports results today, run as the real binary.
//! These pin the hand-written parser's behaviour, quirks included; a change to it updates them on purpose.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use soundcraft_audio_io::{AudioBuffer, BitDepth, EncodeOptions, FileFormat};

/// What one run of the binary produced.
struct Invoke {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Runs the binary in `dir` with empty stdin and no control-port variable.
fn invoke(dir: &Path, args: &[&str]) -> Invoke {
    invoke_env(dir, args, None)
}

fn invoke_env(dir: &Path, args: &[&str], control_port: Option<&str>) -> Invoke {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_soundcraft-cli"));
    cmd.env("SOUNDCRAFT_NO_PREFS", "1").env_remove("SOUNDCRAFT_CONTROL_PORT");
    if let Some(port) = control_port {
        cmd.env("SOUNDCRAFT_CONTROL_PORT", port);
    }
    let out = cmd.current_dir(dir).args(args).stdin(Stdio::null()).output().unwrap();
    Invoke { code: out.status.code().unwrap(), stdout: String::from_utf8(out.stdout).unwrap(), stderr: String::from_utf8(out.stderr).unwrap() }
}

/// A fresh directory holding `in.wav` (200 samples, 16-bit mono, 48 kHz) and `s.txt` (a one-command script).
fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sc-cli-behaviour-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut buf = AudioBuffer::new(48_000, 1, 200);
    for (i, d) in buf.channels[0].iter_mut().enumerate() {
        *d = i as f32 / 400.0;
    }
    let opts = EncodeOptions { format: FileFormat::Wav, bit_depth: BitDepth::Int16, dither: false, bwf: None };
    std::fs::write(dir.join("in.wav"), soundcraft_audio_io::encode(&buf, &opts).unwrap()).unwrap();
    std::fs::write(dir.join("s.txt"), "track.new {}\n").unwrap();
    dir
}

/// The sample rate of the WAV `name` in `dir`.
fn rate_of(dir: &Path, name: &str) -> u32 {
    soundcraft_audio_io::probe(&std::fs::read(dir.join(name)).unwrap(), None).unwrap().sample_rate
}

/// Bit depth of the WAV `name` in `dir`, as the probe reports the sample format.
fn format_of(dir: &Path, name: &str) -> String {
    format!("{:?}", soundcraft_audio_io::probe(&std::fs::read(dir.join(name)).unwrap(), None).unwrap().sample_format)
}

/// Exit 1, nothing on stdout, and exactly `soundcraft-cli: {message}` on stderr.
#[track_caller]
fn assert_fails(got: &Invoke, message: &str) {
    assert_eq!(got.code, 1, "stderr: {}", got.stderr);
    assert_eq!(got.stdout, "");
    assert_eq!(got.stderr, format!("soundcraft-cli: {message}\n"));
}

/// Exit 0 with nothing on either stream.
#[track_caller]
fn assert_silent_success(got: &Invoke) {
    assert_eq!((got.code, got.stdout.as_str(), got.stderr.as_str()), (0, "", ""));
}

const USAGE_FIRST_LINE: &str = "soundcraft-cli info FILE                       audio file or .scraft summary";
const USAGE_LAST_LINE: &str = "soundcraft-cli plugins                          list built-in plugins";
const NO_ARGS_USAGE: &str = "usage: soundcraft-cli <info|convert|run|script|commands|describe|app|mcp|parity|plugins> …  (see --help)\n";
/// The OS's message for a missing file: error 2 is `ENOENT` on Unix and `ERROR_FILE_NOT_FOUND` on Windows.
fn not_found() -> std::io::Error {
    std::io::Error::from_raw_os_error(2)
}

/// Exit 1 with the refused-connection message for `port`; the OS's text for the refusal differs by platform.
#[track_caller]
fn assert_refused(got: &Invoke, port: u16) {
    assert_eq!((got.code, got.stdout.as_str()), (1, ""));
    let prefix = format!("soundcraft-cli: cannot reach SoundCraft at 127.0.0.1:{port}: ");
    assert!(got.stderr.starts_with(&prefix) && got.stderr.contains(" (os error ") && got.stderr.ends_with(")\n"), "{}", got.stderr);
}

// ---- top level ----

#[test]
fn top_level_help_spellings_print_the_usage_block_to_stdout() {
    let dir = sandbox("help");
    let long = invoke(&dir, &["--help"]);
    assert_eq!(long.code, 0);
    assert_eq!(long.stderr, "");
    assert!(long.stdout.starts_with(USAGE_FIRST_LINE), "{}", long.stdout);
    assert!(long.stdout.ends_with(&format!("{USAGE_LAST_LINE}\n")), "{}", long.stdout);
    for spelling in ["-h", "help"] {
        let got = invoke(&dir, &[spelling]);
        assert_eq!((got.code, got.stdout, got.stderr), (0, long.stdout.clone(), String::new()), "{spelling}");
    }
}

#[test]
fn top_level_version_spellings_print_the_version_to_stdout() {
    let dir = sandbox("version");
    let expected = format!("SoundCraft CLI {}\n", env!("CARGO_PKG_VERSION"));
    for spelling in ["--version", "-V", "version"] {
        let got = invoke(&dir, &[spelling]);
        assert_eq!((got.code, got.stdout, got.stderr), (0, expected.clone(), String::new()), "{spelling}");
    }
}

#[test]
fn no_arguments_prints_a_one_line_usage_to_stderr_and_fails() {
    let got = invoke(&sandbox("noargs"), &[]);
    assert_eq!((got.code, got.stdout.as_str(), got.stderr.as_str()), (1, "", NO_ARGS_USAGE));
}

#[test]
fn unknown_subcommand_fails_naming_it() {
    let dir = sandbox("unknown-sub");
    assert_fails(&invoke(&dir, &["bogus"]), "unknown subcommand `bogus`");
    // Only the first argument is the subcommand: `--help` after it does not rescue it.
    assert_fails(&invoke(&dir, &["bogus", "--help"]), "unknown subcommand `bogus`");
    // Subcommand names are case sensitive and not abbreviated.
    assert_fails(&invoke(&dir, &["INFO"]), "unknown subcommand `INFO`");
    assert_fails(&invoke(&dir, &["plug"]), "unknown subcommand `plug`");
    // A leading unknown flag is an unknown subcommand too.
    assert_fails(&invoke(&dir, &["--bogus"]), "unknown subcommand `--bogus`");
}

// ---- --help / -h after each subcommand ----

#[test]
fn info_and_script_read_help_as_a_file_name() {
    let dir = sandbox("help-file");
    for sub in ["info", "script"] {
        for flag in ["--help", "-h"] {
            assert_fails(&invoke(&dir, &[sub, flag]), &format!("{flag}: {}", not_found()));
        }
    }
}

#[test]
fn convert_and_app_help_fall_through_to_their_usage_or_connection_error() {
    let dir = sandbox("help-convert-app");
    // convert needs two positionals; one flag is not enough.
    for flag in ["--help", "-h"] {
        assert_fails(&invoke(&dir, &["convert", flag]), "convert IN OUT");
    }
    // `app --help` has no method, so it prints its usage; `app -h` takes `-h` as the method and
    // tries the default port (7979) -- pinned via an explicit refused port.
    assert_fails(&invoke(&dir, &["app", "--help"]), "app [--port P] METHOD [JSON]");
    assert_refused(&invoke(&dir, &["app", "--port", "1", "-h"]), 1);
}

#[test]
fn run_rejects_long_help_but_ignores_short_help() {
    let dir = sandbox("help-run");
    assert_fails(&invoke(&dir, &["run", "--help"]), "unknown option --help");
    // `-h` has no `--` prefix, so the option check skips it like any positional.
    assert_silent_success(&invoke(&dir, &["run", "-h"]));
}

#[test]
fn describe_reads_help_as_a_command_id() {
    let dir = sandbox("help-describe");
    assert_fails(&invoke(&dir, &["describe", "--help"]), "unknown command `--help`");
    assert_fails(&invoke(&dir, &["describe", "-h"]), "unknown command `-h`");
}

#[test]
fn commands_reads_help_as_a_filter_that_matches_nothing() {
    let dir = sandbox("help-commands");
    for flag in ["--help", "-h"] {
        assert_silent_success(&invoke(&dir, &["commands", flag]));
    }
}

#[test]
fn mcp_ignores_help_and_exits_cleanly_on_empty_stdin() {
    let dir = sandbox("help-mcp");
    for flag in ["--help", "-h"] {
        assert_silent_success(&invoke(&dir, &["mcp", flag]));
    }
}

#[test]
fn parity_and_plugins_ignore_help_and_print_their_normal_output() {
    let dir = sandbox("help-parity-plugins");
    let parity = invoke(&dir, &["parity"]);
    let plugins = invoke(&dir, &["plugins"]);
    for flag in ["--help", "-h"] {
        let got = invoke(&dir, &["parity", flag]);
        assert_eq!((got.code, got.stdout, got.stderr), (0, parity.stdout.clone(), String::new()), "parity {flag}");
        let got = invoke(&dir, &["plugins", flag]);
        assert_eq!((got.code, got.stdout, got.stderr), (0, plugins.stdout.clone(), String::new()), "plugins {flag}");
    }
}

// ---- unknown flag on every subcommand ----

#[test]
fn unknown_flag_is_rejected_by_convert_run_and_script() {
    let dir = sandbox("unknown-flag-reject");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "out.wav", "--bogus"]), "unknown option --bogus");
    assert_fails(&invoke(&dir, &["run", "--bogus"]), "unknown option --bogus");
    assert_fails(&invoke(&dir, &["run", "--bogus", "--cmd", "track.new"]), "unknown option --bogus");
    assert_fails(&invoke(&dir, &["script", "s.txt", "--bogus"]), "unknown option --bogus");
    assert!(!dir.join("out.wav").exists());
}

#[test]
fn unknown_flag_is_read_as_a_value_or_ignored_elsewhere() {
    let dir = sandbox("unknown-flag-ignore");
    // info: the first argument is the file, later ones are ignored.
    let info = invoke(&dir, &["info", "in.wav", "--bogus"]);
    assert_eq!((info.code, info.stderr.as_str()), (0, ""));
    assert!(info.stdout.contains("\"frames\": 200"), "{}", info.stdout);
    assert_fails(&invoke(&dir, &["info", "--bogus"]), &format!("--bogus: {}", not_found()));
    // commands: the first argument is a filter; a later one is ignored.
    assert_silent_success(&invoke(&dir, &["commands", "--bogus"]));
    assert_eq!(invoke(&dir, &["commands", "bounce", "--bogus"]).stdout, invoke(&dir, &["commands", "bounce"]).stdout);
    // describe: the first argument is the id; a later one is ignored.
    assert_fails(&invoke(&dir, &["describe", "--bogus"]), "unknown command `--bogus`");
    let describe = invoke(&dir, &["describe", "file.bounce_mix", "--bogus"]);
    assert_eq!((describe.code, describe.stdout), (0, invoke(&dir, &["describe", "file.bounce_mix"]).stdout));
    // parity and plugins ignore every argument but parity's --write.
    let parity = invoke(&dir, &["parity", "--bogus"]);
    assert_eq!((parity.code, parity.stdout, parity.stderr), (0, invoke(&dir, &["parity"]).stdout, String::new()));
    let plugins = invoke(&dir, &["plugins", "--bogus"]);
    assert_eq!((plugins.code, plugins.stdout, plugins.stderr), (0, invoke(&dir, &["plugins"]).stdout, String::new()));
    // app: a `--` word is dropped from the positionals, so the call still goes out and is refused.
    assert_refused(&invoke(&dir, &["app", "--port", "1", "--bogus", "session.inspect"]), 1);
    assert_fails(&invoke(&dir, &["app", "--bogus"]), "app [--port P] METHOD [JSON]");
    // mcp: unknown plain flags are ignored; `--automation-*` flags are rejected.
    assert_silent_success(&invoke(&dir, &["mcp", "--bogus"]));
    assert_fails(&invoke(&dir, &["mcp", "--automation-bogus", "x"]), "unknown mcp option `--automation-bogus`");
    assert_fails(&invoke(&dir, &["mcp", "--automation-bogus=x"]), "unknown mcp option `--automation-bogus=x`");
}

#[test]
fn single_dash_words_are_never_options() {
    let dir = sandbox("single-dash");
    let got = invoke(&dir, &["convert", "in.wav", "out.wav", "-x"]);
    assert_eq!(got.code, 0, "{}", got.stderr);
    assert!(dir.join("out.wav").exists());
    assert_silent_success(&invoke(&dir, &["run", "-x"]));
}

// ---- positionals ----

#[test]
fn missing_and_extra_positionals() {
    let dir = sandbox("positionals");
    assert_fails(&invoke(&dir, &["info"]), "info FILE");
    assert_fails(&invoke(&dir, &["convert"]), "convert IN OUT");
    assert_fails(&invoke(&dir, &["convert", "in.wav"]), "convert IN OUT");
    assert_fails(&invoke(&dir, &["script"]), "script FILE");
    assert_fails(&invoke(&dir, &["describe"]), "describe ID");
    assert_fails(&invoke(&dir, &["app"]), "app [--port P] METHOD [JSON]");
    assert_fails(&invoke(&dir, &["app", "--port", "1"]), "app [--port P] METHOD [JSON]");
    // Extra positionals are ignored by every subcommand that takes any.
    assert_eq!(invoke(&dir, &["convert", "in.wav", "extra.wav", "third"]).code, 0);
    assert!(dir.join("extra.wav").exists());
    assert_eq!(invoke(&dir, &["script", "s.txt", "extra"]).stdout, "track.new: {\"tracks\":[1]}\n");
    assert_eq!(invoke(&dir, &["run", "extra"]).code, 0);
    assert_fails(&invoke(&dir, &["info", "a", "b"]), &format!("a: {}", not_found()));
    assert_silent_success(&invoke(&dir, &["mcp", "extra"]));
    assert_eq!(invoke(&dir, &["parity", "extra"]).code, 0);
    assert_eq!(invoke(&dir, &["plugins", "extra"]).code, 0);
}

#[test]
fn convert_flags_before_the_positionals_are_taken_as_file_names() {
    let dir = sandbox("convert-order");
    // `--rate` is IN, `44100` is OUT: the tool looks for a file called `--rate`.
    assert_fails(&invoke(&dir, &["convert", "--rate", "44100", "in.wav", "out.wav"]), &format!("--rate: {}", not_found()));
    // Flags between IN and OUT: the flag word becomes OUT and the value is dropped.
    let got = invoke(&dir, &["convert", "in.wav", "--rate", "44100", "out.wav"]);
    assert_eq!((got.code, got.stdout.as_str(), got.stderr.as_str()), (0, "wrote --rate (1 ch, 44100 Hz, 0.00 s)\n", ""));
    assert!(dir.join("--rate").exists());
    assert!(!dir.join("out.wav").exists());
}

// ---- value flags ----

#[test]
fn convert_value_flags_take_a_separate_or_inline_value() {
    let dir = sandbox("convert-values");
    assert_eq!(invoke(&dir, &["convert", "in.wav", "c.wav", "--rate", "44100"]).stdout, "wrote c.wav (1 ch, 44100 Hz, 0.00 s)\n");
    assert_eq!(invoke(&dir, &["convert", "in.wav", "d.wav", "--rate=44100"]).stdout, "wrote d.wav (1 ch, 44100 Hz, 0.00 s)\n");
    assert_eq!(rate_of(&dir, "c.wav"), 44_100);
    assert_eq!(rate_of(&dir, "d.wav"), 44_100);
    assert_eq!(invoke(&dir, &["convert", "in.wav", "e.wav", "--bit-depth", "16"]).code, 0);
    assert_eq!(invoke(&dir, &["convert", "in.wav", "f.wav", "--bit-depth=32f"]).code, 0);
    assert_eq!(format_of(&dir, "e.wav"), "Int16");
    assert_eq!(format_of(&dir, "f.wav"), "Float32");
    assert_eq!(invoke(&dir, &["convert", "in.wav", "g.wav"]).code, 0);
    assert_eq!(format_of(&dir, "g.wav"), "Int24");
}

#[test]
fn convert_value_flag_without_a_value_or_with_a_bad_one_fails() {
    let dir = sandbox("convert-bad-values");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--rate"]), "--rate needs a value");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--bit-depth"]), "--bit-depth needs a value");
    // A following `--` word is the next flag, not the value.
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--rate", "--normalize"]), "--rate needs a value");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--rate", "0"]), "--rate: expected a sample rate in Hz, got `0`");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--rate=abc"]), "--rate: expected a sample rate in Hz, got `abc`");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--bit-depth", "8"]), "--bit-depth: expected 16, 24 or 32f, got `8`");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--normalize=1"]), "--normalize takes no value");
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.wav", "--dither", "--no-dither"]), "--dither and --no-dither: choose one");
    assert!(!dir.join("o.wav").exists());
}

#[test]
fn convert_repeated_flags_use_the_first_occurrence_in_either_spelling() {
    let dir = sandbox("convert-repeat");
    assert_eq!(invoke(&dir, &["convert", "in.wav", "a.wav", "--rate", "44100", "--rate", "22050"]).code, 0);
    assert_eq!(rate_of(&dir, "a.wav"), 44_100);
    assert_eq!(invoke(&dir, &["convert", "in.wav", "b.wav", "--rate=22050", "--rate", "44100"]).code, 0);
    assert_eq!(rate_of(&dir, "b.wav"), 22_050);
    assert_eq!(invoke(&dir, &["convert", "in.wav", "c.wav", "--bit-depth", "16", "--bit-depth", "24"]).code, 0);
    assert_eq!(format_of(&dir, "c.wav"), "Int16");
    assert_eq!(invoke(&dir, &["convert", "in.wav", "d.wav", "--normalize", "--normalize"]).code, 0);
}

#[test]
fn convert_input_and_output_errors() {
    let dir = sandbox("convert-io");
    assert_fails(&invoke(&dir, &["convert", "nope.wav", "o.wav"]), &format!("nope.wav: {}", not_found()));
    assert_fails(&invoke(&dir, &["convert", "in.wav", "o.xyz"]), "o.xyz: cannot write `.xyz` files (supported: wav, aif, aiff, flac)");
}

#[test]
fn run_value_flags_take_a_separate_or_inline_value() {
    let dir = sandbox("run-values");
    let tracks = "track.new: {\"tracks\":[1]}\n";
    assert_eq!(invoke(&dir, &["run", "--cmd", "track.new"]).stdout, tracks);
    assert_eq!(invoke(&dir, &["run", "--cmd=track.new"]).stdout, tracks);
    let save = invoke(&dir, &["run", "--save", "s1.scraft"]);
    assert_eq!((save.code, save.stdout.as_str()), (0, "{\"audio_files_written\":0,\"path\":\"s1.scraft\"}\n"));
    let save = invoke(&dir, &["run", "--save=s2.scraft"]);
    assert_eq!((save.code, save.stdout.as_str()), (0, "{\"audio_files_written\":0,\"path\":\"s2.scraft\"}\n"));
    assert!(dir.join("s1.scraft").exists() && dir.join("s2.scraft").exists());
    // --in in both spellings loads the saved session.
    for args in [["run", "--in", "s1.scraft", "--inspect"], ["run", "--in=s1.scraft", "--inspect", ""]] {
        let args: Vec<&str> = args.into_iter().filter(|a| !a.is_empty()).collect();
        let got = invoke(&dir, &args);
        assert_eq!(got.code, 0, "{args:?}: {}", got.stderr);
        assert!(got.stdout.contains("\"dirty\": false"), "{args:?}: {}", got.stdout);
    }
    // --bounce with --start/--end in both spellings.
    let spaced = invoke(&dir, &["run", "--demo", "--bounce", "b1.wav", "--start", "0", "--end", "0.1"]);
    assert_eq!(spaced.code, 0, "{}", spaced.stderr);
    assert!(spaced.stdout.contains("\"path\":\"b1.wav\"") && spaced.stdout.contains("\"seconds\":0.1"), "{}", spaced.stdout);
    let inline = invoke(&dir, &["run", "--demo", "--bounce=b2.wav", "--start=0", "--end=0.1"]);
    assert_eq!(inline.code, 0, "{}", inline.stderr);
    assert!(inline.stdout.contains("\"path\":\"b2.wav\"") && inline.stdout.contains("\"seconds\":0.1"), "{}", inline.stdout);
    assert!(dir.join("b1.wav").exists() && dir.join("b2.wav").exists());
}

#[test]
fn run_and_script_value_flags_without_a_value_fail() {
    let dir = sandbox("run-missing-values");
    for flag in ["--cmd", "--in", "--save", "--bounce", "--start", "--end"] {
        assert_fails(&invoke(&dir, &["run", flag]), &format!("{flag} needs a value"));
    }
    for flag in ["--in", "--save", "--bounce", "--start", "--end"] {
        assert_fails(&invoke(&dir, &["script", "s.txt", flag]), &format!("{flag} needs a value"));
    }
    // A following `--` word is the next flag, not the value.
    assert_fails(&invoke(&dir, &["run", "--bounce", "--inspect"]), "--bounce needs a value");
    assert_fails(&invoke(&dir, &["script", "s.txt", "--demo", "--bounce"]), "--bounce needs a value");
}

#[test]
fn run_boolean_flags_reject_an_inline_value() {
    let dir = sandbox("run-boolean");
    assert_fails(&invoke(&dir, &["run", "--demo=1"]), "--demo takes no value");
    assert_fails(&invoke(&dir, &["run", "--inspect=1"]), "--inspect takes no value");
    assert_fails(&invoke(&dir, &["script", "s.txt", "--keep-going=1"]), "--keep-going takes no value");
}

#[test]
fn run_repeated_flags() {
    let dir = sandbox("run-repeat");
    // --cmd repeats: every occurrence runs, in order.
    assert_eq!(invoke(&dir, &["run", "--cmd", "track.new", "--cmd=track.new"]).stdout, "track.new: {\"tracks\":[1]}\ntrack.new: {\"tracks\":[2]}\n");
    // --save repeats: the first occurrence wins.
    let got = invoke(&dir, &["run", "--save", "a.scraft", "--save", "b.scraft"]);
    assert_eq!(got.stdout, "{\"audio_files_written\":0,\"path\":\"a.scraft\"}\n");
    assert!(dir.join("a.scraft").exists() && !dir.join("b.scraft").exists());
}

#[test]
fn run_demo_and_sample_are_the_same_flag() {
    let dir = sandbox("run-demo");
    let demo = invoke(&dir, &["run", "--demo", "--inspect"]);
    let sample = invoke(&dir, &["run", "--sample", "--inspect"]);
    assert_eq!((demo.code, sample.code), (0, 0));
    assert_eq!(demo.stdout, sample.stdout);
    assert!(demo.stdout.contains("\"name\": \"Verb\""), "{}", demo.stdout);
}

#[test]
fn run_command_failures() {
    let dir = sandbox("run-cmd-fail");
    assert_fails(&invoke(&dir, &["run", "--cmd", "nosuch.cmd"]), "nosuch.cmd: unknown command `nosuch.cmd`");
    assert_fails(&invoke(&dir, &["run", "--cmd", "track.new={"]), "track.new: bad JSON: EOF while parsing an object at line 1 column 1");
    assert_fails(&invoke(&dir, &["run", "--in", "nope.scraft"]), &format!("nope.scraft: {}", not_found()));
    assert_fails(&invoke(&dir, &["run", "--in=nope.scraft"]), &format!("nope.scraft: {}", not_found()));
}

#[test]
fn script_runs_lines_and_takes_options_after_the_file() {
    let dir = sandbox("script");
    let got = invoke(&dir, &["script", "s.txt"]);
    assert_eq!((got.code, got.stdout.as_str(), got.stderr.as_str()), (0, "track.new: {\"tracks\":[1]}\n", ""));
    let got = invoke(&dir, &["script", "s.txt", "--save", "sv.scraft"]);
    assert_eq!(got.stdout, "track.new: {\"tracks\":[1]}\n{\"audio_files_written\":0,\"path\":\"sv.scraft\"}\n");
    assert!(dir.join("sv.scraft").exists());
    assert_eq!(invoke(&dir, &["script", "s.txt", "--keep-going"]).stdout, "track.new: {\"tracks\":[1]}\n");
    // Options before the file name make the option the file name.
    assert_fails(&invoke(&dir, &["script", "--keep-going", "s.txt"]), &format!("--keep-going: {}", not_found()));
    assert_fails(&invoke(&dir, &["script", "nope.txt"]), &format!("nope.txt: {}", not_found()));
    assert_fails(&invoke(&dir, &["script", "s.txt", "--in=nope.scraft"]), &format!("nope.scraft: {}", not_found()));
}

#[test]
fn script_line_errors_stop_unless_keep_going() {
    let dir = sandbox("script-errors");
    std::fs::write(dir.join("bad.txt"), "nosuch.cmd {}\ntrack.new {}\n").unwrap();
    std::fs::write(dir.join("badjson.txt"), "track.new {x\n").unwrap();
    // The line error goes to stderr without the `soundcraft-cli:` prefix.
    let stop = invoke(&dir, &["script", "bad.txt"]);
    assert_eq!((stop.code, stop.stdout.as_str()), (1, ""));
    assert_eq!(stop.stderr, "bad.txt:1: nosuch.cmd: unknown command `nosuch.cmd`\n");
    let keep = invoke(&dir, &["script", "bad.txt", "--keep-going"]);
    assert_eq!((keep.code, keep.stdout.as_str()), (0, "track.new: {\"tracks\":[1]}\n"));
    assert_eq!(keep.stderr, "bad.txt:1: nosuch.cmd: unknown command `nosuch.cmd`\n");
    assert_fails(&invoke(&dir, &["script", "badjson.txt"]), "badjson.txt:1: bad JSON: key must be a string at line 1 column 2");
}

#[test]
fn commands_filters_by_id_or_label_and_uses_only_the_first_argument() {
    let dir = sandbox("commands");
    let all = invoke(&dir, &["commands"]);
    assert_eq!((all.code, all.stderr.as_str()), (0, ""));
    assert!(all.stdout.starts_with("session.new "), "{}", all.stdout);
    let bounce = invoke(&dir, &["commands", "bounce"]);
    assert!(bounce.stdout.contains("file.bounce_mix ") && !bounce.stdout.contains("session.new"), "{}", bounce.stdout);
    assert_eq!(invoke(&dir, &["commands", "bounce", "extra"]).stdout, bounce.stdout);
    // Matching is case-insensitive on the label only: an upper-case filter finds the labels, not the ids.
    assert!(invoke(&dir, &["commands", "BOUNCE"]).stdout.contains("Bounce Mix..."));
    assert_silent_success(&invoke(&dir, &["commands", "zzzzqq"]));
}

#[test]
fn describe_prints_json_for_a_known_id_and_fails_for_an_unknown_one() {
    let dir = sandbox("describe");
    let got = invoke(&dir, &["describe", "file.bounce_mix"]);
    assert_eq!((got.code, got.stderr.as_str()), (0, ""));
    assert!(got.stdout.starts_with("{\n  \"id\": \"file.bounce_mix\",\n  \"label\": \"Bounce Mix...\""), "{}", got.stdout);
    assert_fails(&invoke(&dir, &["describe", "nosuch"]), "unknown command `nosuch`");
}

#[test]
fn info_prints_json_for_audio_and_fails_for_missing_files() {
    let dir = sandbox("info");
    let got = invoke(&dir, &["info", "in.wav"]);
    assert_eq!((got.code, got.stderr.as_str()), (0, ""));
    assert!(
        got.stdout.contains("\"channels\": 1")
            && got.stdout.contains("\"sample_rate\": 48000")
            && got.stdout.contains("\"sample_format\": \"Int16\""),
        "{}",
        got.stdout
    );
    assert_fails(&invoke(&dir, &["info", "nope.wav"]), &format!("nope.wav: {}", not_found()));
}

#[test]
fn parity_writes_a_report_with_write_in_both_spellings() {
    let dir = sandbox("parity");
    let stdout_report = invoke(&dir, &["parity"]);
    assert_eq!(stdout_report.code, 0);
    assert!(stdout_report.stdout.starts_with("# Parity report\n"), "{}", stdout_report.stdout);
    for (flag, name) in [(vec!["--write", "p1.md"], "p1.md"), (vec!["--write=p2.md"], "p2.md")] {
        let mut args = vec!["parity"];
        args.extend(flag);
        let got = invoke(&dir, &args);
        assert_eq!(got.code, 0, "{}", got.stderr);
        assert!(got.stdout.starts_with(&format!("wrote {name}: ")), "{}", got.stdout);
        assert!(std::fs::read_to_string(dir.join(name)).unwrap().starts_with("# Parity report\n"));
    }
    // `--write` without a value reads as absent: the report goes to stdout.
    let bare = invoke(&dir, &["parity", "--write"]);
    assert_eq!((bare.code, bare.stdout, bare.stderr), (0, stdout_report.stdout, String::new()));
}

#[test]
fn plugins_lists_one_line_per_plugin() {
    let got = invoke(&sandbox("plugins"), &["plugins"]);
    assert_eq!((got.code, got.stderr.as_str()), (0, ""));
    assert!(got.stdout.starts_with("eq_1band             EQ 1-Band              Eq\n"), "{}", got.stdout);
}

// ---- app ----

#[test]
fn app_reports_a_refused_connection_and_checks_arguments_first() {
    let dir = sandbox("app");
    for args in [
        vec!["app", "--port", "1", "session.inspect"],
        vec!["app", "--port=1", "session.inspect"],
        vec!["app", "session.inspect", "--port", "1"],
        vec!["app", "--port", "1", "session.inspect", "{}", "extra"],
    ] {
        assert_refused(&invoke(&dir, &args), 1);
    }
    assert_fails(
        &invoke(&dir, &["app", "--port", "1", "session.inspect", "{bad"]),
        "session.inspect: bad JSON: key must be a string at line 1 column 2",
    );
    assert_fails(&invoke(&dir, &["app", "--port", "abc", "session.inspect"]), "127.0.0.1:abc: invalid port value");
}

#[test]
fn app_port_comes_from_the_flag_then_the_environment() {
    let dir = sandbox("app-env");
    assert_refused(&invoke_env(&dir, &["app", "session.inspect"], Some("1")), 1);
    assert_refused(&invoke_env(&dir, &["app", "--port", "2", "session.inspect"], Some("1")), 2);
}

// ---- mcp ----

#[test]
fn mcp_argument_errors_fail_before_reading_stdin() {
    let dir = sandbox("mcp-args");
    assert_fails(&invoke(&dir, &["mcp", "--connect"]), "--connect needs a port or host:port");
    assert_fails(&invoke(&dir, &["mcp", "--automation-read-root"]), "--automation-read-root needs a folder");
    assert_fails(
        &invoke(&dir, &["mcp", "--automation-read-root", "--demo"]),
        "--automation-read-root needs a folder; use --automation-read-root=<dir> for a folder starting with '-'",
    );
    assert_fails(
        &invoke(&dir, &["mcp", "--automation-read-root", "missing-dir"]),
        &format!("cannot open the automation read root `missing-dir`: {}", not_found()),
    );
    assert_fails(
        &invoke(&dir, &["mcp", "--automation-read-root", ".", "--connect", "1"]),
        "--automation-read-root and --automation-write-root confine the in-process server; a running app is not confined by them, so drop --connect (or use --connect without the roots)",
    );
}

#[test]
fn mcp_accepts_its_flags_in_both_spellings_and_exits_cleanly_on_empty_stdin() {
    let dir = sandbox("mcp-ok");
    for args in [vec!["mcp"], vec!["mcp", "--demo"], vec!["mcp", "--connect", "1"], vec!["mcp", "--connect=1"], vec!["mcp", "--demo=1"]] {
        assert_silent_success(&invoke(&dir, &args));
    }
    // A root prints the confinement line to stderr (never stdout).
    for args in [&["mcp", "--automation-read-root", "."][..], &["mcp", "--automation-read-root=."][..]] {
        let got = invoke(&dir, args);
        assert_eq!((got.code, got.stdout.as_str()), (0, ""));
        assert!(got.stderr.starts_with("soundcraft-cli: files confined: read ") && got.stderr.ends_with("; write none\n"), "{}", got.stderr);
    }
    // A repeated write-root flag is accepted in either spelling.
    let twice = invoke(&dir, &["mcp", "--automation-write-root=.", "--automation-write-root", "."]);
    assert_eq!(twice.code, 0);
    assert!(
        twice.stderr.starts_with("soundcraft-cli: files confined: read none; write ") && !twice.stderr.contains("; write none"),
        "{}",
        twice.stderr
    );
}
