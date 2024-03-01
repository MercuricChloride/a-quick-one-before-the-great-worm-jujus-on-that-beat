use eframe::{run_native, HardwareAcceleration, NativeOptions};
use rhai_egui::EditorState;

use dotenv::dotenv;
use std::env;

fn main() {
    dotenv().ok();

    let api_key = env::var("API_KEY").ok();

    let mut native_options = NativeOptions::default();
    //native_options.hardware_acceleration = HardwareAcceleration::Off;

    run_native(
        "Substreams Editor",
        native_options,
        Box::new(|cc| Box::new(EditorState::new(cc, api_key))),
    )
    .unwrap();
}
