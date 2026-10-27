use super::error::fail;
use soundcraft_automation::{Headless, Remote, mcp::Server};
use soundcraft_engine::Engine;
use soundcraft_engine::file_access::{self, AutomationRoots};
use std::process::ExitCode;

/// What `mcp` was asked for.
struct McpArgs {
    connect: Option<String>,
    demo: bool,
    roots: Option<std::sync::Arc<AutomationRoots>>,
}

/// Parse the `mcp` options up front: a malformed or misspelled root flag must not leave the
/// server running unconfined.
fn parse_mcp_args(args: &[String]) -> Result<McpArgs, String> {
    let mut connect = None;
    let mut demo = false;
    let mut roots_args = crate::automation_args::AutomationArgs::default();
    let mut it = args.iter().cloned();
    while let Some(a) = it.next() {
        if roots_args.consume(&a, &mut it)? {
            continue;
        }
        match a.as_str() {
            "--connect" => connect = Some(it.next().ok_or("--connect needs a port or host:port")?),
            "--demo" => demo = true,
            other if other.starts_with("--automation-") => return Err(format!("unknown mcp option `{other}`")),
            _ => {}
        }
    }
    let read: Vec<&std::path::Path> = roots_args.read_roots.iter().map(std::path::PathBuf::as_path).collect();
    let write: Vec<&std::path::Path> = roots_args.write_roots.iter().map(std::path::PathBuf::as_path).collect();
    let roots = AutomationRoots::new(&read, &write)?;
    if roots.is_some() && connect.is_some() {
        return Err("--automation-read-root and --automation-write-root confine the in-process server; a running app is not confined by them, \
so drop --connect (or use --connect without the roots)"
            .into());
    }
    Ok(McpArgs { connect, demo, roots })
}

pub(crate) fn mcp(args: &[String]) -> ExitCode {
    let McpArgs { connect, demo, roots } = match parse_mcp_args(args) {
        Ok(a) => a,
        Err(err) => return fail(err),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let r = if let Some(port) = connect {
        let mut s = Server::new(Remote::new(&port));
        s.serve(stdin.lock(), stdout.lock())
    } else {
        // `--demo` is the operator's own choice, made before the roots come into force.
        let e = if demo { soundcraft_engine::demo::demo_engine() } else { Engine::default() };
        let mut h = Headless::new(e);
        if let Some(roots) = roots {
            // Every thread of this process, not only the server's: nothing reaches past the roots.
            file_access::confine_process(roots.clone());
            eprintln!("soundcraft-cli: files confined: {}", roots.describe());
            h = h.with_automation_roots(Some(roots));
        }
        let mut s = Server::new(h);
        s.serve(stdin.lock(), stdout.lock())
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}
