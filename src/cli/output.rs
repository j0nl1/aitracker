#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub format: OutputFormat,
    pub pretty: bool,
    pub use_color: bool,
    pub verbose: bool,
}

pub fn detect_color(color_flag: bool) -> bool {
    if !color_flag {
        return false;
    }
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    atty_stdout()
}

fn atty_stdout() -> bool {
    unsafe { libc_isatty(1) != 0 }
}

extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}
