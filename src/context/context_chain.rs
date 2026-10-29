//! A chain of contexts along the redirect/modifier path, mirroring Java's
//! `com.mojang.brigadier.context.ContextChain`.
//!
//! Java's file carries the library's single known open TODO: "ideally those
//! two would have separate types, but modifiers and executables expect full
//! context". The port closes it — [`ModifierStage`] and [`ExecutableStage`]
//! are the separate types, and both hand out the full
//! [`CommandContext`] the Java comment worried about.

use std::sync::Arc;

use parking_lot::Mutex;

use super::CommandContext;
use crate::{
    errors::{CommandResultTrait, CommandSyntaxException},
    result_consumer::ResultConsumer,
};

/// The command closure an executable stage carries. The inner half of
/// [`tree::Command`](crate::tree::Command) (that alias adds the `Option`).
type StageCommand<S, R> =
    Arc<dyn Fn(&CommandContext<S, R>) -> Result<R, CommandSyntaxException> + Send + Sync>;

/// A context on the modifier path of a [`ContextChain`]: a redirect stage
/// whose own command (if any) is never run. Any context is a valid modifier
/// stage — Java never runs a modifier's command either, so the split guards
/// the chain shape without rejecting anything Java would accept.
pub struct ModifierStage<S, R> {
    context: Arc<CommandContext<S, R>>,
}

impl<S, R> ModifierStage<S, R> {
    /// Wrap a context as a modifier stage. Java: the modifier entries of
    /// `ContextChain`'s `modifiers` list.
    pub fn new(context: Arc<CommandContext<S, R>>) -> Self {
        Self { context }
    }

    /// The wrapped context, handed out in full — the "modifiers expect full
    /// context" half of Java's TODO.
    pub fn context(&self) -> &Arc<CommandContext<S, R>> {
        &self.context
    }
}

impl<S, R> Clone for ModifierStage<S, R> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
        }
    }
}

impl<S, R> std::fmt::Debug for ModifierStage<S, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModifierStage").finish_non_exhaustive()
    }
}

/// The executable stage of a [`ContextChain`]: the chain's final context,
/// carrying its command. Java guards the "executable has a command"
/// invariant with a runtime `IllegalArgumentException` ("Last command in
/// chain must be executable"); here the failure is unrepresentable — the
/// stage can only be built for a command-bearing context, and it keeps the
/// command so execution never has to re-check it.
pub struct ExecutableStage<S, R> {
    context: Arc<CommandContext<S, R>>,
    command: StageCommand<S, R>,
}

impl<S, R: CommandResultTrait> ExecutableStage<S, R> {
    /// Wrap a context as the executable stage. Returns `None` when the
    /// context carries no command — the typed form of Java's
    /// `IllegalArgumentException("Last command in chain must be executable")`
    /// from the `ContextChain` constructor.
    pub fn new(context: Arc<CommandContext<S, R>>) -> Option<Self> {
        // Java's runExecutable reads the command off the original context
        // (not the per-source copy), so hoisting it here is the same object
        // Java would call through.
        let command = context.get_command().as_ref()?.clone();
        Some(Self { context, command })
    }

    /// The wrapped context, handed out in full — the "executables expect
    /// full context" half of Java's TODO.
    pub fn context(&self) -> &Arc<CommandContext<S, R>> {
        &self.context
    }
}

impl<S, R> Clone for ExecutableStage<S, R> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            command: self.command.clone(),
        }
    }
}

impl<S, R> std::fmt::Debug for ExecutableStage<S, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutableStage").finish_non_exhaustive()
    }
}

/// The contexts a single `execute` touches: zero or more modifier stages
/// ending in exactly one executable stage.
///
/// # Thread safety
///
/// The contexts are shared through `Arc` handles and the next-stage cache is
/// a `Mutex`, so `ContextChain<S, R>` is `Send + Sync` exactly when
/// `S: Send + Sync`. Java mutates the cache through any reference; the lock
/// gives the same behavior under shared references and stays correct if two
/// threads race on `next_stage` (Java's practical rule is single-threaded
/// chain use — the lock adds that safety for free).
pub struct ContextChain<S, R> {
    modifiers: Vec<ModifierStage<S, R>>,
    executable: ExecutableStage<S, R>,
    // Java caches the next stage on the chain object itself, callable
    // through any reference — a plain field would force `&mut self` here,
    // which is unusable behind the returned `Arc` handle (the parent chain's
    // cache always holds a second reference). The mutex keeps Java's lazy
    // compute-once, same-object-return semantics under shared references.
    next_stage_cache: Mutex<Option<Arc<ContextChain<S, R>>>>,
}

impl<S, R: CommandResultTrait> ContextChain<S, R> {
    /// Build a chain from its stages. Infallible: an [`ExecutableStage`]
    /// only exists for a context with a command, which is Java's runtime
    /// "Last command in chain must be executable" check turned into the type
    /// system.
    pub fn new(modifiers: Vec<ModifierStage<S, R>>, executable: ExecutableStage<S, R>) -> Self {
        Self {
            modifiers,
            executable,
            next_stage_cache: Mutex::new(None),
        }
    }

    /// Flatten a context chain built by parsing into a [`ContextChain`],
    /// returning `None` if the final context has no command (nothing to
    /// execute — Java reports unknown-command in that case).
    pub fn try_flatten(root_context: Arc<CommandContext<S, R>>) -> Option<Self> {
        let mut modifiers = Vec::new();
        let mut current = root_context;
        loop {
            let child = current.get_child();
            let Some(child) = child else {
                // Last entry must be executable command
                let executable = ExecutableStage::new(current)?;
                return Some(ContextChain::new(modifiers, executable));
            };

            modifiers.push(ModifierStage::new(current));
            current = child;
        }
    }

    /// Run one modifier stage: apply its redirect modifier to `source` and
    /// return the resulting sources. A missing modifier is the identity (`Ok`
    /// with just `source`). In forked mode a failing modifier is swallowed
    /// with an empty source list; otherwise the error propagates after the
    /// consumer is notified.
    pub fn run_modifier(
        modifier: &ModifierStage<S, R>,
        source: Arc<S>,
        result_consumer: &dyn ResultConsumer<S, R>,
        forked_mode: bool,
    ) -> Result<Vec<Arc<S>>, CommandSyntaxException> {
        let context = modifier.context();
        let source_modifier = context.get_redirect_modifier();
        let Some(source_modifier) = source_modifier else {
            return Ok(vec![source]);
        };

        let context_to_use = Arc::new(context.copy_for(source));
        let err = match (source_modifier)(&context_to_use) {
            Ok(res) => return Ok(res),
            Err(e) => e,
        };

        result_consumer.on_command_complete(context_to_use, false, 0);
        if forked_mode {
            return Ok(vec![]);
        }
        Err(err)
    }

    /// Execute the executable stage for `source`, notifying the consumer
    /// either way, and folding results into forked-mode semantics (`1` per
    /// success, `0` on failure).
    pub fn run_executable(
        executable: &ExecutableStage<S, R>,
        source: Arc<S>,
        result_consumer: &dyn ResultConsumer<S, R>,
        forked_mode: bool,
    ) -> Result<R, CommandSyntaxException> {
        let context = executable.context();
        let context_to_use = Arc::new(context.copy_for(source));

        let res = (executable.command)(&context_to_use);
        let err = match res {
            Ok(res) => {
                let Some(res) = res.as_i32() else {
                    // these are treated as exceptions, so they can bubble up
                    // without doing anything else
                    return Ok(res);
                };
                result_consumer.on_command_complete(context_to_use, true, res);
                return if forked_mode {
                    Ok(R::new(1))
                } else {
                    Ok(R::new(res))
                };
            }
            Err(err) => err,
        };

        result_consumer.on_command_complete(context_to_use, false, 0);
        if forked_mode { Ok(R::new(0)) } else { Err(err) }
    }

    /// Run the whole chain: every modifier, then the executable for each
    /// surviving source. Forked mode changes failure handling and result
    /// folding, exactly like Java. Non-`i32` results short-circuit and are
    /// returned unwrapped.
    pub fn execute_all(
        &self,
        source: Arc<S>,
        result_consumer: &dyn ResultConsumer<S, R>,
    ) -> Result<R, CommandSyntaxException> {
        if self.modifiers.is_empty() {
            return Self::run_executable(&self.executable, source, result_consumer, false);
        }

        let mut forked_mode = false;
        let mut current_sources = vec![source];

        for modifier in &self.modifiers {
            forked_mode |= modifier.context().is_forked();

            let mut next_sources = Vec::new();
            for source_to_run in current_sources {
                let res = Self::run_modifier(
                    modifier,
                    source_to_run.clone(),
                    result_consumer,
                    forked_mode,
                )?;
                next_sources.extend(res);
            }
            if next_sources.is_empty() {
                return Ok(R::new(0));
            }
            current_sources = next_sources;
        }

        let mut summed = 0;
        for execution_source in current_sources {
            let res = Self::run_executable(
                &self.executable,
                execution_source,
                result_consumer,
                forked_mode,
            )?;
            match res.as_i32() {
                Some(res) => summed += res,
                None => return Ok(res),
            }
        }

        Ok(R::new(summed))
    }

    /// Which stage this chain is at. Java: `getStage`.
    pub fn get_stage(&self) -> Stage {
        if self.modifiers.is_empty() {
            Stage::Execute
        } else {
            Stage::Modify
        }
    }

    /// The topmost context in the chain (first modifier, or the executable).
    /// Java: `getTopContext`.
    pub fn get_top_context(&self) -> Arc<CommandContext<S, R>> {
        self.modifiers
            .first()
            .map(|stage| stage.context().clone())
            .unwrap_or_else(|| self.executable.context().clone())
    }

    /// The next stage of the chain, or `None` when this is the final stage.
    /// Java: `nextStage`. The result is cached on this chain (Java's field),
    /// so every call through the same chain returns the same stage object —
    /// like Java, callable through any shared reference (the cache is a
    /// mutex here, so racing calls still observe one shared stage).
    pub fn next_stage(&self) -> Option<Arc<ContextChain<S, R>>> {
        if self.modifiers.is_empty() {
            return None;
        }

        let mut cache = self.next_stage_cache.lock();
        if cache.is_none() {
            let next_modifiers = self.modifiers[1..]
                .iter()
                .map(|stage| ModifierStage::new(stage.context().clone()))
                .collect();
            *cache = Some(Arc::new(ContextChain::new(
                next_modifiers,
                self.executable.clone(),
            )));
        }

        cache.as_ref().cloned()
    }
}

/// The two stages of a context chain, mirroring Java's `ContextChain.Stage`.
pub enum Stage {
    /// This chain still has modifiers to run.
    Modify,
    /// This chain is ready to execute.
    Execute,
}
