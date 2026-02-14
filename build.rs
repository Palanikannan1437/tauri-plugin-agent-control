const COMMANDS: &[&str] = &["agent_control_respond"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .build();
    println!("cargo:rerun-if-changed=guest-js/index.js");
}
