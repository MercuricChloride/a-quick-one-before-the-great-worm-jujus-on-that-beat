use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    sync::{mpsc::channel, Mutex},
    thread,
};

use eframe::{
    egui::{self, Key, Ui, Widget, Window},
    run_native, AppCreator, NativeOptions,
};
use rhai::{eval, Dynamic, Engine, Scope};
use serde::{Deserialize, Serialize};

/// Config for the editor
#[derive(Default, Serialize, Deserialize)]
pub struct EditorConfig {
    show_config: bool,
    show_full_source: bool,
}

#[derive(Serialize, Deserialize)]
pub enum Module {
    Map {
        name: String,
        code: String,
        inputs: Vec<String>,
        editing: bool,
    },
    Store {
        name: String,
        code: String,
        inputs: Vec<String>,
        update_policy: String,
        editing: bool,
    },
}

impl Module {
    pub fn name(&self) -> &str {
        match self {
            Module::Map { name, .. } => name,
            Module::Store { name, .. } => name,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Module::Map { code, .. } => code,
            Module::Store { code, .. } => code,
        }
    }

    pub fn code_mut(&mut self) -> &mut String {
        match self {
            Module::Map { code, .. } => code,
            Module::Store { code, .. } => code,
        }
    }

    pub fn editing(&self) -> &bool {
        match self {
            Module::Map { editing, .. } => editing,
            Module::Store { editing, .. } => editing,
        }
    }

    pub fn editing_mut(&mut self) -> &mut bool {
        match self {
            Module::Map { editing, .. } => editing,
            Module::Store { editing, .. } => editing,
        }
    }

    pub fn inputs(&self) -> &Vec<String> {
        match self {
            Module::Map { inputs, .. } => &inputs,
            Module::Store { inputs, .. } => &inputs,
        }
    }

    fn default() -> HashMap<String, Self> {
        let mut map = HashMap::new();
        map.insert(
            "test_map".to_string(),
            Module::Map {
                name: "test_map".to_string(),
                code: "fn test_map($BLOCK) { block.number }".to_string(),
                inputs: vec!["$BLOCK".to_string()],
                editing: true,
            },
        );

        map.insert(
            "test_store".to_string(),
            Module::Store {
                name: "store".to_string(),
                code: "fn test_store(test_map,s) { s.set(test_map); }".to_string(),
                inputs: vec!["test_map".to_string()],
                update_policy: "set".to_string(),
                editing: true,
            },
        );
        map
    }
}

/// Egui App State
#[derive(Serialize, Deserialize)]
pub struct EditorState {
    template_repo_path: String,
    source_file: String,
    #[serde(skip)]
    rhai_engine: Engine,
    #[serde(skip)]
    rhai_scope: Scope<'static>,
    config: EditorConfig,
    messages: Mutex<Vec<String>>,
    modules: Mutex<HashMap<String, Module>>,
}

impl Default for EditorState {
    fn default() -> Self {
        let engine = Engine::new_raw();
        let scope = Scope::new();
        let (engine, scope) = rhai::packages::streamline::init_package(engine, scope);

        Self {
            template_repo_path: "/path/to/template/repo".to_string(),
            source_file: "".to_string(),
            rhai_engine: engine,
            rhai_scope: scope,
            config: EditorConfig::default(),
            messages: Vec::new().into(),
            modules: Module::default().into(),
        }
    }
}

impl EditorState {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        //let mut engine = Engine::new();
        // Bootstrap the engine with the rhai_egui module
        //init_engine(&mut engine);

        // try to recover the state from storage

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        #[cfg(not(feature = "dev"))]
        if let Some(storage) = cc.storage {
            let engine = Engine::new_raw();
            let scope = Scope::new();
            let (engine, scope) = rhai::packages::streamline::init_package(engine, scope);
            let mut state: Self = eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
            state.rhai_engine = engine;
            state.rhai_scope = scope;
            return state;
        }

        // If we failed to recover the state, return a default state
        Self::default()
    }
}

impl eframe::App for EditorState {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let Self {
            template_repo_path,
            source_file,
            rhai_engine,
            rhai_scope,
            config,
            messages,
            modules,
        } = self;

        let mut menu_bar = |ui: &mut Ui, source_file: &str| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Config", |ui| {
                    ui.checkbox(&mut config.show_config, "Open Config Panel");
                    ui.checkbox(&mut config.show_full_source, "Show Full Source");
                });
            });

            if config.show_config {
                Window::new("Config").show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Template Repository Path");
                        ui.text_edit_singleline(template_repo_path);
                    })
                });
            }

            ui.horizontal(|ui| {
                if ui.button("Clear Messages").clicked() {
                    let mut messages = messages.lock().unwrap();
                    messages.clear();
                }

                if ui.button("Run in repl").clicked() {
                    let result = rhai_engine.eval::<Dynamic>(source_file);
                    let mut messages = messages.lock().unwrap();
                    messages.push(format!("Result: {:?}", result));
                }

                if ui.button("Build").clicked() {
                    let result = rhai_engine.eval::<Dynamic>("codegen()").unwrap();
                    let mut messages = messages.lock().unwrap();
                    messages.push(format!("Build result: {:?}", result));
                }

                if ui.button("Write to template repo").clicked() {
                    let path = format!("{}/streamline_ide_output.rhai", template_repo_path);
                    fs::write(path, source_file).unwrap();
                }
            })
        };

        let module_panel = |ui: &mut Ui| {
            egui::Grid::new("Modules")
                .num_columns(1)
                .spacing([40.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    //ui.heading("Modules");
                    //ui.separator();
                    let mut modules = modules.lock().unwrap();
                    for (name, module) in modules.iter_mut() {
                        // TODO Fix this lazy clone
                        ui.collapsing(name.as_str(), |ui| {
                            ui.collapsing("Inputs", |ui| {
                                for input in module.inputs() {
                                    ui.label(input);
                                }
                            });
                            ui.checkbox(module.editing_mut(), "Show Editor?");
                            if *module.editing() {
                                Window::new(name).show(ctx, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            if ui.button("Eval").clicked() {
                                                let result =
                                                    rhai_engine.eval::<Dynamic>(module.code());
                                                let mut messages = messages.lock().unwrap();
                                                messages.push(format!("Result: {:?}", result));
                                            }
                                        });
                                        ui.text_edit_multiline(module.code_mut());
                                    });
                                });
                            }
                        });
                        ui.end_row();
                    }
                });
        };

        let message_panel = |ui: &mut Ui| {
            ui.vertical(|ui| {
                ui.heading("Messages");
                ui.separator();
                ui.vertical(|ui| {
                    let messages = messages.lock().unwrap();
                    for message in messages.iter() {
                        ui.label(message);
                    }
                });
            });
        };

        if ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.ctrl) {
            let result = rhai_engine.eval_with_scope::<Dynamic>(rhai_scope, source_file);
            let mut messages = messages.lock().unwrap();
            messages.push(format!("Result: {:?}", result));
        }

        egui::SidePanel::left("Modules")
            .max_width(250.0)
            .show(ctx, |ui| {
                module_panel(ui);
            });

        egui::SidePanel::right("Messages").show(ctx, |ui| {
            message_panel(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");

            menu_bar(ui, &source_file);

            Window::new("A sample module")
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.text_edit_multiline(source_file);
                });
        });
    }
}

// pub fn init_engine(engine: &mut Engine) -> &mut Engine {
//     engine.register_fn("run_gui", |app: AppCreator| {
//         let native_options = NativeOptions::default();
//         run_native(
//             "Rhai Egui",
//             native_options,
//             Box::new(|cc| Box::new(MyEguiApp::new(cc))),
//         )
//         .unwrap();
//     });

//     engine
// }
