fn main() -> std::process::ExitCode {
    restore_sigpipe();
    currcalc::cli::main()
}

/// Rust ignores SIGPIPE, which turns `currcalc list | head` into a panic.
/// Let the process die quietly like other Unix tools instead.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: resetting a signal disposition is async-signal-safe and the
    // default handler is valid for SIGPIPE.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}
