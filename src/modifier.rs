//! Redirect modifiers: turning one context's source into a list of sources.
//!
//! Mirrors Java's `com.mojang.brigadier.RedirectModifier`.

use std::sync::Arc;

use crate::{context::CommandContext, errors::CommandSyntaxException};

/// A function applied to a redirect target's context to produce additional
/// command sources. Java: `RedirectModifier<S>` — `apply(CommandContext)`
/// returns the list of sources the following command runs with.
///
/// Returning an empty list (in forked mode) aborts the redirect silently;
/// see [`crate::context::ContextChain::execute_all`] for how the results are
/// consumed.
pub type RedirectModifier<S, R> =
    dyn Fn(&CommandContext<S, R>) -> Result<Vec<Arc<S>>, CommandSyntaxException> + Send + Sync;

/// A redirect modifier that maps one context to exactly one source. Java:
/// `SingleRedirectModifier<S>` — `apply(CommandContext)` returns the single
/// source the redirect runs the target with.
///
/// The source comes back as the [`Arc`] handle, not the bare `S` Java's
/// signature shows — the same rule [`RedirectModifier`] authors already
/// live by ([`CommandContext::get_source_arc`]); handing the handle back
/// keeps source identity intact for `copy_for` and friends.
///
/// [`crate::builder::ArgumentBuilder::redirect_with`] takes this and wraps it
/// into a singleton [`RedirectModifier`] (Java wraps it in
/// `Collections.singleton(...)` the same way), so a redirect with a single
/// modifier never forks.
pub type SingleRedirectModifier<S, R> =
    dyn Fn(&CommandContext<S, R>) -> Result<Arc<S>, CommandSyntaxException> + Send + Sync;
