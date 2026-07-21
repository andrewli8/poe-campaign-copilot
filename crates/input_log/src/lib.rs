//! Passive Client.txt tailing: byte chunks -> complete text lines,
//! resilient to partial writes, truncation, and file replacement.

mod assembler;
mod poller;
mod tailer;

pub use assembler::LineAssembler;
pub use poller::FilePoller;
pub use tailer::{spawn_tailer, TailerHandle};
