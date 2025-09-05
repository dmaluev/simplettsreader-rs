// Simple TTS Reader is a small clipboard reader
// © 2025 Dmitry Maluev <dmaluev@gmail.com>

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let instance_name = "SimpleTTSReader-{85CBCC28-E397-4fcd-802E-100BE5F064A2}";
    let instance = single_instance::SingleInstance::new(instance_name).unwrap();
    if !instance.is_single() {
        return;
    }

    let mut pargs = pico_args::Arguments::from_env();
    let hidden = pargs.opt_value_from_str("--hidden").unwrap_or(Some(true));

    if let Err(e) = simplettsreader::run(hidden) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
