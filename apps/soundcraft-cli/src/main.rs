//! `soundcraft-cli`: SoundCraft without a window.
//!
//! ```text
//! soundcraft-cli info FILE                       audio file or .scraft summary
//! soundcraft-cli convert IN OUT [--bit-depth 16|24|32f] [--rate HZ] [--normalize] [--dither | --no-dither]
//!                                                 dithers only when the output loses resolution, unless told
//! soundcraft-cli run [--in S.scraft | --demo] [--cmd ID[=JSON]]… [--bounce OUT] [--start T --end T] [--save S.scraft] [--inspect]
//! soundcraft-cli script FILE [--in S | --demo] [--save S] [--bounce OUT [--start T --end T]]   (one `ID JSON` per line;
//!                                                 without --start/--end, --bounce covers the whole session)
//! soundcraft-cli commands [FILTER]               list commands
//! soundcraft-cli describe ID                     one command's details
//! soundcraft-cli app [--port P] METHOD [JSON]     call a running app's control channel
//! soundcraft-cli mcp [--connect PORT] [--demo] [--automation-read-root DIR]... [--automation-write-root DIR]...
//!                                                 MCP server on stdio. The roots confine every file the
//!                                                 agent's commands read (open, import, presets) and write
//!                                                 (save, bounce, export) to those folders, links followed;
//!                                                 each flag may be given more than once, a path inside any
//!                                                 of its folders being allowed, and a kind left out grants
//!                                                 none of its access. They run the in-process server, so
//!                                                 they conflict with --connect.
//! soundcraft-cli parity [--write PATH]            parity report (markdown)
//! soundcraft-cli plugins                          list built-in plugins
//! ```
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod automation_args;
#[macro_use]
mod output;

use std::io::Write;
use std::process::ExitCode;

mod app;
mod args;
mod catalog;
mod convert;
mod error;
mod info;
mod mcp;
mod session;

use app::app;
use catalog::{commands, describe, parity};
use convert::convert;
use error::fail;
use info::info;
use mcp::mcp;
use session::{run, script};

/// The usage block of the module doc comment, up to its closing code fence.
fn help_text() -> String {
    include_str!("main.rs")
        .lines()
        .skip(3)
        .take_while(|l| l.trim_end() != "//! ```")
        .map(|l| l.trim_start_matches("//! "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        eprintln!("usage: soundcraft-cli <info|convert|run|script|commands|describe|app|mcp|parity|plugins> …  (see --help)");
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    let code = match cmd.as_str() {
        "--version" | "-V" | "version" => {
            outln!("SoundCraft CLI {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "--help" | "-h" | "help" => {
            outln!("{}", help_text());
            ExitCode::SUCCESS
        }
        "info" => info(&rest),
        "convert" => convert(&rest),
        "run" => run(&rest),
        "script" => script(&rest),
        "commands" => commands(&rest),
        "describe" => describe(&rest),
        "app" => app(&rest),
        "mcp" => mcp(&rest),
        "parity" => parity(&rest),
        "plugins" => {
            for p in soundcraft_dsp::plugins() {
                outln!("{:<20} {:<22} {:?}{}", p.id, p.name, p.category, if p.is_instrument { " (instrument)" } else { "" });
            }
            ExitCode::SUCCESS
        }
        other => fail(format!("unknown subcommand `{other}`")),
    };
    // The subcommand's engine and its plugin instances are gone by now.
    soundcraft_engine::shutdown_plugin_hosts();
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_text_is_only_the_usage_block() {
        let help = help_text();
        assert!(help.starts_with("soundcraft-cli info FILE"));
        assert!(help.ends_with("soundcraft-cli plugins                          list built-in plugins"));
        assert!(help.contains("--automation-read-root DIR") && help.contains("--automation-write-root DIR"));
        assert!(!help.contains("```"));
        assert!(!help.contains("#![") && !help.contains("use serde_json"));
    }
}
