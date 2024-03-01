use std::{
    collections::HashMap,
    fs,
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex, RwLock,
    },
    thread,
};

use block_cache::BlockCache;
use eframe::{
    egui::{self, ComboBox, Context, Frame, Key, Ui, Widget, Window},
    run_native, AppCreator, NativeOptions,
};

use rhai::{eval, Dynamic, Engine, EvalAltResult, FuncArgs, OptimizationLevel, Scope, AST};
use serde::{Deserialize, Serialize};

pub mod abis;
pub mod block_cache;
pub mod modules;
pub mod tasks;
mod widgets;

use modules::Module;
use serde_json::Value;
use substreams_sink_rust_lib::{start_stream, start_stream_channel, StreamConfig};
use tasks::{GuiMessage, MessageKind, StreamMessages, WorkerMessage};
use tokio::runtime::Runtime;
use widgets::{module_panel::ModulePanel, panels::rust_view_ui, *};

/// Config for the editor
#[derive(Serialize, Deserialize)]
pub struct EditorConfig {
    module_name: String,
    substream_package: String,
    substream_endpoint: String,
    stream_start_block: i64,
    stream_stop_block: u64,
}

#[derive(Serialize, Deserialize)]
pub struct EditorViews {
    show_config: bool,
    show_full_source: bool,
    show_null_json: bool,
    show_modules: bool,
    show_messages: bool,
    show_user_config: bool,
    show_block_cache: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Spkg {
    pub name: String,
    pub url: String,
}

impl Spkg {
    pub fn uniswap() -> Self {
        Self {
            name: "Uniswap v3".to_string(),
            url: "https://github.com/streamingfast/substreams-uniswap-v3/releases/download/v0.2.8/substreams.spkg".to_string()
        }
    }

    pub fn eth_explorer() -> Self {
        Self {
            name: "Ethereum Explorer".to_string(),
            url: "https://spkg.io/streamingfast/ethereum-explorer-v0.1.2.spkg".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
}

impl Endpoint {
    pub fn sf_mainnet() -> Self {
        Self {
            name: "Streamingfast Mainnet".to_string(),
            url: "https://mainnet.eth.streamingfast.io:443".to_string(),
        }
    }

    pub fn pinax_mainnet() -> Self {
        Self {
            name: "Pinax Mainnet".to_string(),
            url: "https://eth.substreams.pinax.network:443".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct UserConfig {
    substream_list: Vec<Spkg>,
    selected_substream: usize,

    endpoint_list: Vec<Endpoint>,
    selected_endpoint: usize,

    selected_module: String,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            substream_list: vec![Spkg::uniswap(), Spkg::eth_explorer()],
            selected_substream: 0,
            endpoint_list: vec![Endpoint::pinax_mainnet(), Endpoint::sf_mainnet()],
            selected_endpoint: 0,
            selected_module: "graph_out".to_string(),
        }
    }
}

impl Widget for &mut UserConfig {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let UserConfig {
            substream_list,
            selected_substream,
            endpoint_list,
            selected_endpoint,
            selected_module,
        } = self;

        ui.vertical(|ui| {
            ui.heading("User Config");
            ui.separator();
            ui.label("Substream");
            ComboBox::from_label("Substream")
                .selected_text(&substream_list[*selected_substream].name)
                .show_ui(ui, |ui| {
                    for (i, substream) in substream_list.iter().enumerate() {
                        if ui
                            .selectable_label(*selected_substream == i, &substream.name)
                            .clicked()
                        {
                            *selected_substream = i;
                        }
                    }
                });

            ui.label("Endpoint");
            ComboBox::from_label("Endpoint")
                .selected_text(&endpoint_list[*selected_endpoint].name)
                .show_ui(ui, |ui| {
                    for (i, endpoint) in endpoint_list.iter().enumerate() {
                        if ui
                            .selectable_label(*selected_endpoint == i, &endpoint.name)
                            .clicked()
                        {
                            *selected_endpoint = i;
                        }
                    }
                });

            ui.label("Module Name");
            ui.text_edit_singleline(selected_module);
        })
        .response
    }
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            module_name: "graph_out".to_string(),
            substream_endpoint: "https://mainnet.eth.streamingfast.io:443".to_string(),
            // Default to the Uniswap v3 substream package
            substream_package: "https://github.com/streamingfast/substreams-uniswap-v3/releases/download/v0.2.8/substreams.spkg".to_string(),
            // Default to the Uniswap v3 substream package
            stream_start_block: 12369621,
            // Default to +10 blocks
            stream_stop_block: 12369631,
        }
    }
}

impl Default for EditorViews {
    fn default() -> Self {
        Self {
            show_config: false,
            show_full_source: false,
            show_null_json: false,
            show_modules: true,
            show_messages: true,
            show_user_config: false,
            show_block_cache: false,
        }
    }
}

/// Egui App State
#[derive(Default, Serialize, Deserialize)]
pub struct EditorState {
    template_repo_path: String,

    substreams_api_key: String,

    user_config: UserConfig,

    block_cache: BlockCache,

    editor_config: EditorConfig,

    view_config: EditorViews,

    /// A map from abi_name -> abj_json
    abis: HashMap<String, String>,

    messages: Vec<MessageKind>,
    /// The search string for the messages
    message_search: String,

    modules: HashMap<i64, Module>,

    display_welcome_message: bool,

    #[serde(skip)]
    gui_receiver: Option<mpsc::Receiver<GuiMessage>>,
    #[serde(skip)]
    gui_sender: Option<mpsc::Sender<GuiMessage>>,

    #[serde(skip)]
    stream_sender: Option<mpsc::Sender<StreamMessages>>,

    #[serde(skip)]
    worker_sender: Option<mpsc::Sender<WorkerMessage>>,
}

fn build_and_run(
    engine: &mut Engine,
    scope: &mut Scope,
    code: &str,
    main_ast: &mut AST,
) -> Result<Dynamic, Box<EvalAltResult>> {
    engine
        .compile_with_scope(&scope, code)
        .map_err(Into::into)
        .and_then(|r| {
            let ast = engine.optimize_ast(scope, r, OptimizationLevel::Full);

            // Merge the AST into the main
            *main_ast += ast;

            // Evaluate
            engine.eval_ast_with_scope::<Dynamic>(scope, &main_ast)
        })
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

        // sender to the worker thread
        state.worker_sender = Some(worker_send);

        // receiver on the gui thread
        state.gui_receiver = Some(gui_rec);
        // sender to the gui thread
        state.gui_sender = Some(gui_send.clone());

        // sender to the stream thread
        state.stream_sender = Some(stream_send);

        state.modules = Module::build_default_modules();

        let mut abis = HashMap::new();
        abis.insert("erc20".into(), abis::ERC20.to_string());
        abis.insert("erc721".into(), abis::ERC721.to_string());
        state.abis = abis;

        state.display_welcome_message = true;

        if let Some(api_key) = api_key {
            state.substreams_api_key = api_key;
        }

        let gui_sender = gui_send.clone();
        thread::spawn(move || {
            let engine = Engine::new_raw();
            let scope = Scope::new();
            let mut main_ast = AST::empty();
            let (mut engine, mut scope) = rhai::packages::streamline::init_package(engine, scope);
            engine.set_optimization_level(OptimizationLevel::Full);

            // TODO Add support to store outputs of streams to use as input data

            loop {
                while let Ok(msg) = worker_rec.try_recv() {
                    match msg {
                        WorkerMessage::Eval(code) => {
                            let result =
                                build_and_run(&mut engine, &mut scope, &code, &mut main_ast);
                            let message = format!("Result: {:?}", result);
                            gui_sender.send(GuiMessage::PushMessage(message)).unwrap()
                        }
                        WorkerMessage::EvalWithArgs(fn_name, args) => {
                            let args = args
                                .into_iter()
                                .filter_map(|v| serde_json::from_value(v).ok())
                                .collect::<Vec<Dynamic>>();

                            let result: Result<Dynamic, _> =
                                engine.call_fn(&mut scope, &main_ast, &fn_name, args);

                            match result {
                                Ok(result) => {
                                    let result_json_str =
                                        serde_json::to_string_pretty(&result).unwrap();
                                    gui_sender
                                        .send(GuiMessage::PushJson(result_json_str))
                                        .unwrap()
                                }
                                Err(err) => {
                                    let message = format!("Error: {:?}", err);
                                    gui_sender.send(GuiMessage::PushMessage(message)).unwrap()
                                }
                            }
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
                                start,
                                stop,
                                api_key,
                                package_file,
                                endpoint,
                                module_name,
                            } => {
                                let stream_config = StreamConfig {
                                    endpoint_url: endpoint,
                                    package_file,
                                    module_name,
                                    token: Some(api_key),
                                    start,
                                    stop,
                                };

                                let start_message =
                                    format!("Starting stream from {} to {}", start, stop);

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

                                let stop_message = "Stream Completed Successfully".to_string();
                                gui_sender
                                    .send(GuiMessage::PushMessage(stop_message))
                                    .unwrap();
                            }
                            StreamMessages::GetBlock {
                                number,
                                api_key,
                                endpoint,
                                cache_slot,
                            } => {
                                let spkg = Spkg::eth_explorer().url;

                                let stream_config = StreamConfig {
                                    endpoint_url: endpoint,
                                    package_file: spkg,
                                    module_name: "map_block_full".to_string(),
                                    token: Some(api_key),
                                    start: number,
                                    stop: (number + 1) as u64,
                                };

                                let start_message = format!("Getting block {}", number);

                                if let Ok(rx) = start_stream_channel(stream_config).await {
                                    gui_sender
                                        .send(GuiMessage::PushMessage(start_message))
                                        .unwrap();
                                    while let Ok(data) = rx.recv() {
                                        gui_sender
                                            .send(GuiMessage::SetBlock(cache_slot, data))
                                            .unwrap();
                                    }
                                } else {
                                    let message = "Failed to get block".to_string();
                                    gui_sender.send(GuiMessage::PushMessage(message)).unwrap();
                                }
                            }
                        }
                    }
                }
            })
        });

        state
    }

    pub fn source_file(&self) -> String {
        let modules = &self.modules;
        let mut source = String::new();
        for module in modules.values() {
            source.push_str(&module.register_module(&modules));
            source.push_str(module.code());
            source.push_str("\n");
        }

        source
    }
}

impl eframe::App for EditorState {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let source_file = self.source_file();
        let api_key = self.substreams_api_key.clone();
        let endpoint = self.user_config.endpoint_list[self.user_config.selected_endpoint]
            .url
            .clone();

        let Self {
            template_repo_path,
            editor_config,
            view_config,
            messages,
            modules,
            display_welcome_message,
            worker_sender,
            gui_receiver,
            gui_sender,
            stream_sender,
            message_search,
            block_cache,
            ..
        } = self;

        let stream_sender = stream_sender.as_ref().unwrap();
        let worker_sender = worker_sender.as_ref().unwrap();
        let gui_receiver = gui_receiver.as_mut().unwrap();
        let gui_sender = gui_sender.as_ref().unwrap();

        // In the gui thread, we listen for messages from the other threads
        while let Ok(msg) = gui_receiver.try_recv() {
            match msg {
                GuiMessage::PushMessage(msg) => {
                    let message = MessageKind::TextMessage(msg);
                    messages.push(message);
                }
                GuiMessage::ClearMessages => messages.clear(),
                GuiMessage::PushJson(json_str) => {
                    let value = serde_json::from_str(&json_str).unwrap();
                    let message = MessageKind::JsonMessage(value);
                    messages.push(message);
                }
                GuiMessage::SetBlock(cache_slot, json_str) => {
                    let value: Value = serde_json::from_str(&json_str).unwrap();
                    block_cache.set(cache_slot, value.clone());

                    let message = MessageKind::TextMessage(format!("Block {} set", cache_slot));
                    messages.push(message);

                    let message = MessageKind::JsonMessage(value);
                    messages.push(message);
                }
            }
        }

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

        if view_config.show_modules {
            egui::SidePanel::left("Modules")
                .max_width(250.0)
                .show(ctx, |ui| {
                    let channel = worker_sender.clone();
                    let view = ModulePanel::new(ctx, channel, modules);
                    ui.add(view)
                });
        }

        if view_config.show_block_cache {
            Window::new("Block Cache").show(ctx, |ui| {
                block_cache.show(ui, &api_key, &endpoint, stream_sender);
            });
        }

        if view_config.show_messages {
            egui::SidePanel::right("Messages").show(ctx, |ui| {
                panels::message_panel(ui, messages, message_search, gui_sender, worker_sender);
            });
        }

        if view_config.show_config {
            Window::new("Stream Config")
                .min_width(250.0)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Template Repository Path");
                        ui.text_edit_singleline(template_repo_path);

                        ui.separator();

                        ui.label("Start Block");
                        let mut start_block = editor_config.stream_start_block.to_string();
                        ui.text_edit_singleline(&mut start_block);
                        if let Ok(start_block) = start_block.parse::<i64>() {
                            editor_config.stream_start_block = start_block;
                        }
                        ui.separator();

                        ui.label("Stop Block");
                        let mut stop_block = editor_config.stream_stop_block.to_string();
                        ui.text_edit_singleline(&mut stop_block);
                        if let Ok(stop_block) = stop_block.parse::<u64>() {
                            editor_config.stream_stop_block = stop_block;
                        }
                    })
                });
        }

        if view_config.show_full_source {
            panels::rust_view_ui(ctx, &source_file);
        }

        if view_config.show_user_config {
            panels::user_config(
                ctx,
                &mut self.user_config,
                block_cache,
                &endpoint,
                &api_key,
                stream_sender,
            );
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            panels::menu_bar(
                ui,
                view_config,
                editor_config,
                template_repo_path,
                &api_key,
                &source_file,
                worker_sender,
                stream_sender,
            );

            if ui.button("Eval Block for `foo`").clicked() {
                let block = block_cache.get(1 as u8);

                gui_sender
                    .send(GuiMessage::PushMessage(
                        "Evaluating block for `foo`".to_string(),
                    ))
                    .unwrap();

                let fn_name = "foo".to_string();
                let args = vec![block.clone()];
                let message = WorkerMessage::EvalWithArgs(fn_name, args);
                worker_sender.send(message).unwrap();
            }
        });
    }
}
