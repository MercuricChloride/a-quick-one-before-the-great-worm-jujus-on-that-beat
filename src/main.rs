//use rhai_egui::init_engine;

use eframe::{run_native, HardwareAcceleration, NativeOptions};
use rhai_egui::EditorState;

fn main() {
    let mut native_options = NativeOptions::default();
    native_options.hardware_acceleration = HardwareAcceleration::Off;
    run_native(
        "Substreams Editor",
        native_options,
        Box::new(|cc| Box::new(EditorState::new(cc))),
    )
    .unwrap();
}
