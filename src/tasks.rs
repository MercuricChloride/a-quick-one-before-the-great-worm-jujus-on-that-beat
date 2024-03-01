//! This module contains the code to handle background threads and message passing between them
//!
//! At any point in the running program, we have three threads running:
//! 1. The gui thread, which is the main thread that runs the GUI
//! 2. The stream thread, which is a thread that runs the substreams engine
//! 3. The worker thread, which is a thread that runs rhai scripts

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Messages that can be sent to the worker thread
pub enum WorkerMessage {
    Eval(String),
    EvalWithArgs(String, Vec<Value>),
    Reset,
    Build,
}

/// Messages that can be sent to the gui thread
pub enum GuiMessage {
    PushMessage(String),
    PushJson(String),
    SetBlock(u8, String),
    ClearMessages,
}

pub enum StreamMessages {
    Run {
        start: i64,
        stop: u64,
        api_key: String,
        package_file: String,
        endpoint: String,
        module_name: String,
    },

    GetBlock {
        number: i64,
        api_key: String,
        endpoint: String,
        cache_slot: u8,
    },
}

#[derive(Serialize, Deserialize)]
pub enum MessageKind {
    JsonMessage(Value),
    TextMessage(String),
    //ErrorMessage(String),
}
