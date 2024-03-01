use std::{
    collections::HashMap,
    sync::{mpsc, Arc, RwLock},
};

use eframe::egui::{self, ComboBox, Key, Response, Ui, Widget, Window};
use rand::random;

use crate::{Module, WorkerMessage};

pub struct ModulePanel<'a> {
    context: &'a egui::Context,
    channel: mpsc::Sender<WorkerMessage>,
    modules: &'a mut HashMap<i64, Module>,
}

impl<'a> ModulePanel<'a> {
    pub fn new(
        context: &'a egui::Context,
        channel: mpsc::Sender<WorkerMessage>,
        modules: &'a mut HashMap<i64, Module>,
    ) -> Self {
        Self {
            context,
            modules,
            channel,
        }
    }
}

impl Widget for ModulePanel<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = self.context;
        let modules = self.modules;

        ui.heading("Modules");

        ui.separator();

        let module_names = &modules
            .iter()
            .map(|(k, v)| v.name().to_string())
            .collect::<Vec<_>>();

        for (id, module) in modules.iter_mut() {
            let module_name = module.name().to_string();

            ui.checkbox(module.editing_mut(), module_name);

            if *module.editing() {
                Window::new(module.name()).show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Eval").clicked()
                                || ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.ctrl)
                            {
                                let code = module.code();
                                let message = WorkerMessage::Eval(code.to_string());
                                self.channel.send(message).unwrap();
                            }

                            ui.collapsing("Module Configuration", |ui| match module {
                                Module::Map {
                                    name,
                                    code,
                                    inputs,
                                    editing,
                                } => {
                                    ui.label("Module Name");
                                    ui.text_edit_singleline(name);
                                    ui.separator();

                                    ui.label("Inputs. (Each on a new line)");
                                    for input in inputs.iter_mut() {
                                        ComboBox::from_label("Input")
                                            .selected_text(input.as_str())
                                            .show_ui(ui, |ui| {
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
                                                ui.selectable_value(
                                                    input,
                                                    "BLOCK".to_string(),
                                                    "BLOCK",
                                                );
                                            });
                                    }
                                }
                                Module::Store {
                                    name,
                                    code,
                                    inputs,
                                    update_policy,
                                    editing,
                                } => {
                                    ui.label("Store Configuration");
                                    ui.separator();

                                    ui.label("Module Name");
                                    ui.text_edit_singleline(name);
                                    ui.separator();

                                    ui.label("Update Policy");
                                    ComboBox::from_label("Update Policy")
                                        .selected_text(update_policy.as_str())
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                update_policy,
                                                "set".to_string(),
                                                "set",
                                            );
                                            ui.selectable_value(
                                                update_policy,
                                                "setOnce".to_string(),
                                                "setOnce",
                                            );
                                        });

                                    ui.label("Inputs. (Each on a new line)");
                                    for input in inputs.iter_mut() {
                                        ComboBox::from_label("Input")
                                            .selected_text(input.as_str())
                                            .show_ui(ui, |ui| {
                                                for module_name in module_names.iter() {
                                                    ui.selectable_value(
                                                        input,
                                                        module_name.to_string(),
                                                        module_name,
                                                    );
                                                }
                                                ui.selectable_value(
                                                    input,
                                                    "BLOCK".to_string(),
                                                    "BLOCK",
                                                );
                                            });
                                    }
                                }
                            });
                        });
                        ui.code_editor(module.code_mut());
                    });
                });
            }
            ui.end_row();
        }

        ui.horizontal(|ui| {
            if ui.button("Add Mfn").clicked() {
                let name = "template_mfn";
                modules.insert(
                    random(),
                    Module::Map {
                        name: name.to_string(),
                        code: format!("fn {name}(BLOCK) {{ block.number }}"),
                        inputs: vec!["BLOCK".to_string()],
                        editing: true,
                    },
                );
            }
            if ui.button("Add SFN").clicked() {
                let name = "template_sfn";
                modules.insert(
                    random(),
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
        .response
    }
}
