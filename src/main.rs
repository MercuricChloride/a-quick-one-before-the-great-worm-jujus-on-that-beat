//use rhai_egui::init_engine;

use eframe::{egui, run_native, NativeOptions};
use rhai::Engine;
use rhai_egui::EditorState;

fn main() {
    let native_options = NativeOptions::default();
    run_native(
        "Substreams Editor",
        native_options,
        Box::new(|cc| Box::new(EditorState::new(cc))),
    )
    .unwrap();
    // let mut engine = rhai::Engine::new();
    // init_engine(&mut engine);

    // let script = include_str!("../simple_gui.rhai");

    // engine.eval::<()>(script).unwrap();
}
