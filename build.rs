fn main() {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let resource_header = std::path::Path::new(&out_dir).join("version.h");

    let major = env!("CARGO_PKG_VERSION_MAJOR");
    let minor = env!("CARGO_PKG_VERSION_MINOR");
    let patch = env!("CARGO_PKG_VERSION_PATCH");
    let full = env!("CARGO_PKG_VERSION");
    let description = env!("CARGO_PKG_DESCRIPTION");
    let name = env!("CARGO_PKG_NAME");

    std::fs::write(
        resource_header,
        format!(
            "
#define VERSION_MAJOR {major}
#define VERSION_MINOR {minor}
#define VERSION_PATCH {patch}
#define VERSION_FULL \"{full}\"
#define VERSION_DESCRIPTION  \"{description}\"
#define VERSION_NAME \"{name}\"
"
        ),
    )
    .unwrap();

    let _ = embed_resource::compile("res.rc", embed_resource::NONE);
    slint_build::compile("ui/app-window.slint").expect("Slint build failed");
}
