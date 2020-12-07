use env_logger::fmt::Formatter;
use log::{Level, Record};
use std::{io, io::Write};

/// Checks if stderr is the systemd journal
///
/// See https://www.freedesktop.org/software/systemd/man/systemd.exec.html#%24JOURNAL_STREAM
#[cfg(target_os = "linux")]
pub fn logging_to_journal() -> bool {
    use std::{
        fs::File,
        mem,
        os::{linux::fs::MetadataExt, unix::io::FromRawFd},
    };

    let ab = match std::env::var("JOURNAL_STREAM") {
        Ok(s) => s,
        _ => return false,
    };
    let (l, r) = match ab.find(':') {
        Some(pos) => (&ab[..pos], &ab[pos + 1..]),
        _ => return false,
    };
    let (l, r) = match (l.parse(), r.parse()) {
        (Ok(l), Ok(r)) => (l, r),
        _ => return false,
    };
    let f = unsafe { File::from_raw_fd(2) };
    let metadata = f.metadata();
    mem::forget(f);
    let metadata = match metadata {
        Ok(m) => m,
        Err(_) => return false,
    };
    (metadata.st_dev(), metadata.st_ino()) == (l, r)
}

pub fn configure_logger() {
    std::env::set_var("RUST_LOG", "info");
    let mut b = env_logger::builder();
    #[cfg(target_os = "linux")]
    if logging_to_journal() {
        b.format(formatter);
    }
    b.init();
}

/// Formatter for systemd-journald messages
///
/// See https://www.freedesktop.org/software/systemd/man/sd-daemon.html
fn formatter(f: &mut Formatter, r: &Record) -> io::Result<()> {
    let level = match r.level() {
        Level::Error => "<3>",
        Level::Warn => "<4>",
        Level::Info => "<6>",
        Level::Debug => "<7>",
        Level::Trace => "<7>",
    };
    f.write_all(level.as_bytes())?;
    if let Some(x) = r.module_path() {
        f.write_all(&[b'['])?;
        f.write_all(x.as_bytes())?;
        f.write_all(b"] ")?;
    }
    writeln!(f, "{}", r.args())
}
