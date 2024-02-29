use std::{
    collections::HashMap,
    sync::{mpsc, RwLock},
};

use eframe::egui::{self, ComboBox, Key, Response, Ui, Widget, Window};

use crate::{Module, WorkerMessage};

pub struct ModulePanel<'a> {
    context: &'a egui::Context,
    channel: mpsc::Sender<WorkerMessage>,
    modules: &'a mut RwLock<HashMap<String, Module>>,
}

impl<'a> ModulePanel<'a> {
    pub fn new(
        context: &'a egui::Context,
        channel: mpsc::Sender<WorkerMessage>,
        modules: &'a mut RwLock<HashMap<String, Module>>,
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
        let mut modules = self.modules.write().unwrap();
        ui.heading("Modules");
        ui.separator();
        let module_names = modules
            .iter()
            .map(|(k, v)| v.name().to_string())
            .collect::<Vec<_>>();
        for (name, module) in modules.iter_mut() {
            ui.collapsing(name.as_str(), |ui| {
                ui.collapsing("Inputs", |ui| {
                    for input in module.inputs_mut() {
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
                                ui.selectable_value(input, "BLOCK".to_string(), "BLOCK");
                            });
                    }
                    if ui.button("Add Input").clicked() {
                        module.inputs_mut().push("BLOCK".to_string());
                    }
                });

                ui.checkbox(module.editing_mut(), "Show Editor?");

                if *module.editing() {
                    Window::new(name).show(ctx, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Eval").clicked()
                                    || ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.ctrl)
                                {
                                    let code = module.code();
                                    let message = WorkerMessage::Eval(code.to_string());
                                    self.channel.send(message);
                                }

                                ui.collapsing("Module Configuration", |ui| {});
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
                        code: format!("fn {name}(BLOCK) {{ block.number }}"),
                        inputs: vec!["BLOCK".to_string()],
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
        .response
    }
}
