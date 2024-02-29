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

pub mod modules;
mod widgets;

use modules::Module;
use substreams_sink_rust_lib::{start_stream, start_stream_channel, StreamConfig};
use tokio::runtime::Runtime;
use widgets::{module_panel::ModulePanel, *};

/// Config for the editor
#[derive(Default, Serialize, Deserialize)]
pub struct EditorConfig {
    show_config: bool,
    show_full_source: bool,
    show_null_json: bool,
    stream_start_block: i64,
    stream_stop_block: u64,
}
/// Messages that can be sent to the worker thread
pub enum WorkerMessage {
    Eval(String),
    Reset,
    Build,
}

/// Messages that can be sent to the gui thread
pub enum GuiMessage {
    PushMessage(String),
    PushJson(String),
    ClearMessages,
    //WriteToTemp,
}

pub enum StreamMessages {
    Run {
        start: i64,
        stop: u64,
        api_key: String,
    },
}

#[derive(Serialize, Deserialize)]
pub enum MessageKind {
    JsonMessage(String),
    TextMessage(String),
    //ErrorMessage(String),
}

/// Egui App State
#[derive(Default, Serialize, Deserialize)]
pub struct EditorState {
    template_repo_path: String,
    substreams_api_key: String,
    // #[serde(skip)]
    // rhai_engine: Arc<RwLock<Engine>>,
    // #[serde(skip)]
    // rhai_scope: Arc<RwLock<Scope<'static>>>,
    config: EditorConfig,
    messages: Vec<MessageKind>,
    modules: Arc<RwLock<HashMap<String, Module>>>,
    display_welcome_message: bool,

    #[serde(skip)]
    gui_receiver: Option<mpsc::Receiver<GuiMessage>>,
    #[serde(skip)]
    gui_sender: Option<mpsc::Sender<GuiMessage>>,

    #[serde(skip)]
    stream_sender: Option<mpsc::Sender<StreamMessages>>,
    #[serde(skip)]
    stream_receiver: Option<mpsc::Receiver<StreamMessages>>,

    #[serde(skip)]
    worker_sender: Option<mpsc::Sender<WorkerMessage>>,
    #[serde(skip)]
    message_receiver: Option<mpsc::Receiver<WorkerMessage>>,
}

impl EditorState {
    pub fn new(cc: &eframe::CreationContext<'_>, api_key: Option<String>) -> Self {
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

        // Channel from: gui -> stream thread
        let (stream_send, stream_rec) = mpsc::channel();

        // store the sender in the state so we can send messages to the worker thread
        state.worker_sender = Some(worker_send);
        state.gui_receiver = Some(gui_rec);
        state.gui_sender = Some(gui_send.clone());
        state.stream_sender = Some(stream_send);
        //state.stream_receiver = Some(stream_rec);
        state.modules = Arc::new(RwLock::new(Module::default()));
        if let Some(api_key) = api_key {
            state.substreams_api_key = api_key;
        }

        let gui_sender = gui_send.clone();
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
                            gui_sender.send(GuiMessage::PushMessage(message)).unwrap()
                        }
                        WorkerMessage::Reset => {
                            gui_sender.send(GuiMessage::ClearMessages).unwrap();
                            scope.clear();
                        }
                        WorkerMessage::Build => {
                            let result = engine.eval_with_scope::<Dynamic>(&mut scope, "codegen()");
                            gui_sender
                                .send(GuiMessage::PushMessage(format!("Build Log: {:?}", result)))
                                .unwrap();
                        }
                    };
                }
            }
        });

        let gui_sender = gui_send.clone();
        thread::spawn(move || {
            let rt = Runtime::new().expect("Unable to create Runtime");
            let _enter = rt.enter();
            rt.block_on(async move {
                loop {
                    while let Ok(msg) = stream_rec.try_recv() {
                        match msg {
                            StreamMessages::Run {
                                start: _,
                                stop: _,
                                api_key,
                            } => {
                                let default_start = 12369621;
                                let default_stop = 12369631;
                                let default_package_file = "https://github.com/streamingfast/substreams-uniswap-v3/releases/download/v0.2.8/substreams.spkg".to_string();
                                let stream_config = StreamConfig {
                                    endpoint_url: "https://mainnet.eth.streamingfast.io:443"
                                        .to_string(),
                                    package_file: default_package_file.to_string(),
                                    module_name: "graph_out".to_string(),
                                    token: Some(api_key),
                                    start: default_start,
                                    stop: default_stop,
                                };

                                let start_message = format!(
                                    "Starting stream from {} to {}",
                                    default_start, default_stop
                                );

                                if let Ok(rx) = start_stream_channel(stream_config).await {
                                    gui_sender
                                        .send(GuiMessage::PushMessage(start_message))
                                        .unwrap();
                                    while let Ok(data) = rx.recv() {
                                        gui_sender.send(GuiMessage::PushJson(data)).unwrap();
                                    }
                                } else {
                                    let message = "Failed to start stream".to_string();
                                    gui_sender.send(GuiMessage::PushMessage(message)).unwrap();
                                }
                            }
                        }
                    }
                }})
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
        let api_key = self.substreams_api_key.clone();
        let show_null_json = &self.config.show_null_json;

        let Self {
            template_repo_path,
            config,
            messages,
            modules,
            display_welcome_message,
            worker_sender,
            gui_receiver,
            gui_sender,
            stream_sender,
            ..
        } = self;

        let stream_sender = stream_sender.as_ref().unwrap();
        let worker_sender = worker_sender.as_ref().unwrap();
        let gui_receiver = gui_receiver.as_mut().unwrap();
        let gui_sender = gui_sender.as_ref().unwrap();

        while let Ok(msg) = gui_receiver.try_recv() {
            match msg {
                GuiMessage::PushMessage(msg) => {
                    let message = MessageKind::TextMessage(msg);
                    messages.push(message);
                }
                GuiMessage::ClearMessages => messages.clear(),
                GuiMessage::PushJson(json_str) => {
                    let message = MessageKind::JsonMessage(json_str);
                    messages.push(message);
                }
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

                    if ui.button("Run a stream").clicked() {
                        let message = StreamMessages::Run {
                            start: 12369621,
                            stop: 12369631,
                            api_key: api_key.to_string(),
                        };
                        stream_sender.send(message).unwrap()
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
                        match message {
                            MessageKind::JsonMessage(msg) => {
                                let value: serde_json::Value = serde_json::from_str(&msg).unwrap();

                                if value.is_null() && !config.show_null_json {
                                    continue;
                                } else {
                                    egui_json_tree::JsonTree::new(&msg, &value).show(ui);
                                }
                            }
                            MessageKind::TextMessage(msg) => {
                                ui.label(msg);
                            }
                        }
                        //ui.label(message);
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
