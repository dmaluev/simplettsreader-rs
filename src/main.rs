// Simple TTS Reader is a small clipboard reader
// © 2025-2026 Dmitry Maluev <dmaluev@gmail.com>

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn log_error(e: &dyn std::error::Error) {
    if let Ok(path) = simplettsreader::get_log_path()
        && let Ok(file) = std::fs::File::create(path)
    {
        let _ = simplelog::WriteLogger::init(
            simplelog::LevelFilter::Info,
            simplelog::Config::default(),
            file,
        );
        log::error!("{e}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::panic::set_hook(Box::new(|info| {
        let s = info.payload().downcast_ref::<&str>().unwrap_or(&"PANIC");
        let e: Box<dyn std::error::Error> = (*s).into();
        log_error(&*e);
    }));

    for arg in std::env::args() {
        if arg == "-Embedding" {
            if let Err(e) = simplettsreader::open_recordings_path() {
                log_error(&*e);
                return Err(e);
            } else {
                return Ok(());
            }
        }
    }

    let mut pargs = pico_args::Arguments::from_env();

    let text: Option<String> = pargs.opt_value_from_str(["-t", "--text"]).unwrap_or(None);
    let path: Option<String> = pargs.opt_value_from_str(["-f", "--file"]).unwrap_or(None);
    if text.is_some() || path.is_some() {
        if let Err(e) = simplettsreader::run_simple(text, path) {
            log_error(&*e);
            return Err(e);
        } else {
            return Ok(());
        }
    }

    let instance_name = "SimpleTTSReader-{85CBCC28-E397-4fcd-802E-100BE5F064A2}";
    let instance = single_instance::SingleInstance::new(instance_name)?;
    if !instance.is_single() {
        return Ok(());
    }

    let hidden = pargs.opt_value_from_str("--hidden").unwrap_or(Some(true));
    let chunked = pargs.opt_value_from_str("--chunked").unwrap_or(Some(true));
    if let Err(e) = simplettsreader::run(hidden, chunked) {
        log_error(&*e);
        Err(e)
    } else {
        Ok(())
    }
}
