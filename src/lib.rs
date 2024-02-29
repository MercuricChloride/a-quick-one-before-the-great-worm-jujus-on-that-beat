use std::{
    collections::HashMap,
    fs,
    sync::{mpsc, Arc, Mutex, RwLock},
    thread,
};

use eframe::{
    egui::{self, ComboBox, Frame, Key, Ui, Widget, Window},
    run_native, AppCreator, NativeOptions,
};
use rhai::{eval, Dynamic, Engine, Scope};
use serde::{Deserialize, Serialize};

mod widgets;

use widgets::{module_panel::ModulePanel, *};

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
                if input == "BLOCK" {
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
                code: "fn test_map(BLOCK) {\n block.number \n}".to_string(),
                inputs: vec!["BLOCK".to_string()],
                editing: true,
            },
        );

        map.insert(
            "test_store".to_string(),
            Module::Store {
                name: "test_store".to_string(),
                code: "fn test_store(test_map,s) {\n s.set(test_map); \n}".to_string(),
                inputs: vec!["test_map".to_string()],
                update_policy: "set".to_string(),
                editing: true,
            },
        );
        map
    }
}

/// Messages that can be sent to the worker thread
#[derive(Serialize, Deserialize)]
pub enum WorkerMessage {
    Eval(String),
    Reset,
    Build,
}

/// Messages that can be sent to the gui thread
pub enum GuiMessage {
    PushMessage(String),
    ClearMessages,
    //WriteToTemp,
}

/// Egui App State
#[derive(Default, Serialize, Deserialize)]
pub struct EditorState {
    template_repo_path: String,
    // #[serde(skip)]
    // rhai_engine: Arc<RwLock<Engine>>,
    // #[serde(skip)]
    // rhai_scope: Arc<RwLock<Scope<'static>>>,
    config: EditorConfig,
    messages: Vec<String>,
    modules: Arc<RwLock<HashMap<String, Module>>>,
    display_welcome_message: bool,

    #[serde(skip)]
    gui_receiver: Option<mpsc::Receiver<GuiMessage>>,
    #[serde(skip)]
    gui_sender: Option<mpsc::Sender<GuiMessage>>,

    #[serde(skip)]
    worker_sender: Option<mpsc::Sender<WorkerMessage>>,
    #[serde(skip)]
    message_receiver: Option<mpsc::Receiver<WorkerMessage>>,
}

impl EditorState {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut state;

        #[cfg(not(feature = "dev"))]
        if let Some(storage) = cc.storage {
            state = eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        } else {
            state = Self::default();
        };

        #[cfg(feature = "dev")]
        {
            state = Self::default();
        }

        // Channel from: gui -> worker thread
        let (worker_send, worker_rec) = mpsc::channel();

        // Channel from: worker -> gui
        let (gui_send, gui_rec) = mpsc::channel();

        // store the sender in the state so we can send messages to the worker thread
        state.worker_sender = Some(worker_send);
        state.gui_receiver = Some(gui_rec);
        state.gui_sender = Some(gui_send.clone());
        state.modules = Arc::new(RwLock::new(Module::default()));

        thread::spawn(move || {
            let engine = Engine::new_raw();
            let scope = Scope::new();
            let (engine, mut scope) = rhai::packages::streamline::init_package(engine, scope);

            loop {
                while let Ok(msg) = worker_rec.try_recv() {
                    match msg {
                        WorkerMessage::Eval(code) => {
                            let result = engine.eval_with_scope::<Dynamic>(&mut scope, &code);
                            let message = format!("Result: {:?}", result);
                            gui_send.send(GuiMessage::PushMessage(message)).unwrap()
                        }
                        WorkerMessage::Reset => {
                            gui_send.send(GuiMessage::ClearMessages).unwrap();
                            scope.clear();
                        }
                        WorkerMessage::Build => {
                            let result = engine.eval_with_scope::<Dynamic>(&mut scope, "codegen()");
                            gui_send
                                .send(GuiMessage::PushMessage(format!("Build Log: {:?}", result)))
                                .unwrap();
                        }
                    };
                }
            }
        });

        state
    }

    pub fn source_file(&self) -> String {
        let modules = self.modules.read().unwrap();
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
            config,
            messages,
            modules,
            display_welcome_message,
            worker_sender,
            gui_receiver,
            gui_sender,
            ..
        } = self;

        let worker_sender = worker_sender.as_ref().unwrap();
        let gui_receiver = gui_receiver.as_mut().unwrap();
        let gui_sender = gui_sender.as_ref().unwrap();

        while let Ok(msg) = gui_receiver.try_recv() {
            match msg {
                GuiMessage::PushMessage(msg) => messages.push(msg),
                GuiMessage::ClearMessages => messages.clear(),
            }
        }

        let mut menu_bar = |ui: &mut Ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Config", |ui| {
                    ui.checkbox(&mut config.show_config, "Open Config Panel");
                    ui.checkbox(&mut config.show_full_source, "Show Full Source");
                });

                ui.menu_button("Run", |ui| {
                    if ui.button("Run in repl").clicked() {
                        //let message = WorkerMessage::Eval(source_file);
                        //worker_sender.send(message).unwrap();
                    }

                    if ui.button("Build").clicked() {
                        let message = WorkerMessage::Build;
                        worker_sender.send(message).unwrap();
                    }
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
                let style = egui::Style::default();
                Frame::central_panel(&style).show(ui, |ui| {
                    Window::new("Full Source").show(ctx, |ui| rust_view_ui(ui, &source_file));
                });
            }
        };

        let message_panel = |ui: &mut Ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Messages");
                ui.horizontal(|ui| {
                    if ui.button("Clear Messages").clicked() {
                        gui_sender.send(GuiMessage::ClearMessages).unwrap();
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    for message in messages.iter() {
                        ui.label(message);
                    }
                });
            })
        };

        if *display_welcome_message {
            egui::CentralPanel::default()
                .show(ctx, |ui| {
                    ui.vertical_centered_justified(|ui| {
                        ui.set_width(500.0);
                        ui.heading("Welcome to Streamline!");
                        ui.separator();
                        ui.label("Hello there and welcome to the Streamline IDE! We are so happy to have you here!");
                        ui.separator();
                        ui.label("This editor is an active work in progress, so please report any bugs and weird things you find.");
                        ui.label("Additionally, if you have any features you would like to see, please let me, @blind_nabler know!");
                        ui.separator();
                        ui.label("K thx bye :)");
                        ui.label("PS: I love you");
                        ui.separator();
                        if ui.button("Close this message").clicked() {
                            *display_welcome_message = false;
                        }
                    });
                });
            return ();
        }

        let modules = modules.clone();
        egui::SidePanel::left("Modules")
            .max_width(250.0)
            .show(ctx, |ui| {
                let channel = worker_sender.clone();
                let view = ModulePanel::new(ctx, channel, modules);
                ui.add(view)
            });

        egui::SidePanel::right("Messages").show(ctx, |ui| {
            message_panel(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            menu_bar(ui);
        });
    }
}
