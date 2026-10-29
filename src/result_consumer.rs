//! Result consumers: callbacks invoked when a command finishes.
//!
//! Mirrors Java's `com.mojang.brigadier.ResultConsumer`.

use std::sync::Arc;

use crate::context::CommandContext;

/// Called once for each executed command, with the final context, whether it
/// succeeded, and its numeric result. Java: `ResultConsumer#onCommandComplete`.
///
/// The context arrives as the shared [`Arc`] handle (Java hands the object
/// itself); since the dispatcher stores its consumers as
/// `Box<dyn ResultConsumer + Send + Sync>`, an implementor may be called
/// from any thread that runs the dispatcher — keep shared state behind
/// locks/atomics, like the crate's own tests do.
pub trait ResultConsumer<S, R> {
    /// Notify that a command completed. `result` is the command's return value
    /// (`0` when the command failed or was not run).
    fn on_command_complete(&self, context: Arc<CommandContext<S, R>>, success: bool, result: i32);
}

/// The default consumer: does nothing. Java only wires consumers explicitly,
/// but the dispatcher here always holds one, so this stands in for "no
/// consumer".
pub struct DefaultResultConsumer;
impl<S, R> ResultConsumer<S, R> for DefaultResultConsumer {
    fn on_command_complete(
        &self,
        _context: Arc<CommandContext<S, R>>,
        _success: bool,
        _result: i32,
    ) {
    }
}
