/// Print without panicking on a closed pipe. A write error must not skip later work
/// such as `--bounce` or `--save`, and must not by itself report success.
macro_rules! outln {
    ($($t:tt)*) => {{
        let mut o = std::io::stdout().lock();
        let _ = writeln!(o, $($t)*);
    }};
}
