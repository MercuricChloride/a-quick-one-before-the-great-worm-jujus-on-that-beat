use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    sync::{mpsc::channel, Mutex},
    thread,
};

use eframe::{
    egui::{self, ComboBox, Key, Ui, Widget, Window},
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

    pub fn inputs_mut(&mut self) -> &mut Vec<String> {
        match self {
            Module::Map { inputs, .. } => inputs,
            Module::Store { inputs, .. } => inputs,
        }
    }

    fn generate_input_code(input: &str, module_map: &HashMap<String, Module>) -> String {
        match module_map.get(input) {
            Some(Module::Map {
                name,
                code,
                inputs,
                editing,
            }) => {
                format!("#{{kind: \"map\", name: \"{name}\"}}")
            }
            Some(Module::Store {
                name,
                code,
                inputs,
                update_policy,
                editing,
            }) => {
                format!("#{{kind: \"store\", name: \"{name}\"}}")
            }
            None => {
                if input == "$BLOCK" {
                    "#{kind: \"source\"}".to_string()
                } else {
                    panic!("Unknown input: {}", input)
                }
            }
        }
    }

    fn register_module(&self, module_map: &HashMap<String, Module>) -> String {
        let name = self.name();
        let register_function = match self {
            Module::Map { .. } => "add_mfn",
            Module::Store { .. } => "add_sfn",
        };

        let input_code = self
            .inputs()
            .iter()
            .map(|input| Self::generate_input_code(input, module_map))
            .collect::<Vec<String>>()
            .join(",");

        let code = format!(
            r#"
{register_function}(#{{
    name: "{name}",
    inputs: [{input_code}],
    handler: "{name}"
}});
"#
        );

        code
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
            template_repo_path: "/home/alexandergusev/streamline/streamline-template-repository/"
                .to_string(),
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

    pub fn source_file(&self) -> String {
        let modules = self.modules.lock().unwrap();
        let mut source = String::new();
        for module in modules.values() {
            source.push_str(&module.register_module(&modules));
            source.push_str(module.code());
            source.push_str("\n");
        }

        source
    }
}

pub fn rust_view_ui(ui: &mut egui::Ui, code: &str) {
    let language = "rs";
    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
}

impl eframe::App for EditorState {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let source_file = self.source_file();

        let Self {
            template_repo_path,
            rhai_engine,
            rhai_scope,
            config,
            messages,
            modules,
        } = self;

        let menu_bar = |ui: &mut Ui| {
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

            if config.show_full_source {
                Window::new("Full Source").show(ctx, |ui| rust_view_ui(ui, &source_file));
            }

            ui.horizontal(|ui| {
                if ui.button("Run in repl").clicked() {
                    let result = rhai_engine.eval::<Dynamic>(&source_file);
                    let mut messages = messages.lock().unwrap();
                    messages.push(format!("Result: {:?}", result));
                }

                if ui.button("Build").clicked() {
                    let result = rhai_engine.eval::<Dynamic>("codegen()").unwrap();
                    let mut messages = messages.lock().unwrap();
                    messages.push(format!("Build result: {:?}", result));
                }

                if ui.button("Write to temp").clicked() {
                    let path = "/tmp/streamline_ide_output.rhai";
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
                    let module_names = modules
                        .iter()
                        .map(|(k, v)| v.name().to_string())
                        .collect::<Vec<_>>();
                    for (name, module) in modules.iter_mut() {
                        // TODO Fix this lazy clone
                        ui.collapsing(name.as_str(), |ui| {
                            ComboBox::from_label("Input")
                                .selected_text("Select")
                                .show_ui(ui, |ui| {
                                    for input in module.inputs_mut() {
                                        for module_name in module_names.iter() {
                                            if &module_name == &input {
                                                continue;
                                            }
                                            ui.selectable_value(
                                                input,
                                                module_name.to_string(),
                                                module_name,
                                            );
                                        }
                                    }
                                });
                            ui.checkbox(module.editing_mut(), "Show Editor?");
                            if *module.editing() {
                                Window::new(name).show(ctx, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            if ui.button("Eval").clicked()
                                                || ctx.input(|i| {
                                                    i.key_pressed(Key::Enter) && i.modifiers.ctrl
                                                })
                                            {
                                                let result =
                                                    rhai_engine.eval::<Dynamic>(module.code());
                                                let mut messages = messages.lock().unwrap();
                                                messages.push(format!("Result: {:?}", result));
                                            }
                                        });
                                        ui.code_editor(module.code_mut());
                                    });
                                });
                            }
                        });
                        ui.end_row();
                    }

                    ui.horizontal(|ui| {
                        if ui.button("Add Mfn").clicked() {
                            let name = "template_mfn";
                            modules.insert(
                                name.to_string(),
                                Module::Map {
                                    name: name.to_string(),
                                    code: format!("fn {name}($BLOCK) {{ block.number }}"),
                                    inputs: vec!["$BLOCK".to_string()],
                                    editing: true,
                                },
                            );
                        }
                        if ui.button("Add SFN").clicked() {
                            let name = "template_sfn";
                            modules.insert(
                                name.to_string(),
                                Module::Store {
                                    name: name.to_string(),
                                    code: format!("fn {name}(test_map,s) {{ s.set(test_map); }}"),
                                    inputs: vec!["test_map".to_string()],
                                    update_policy: "set".to_string(),
                                    editing: true,
                                },
                            );
                        }
                    })
                });
        };

        let message_panel = |ui: &mut Ui| {
            ui.vertical(|ui| {
                ui.heading("Messages");
                ui.horizontal(|ui| {
                    if ui.button("Clear Messages").clicked() {
                        let mut messages = messages.lock().unwrap();
                        messages.clear();
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    let messages = messages.lock().unwrap();
                    for message in messages.iter() {
                        ui.label(message);
                    }
                });
            });
        };

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

            menu_bar(ui);
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
