fn main() {
    match project_orchestrator_lib::read_live_telemetry() {
        Ok(snapshot) => println!("{}", serde_json::to_string_pretty(&snapshot).unwrap()),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}
