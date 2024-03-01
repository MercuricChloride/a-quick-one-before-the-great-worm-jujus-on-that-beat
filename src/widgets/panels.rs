use std::sync::mpsc::Sender;

use eframe::{
    egui::{self, menu, Context, ScrollArea, Ui, Window},
    Frame,
};

use crate::{
    block_cache::BlockCache,
    tasks::{GuiMessage, MessageKind, StreamMessages, WorkerMessage},
    EditorConfig, EditorViews, UserConfig,
};

/// Opens a window to configure the users settings
pub fn user_config(
    ctx: &Context,
    user_config: &mut UserConfig,
    block_cache: &mut BlockCache,
    endpoint: &str,
    api_key: &str,
    stream_sender: &Sender<StreamMessages>,
) {
    Window::new("User Config").min_width(250.0).show(ctx, |ui| {
        ui.collapsing("Substream Config", |ui| {
            ui.add(user_config);
        });

        ui.separator();

        ui.collapsing("Block Config", |ui| {
            block_cache.show(ui, api_key, endpoint, stream_sender)
        });
    });
}

/// Opens a window to view the source code of the compiled rhai scripts
pub fn rust_view_ui(ctx: &Context, code: &str) {
    let language = "rs";
    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ctx);
    Window::new("Full Source").show(ctx, |ui| {
        egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language)
    });
}

/// Shows the panel for the messages to the system
pub fn message_panel(
    ui: &mut Ui,
    messages: &Vec<MessageKind>,
    message_search: &mut String,
    gui_sender: &Sender<GuiMessage>,
    worker_sender: &Sender<WorkerMessage>,
) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Messages");
        ui.text_edit_singleline(message_search);
        ui.horizontal(|ui| {
            if ui.button("Clear Messages").clicked() {
                gui_sender.send(GuiMessage::ClearMessages).unwrap();
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            for (i, message) in messages.iter().enumerate() {
                match message {
                    MessageKind::JsonMessage(json) => {
                        match &json {
                            serde_json::Value::Null => continue,
                            serde_json::Value::Array(arr) => {
                                if arr.is_empty() {
                                    continue;
                                }
                            }
                            serde_json::Value::Object(obj) => {
                                if obj.is_empty() {
                                    continue;
                                }
                            }
                            _ => {}
                        };

                        let id = format!("json_message:{}", i);
                        egui_json_tree::JsonTree::new(id, json)
                            .default_expand(egui_json_tree::DefaultExpand::SearchResults(
                                message_search,
                            ))
                            .show(ui);
                    }
                    MessageKind::TextMessage(msg) => {
                        ui.label(msg);
                    }
                }
            }
        });
    });
}

/// Shows the menu bar for the application
pub fn menu_bar(
    ui: &mut Ui,
    view_config: &mut EditorViews,
    editor_config: &mut EditorConfig,
    template_repo_path: &mut String,
    api_key: &str,
    source_file: &str,
    worker_sender: &Sender<WorkerMessage>,
    stream_sender: &Sender<StreamMessages>,
) {
    menu::bar(ui, |ui| {
        ui.menu_button("Panels", |ui| {
            ui.checkbox(&mut view_config.show_config, "Toggle Config Panel");
            ui.checkbox(&mut view_config.show_modules, "Toggle Modules Panel");
            ui.checkbox(&mut view_config.show_block_cache, "Toggle Block Cache");
            ui.checkbox(&mut view_config.show_messages, "Toggle Messages Panel");
            ui.checkbox(
                &mut view_config.show_user_config,
                "Toggle User Config Panel",
            );
            ui.checkbox(&mut view_config.show_null_json, "Show Null Json?");
            ui.checkbox(&mut view_config.show_full_source, "Show Full Source");
        });

        ui.menu_button("Run", |ui| {
            if ui.button("Run in repl").clicked() {
                let message = WorkerMessage::Eval(source_file.to_string());
                worker_sender.send(message).unwrap();
            }

            if ui.button("Run a stream").clicked() {
                let message = StreamMessages::Run {
                    start: editor_config.stream_start_block,
                    stop: editor_config.stream_stop_block,
                    api_key: api_key.to_string(),
                    package_file: editor_config.substream_package.clone(),
                    endpoint: editor_config.substream_endpoint.clone(),
                    module_name: editor_config.module_name.clone(),
                };
                stream_sender.send(message).unwrap()
            }

            if ui.button("Build").clicked() {
                let message = WorkerMessage::Build;
                worker_sender.send(message).unwrap();
            }
        });
    });
}
