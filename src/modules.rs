use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    pub fn register_module(&self, module_map: &HashMap<String, Module>) -> String {
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

    pub fn default() -> HashMap<String, Self> {
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
