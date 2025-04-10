#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let instance_name = "SimpleTTSReader-{85CBCC28-E397-4fcd-802E-100BE5F064A2}";
    let instance = single_instance::SingleInstance::new(instance_name).unwrap();
    if !instance.is_single() {
        return;
    }

    let mut pargs = pico_args::Arguments::from_env();
    let hidden: Option<bool>;
    if let Ok(value) = pargs.opt_value_from_str("--hidden") {
        if let Some(value) = value {
            hidden = Some(value);
        } else {
            hidden = None;
        }
    } else {
        hidden = Some(true);
    }

    if let Err(e) = simplettsreader::run(hidden) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
