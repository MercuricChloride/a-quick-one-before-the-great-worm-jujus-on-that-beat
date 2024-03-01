use std::sync::mpsc::Sender;

use eframe::egui::{self, Response, Ui, Widget, Window};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EditorState, StreamMessages};

const ETH_BLOCK_SUBSTREAM: &str = "https://spkg.io/streamingfast/ethereum-explorer-v0.1.2.spkg";

#[derive(Serialize, Deserialize, Default)]
pub struct BlockCacheUiState {
    block_number: u64,
    cache_index: u8,
}

#[derive(Serialize, Deserialize, Default)]
pub struct BlockCache {
    block_1: Value,
    block_2: Value,
    block_3: Value,
    block_4: Value,
    state: BlockCacheUiState,
}

impl BlockCache {
    pub fn set(&mut self, block: u8, value: Value) {
        match block {
            1 => self.block_1 = value,
            2 => self.block_2 = value,
            3 => self.block_3 = value,
            4 => self.block_4 = value,
            _ => println!("Invalid block number"),
        }
    }

    pub fn get(&self, block: u8) -> &Value {
        match block {
            1 => &self.block_1,
            2 => &self.block_2,
            3 => &self.block_3,
            4 => &self.block_4,
            _ => {
                println!("Invalid block number");
                &Value::Null
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        api_key: &str,
        endpoint: &str,
        stream_sender: &Sender<StreamMessages>,
    ) -> Response {
        let state = &mut self.state;
        let mut temp = state.block_number.to_string();

        ui.label("Block 1");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut temp);
            if let Ok(temp) = temp.parse() {
                state.block_number = temp;
            };

            if ui.button("Get").clicked() {
                let message = StreamMessages::GetBlock {
                    number: state.block_number as i64,
                    api_key: api_key.to_string(),
                    endpoint: endpoint.to_string(),
                    cache_slot: 1 as u8,
                };
                stream_sender.send(message).unwrap();
            }
        })
        .response
    }
}
