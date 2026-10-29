//! The `CompletableFuture` analog for suggestions: a boxed `std` future with
//! no runtime dependency, plus the `allOf`-style combinator the dispatcher
//! needs to fold many provider futures into one merged result.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::errors::CommandSyntaxException;

use super::Suggestions;

/// The future every suggestion producer hands back — the stand-in for Java's
/// `CompletableFuture<Suggestions>`.
///
/// It is a boxed [`Future`] (`Pin<Box<dyn Future<Output = ...> + Send>>`):
/// boxed because trait methods returning `impl Future` would drag `impl
/// Trait` bounds through every `dyn` boundary in the tree, and `Send` so a
/// future produced on one thread can be polled on another, like any Java
/// future can.
///
/// The `Result` rides *inside* the future (`Ok(Suggestions)` /
/// `Err(CommandSyntaxException)`), mirroring how a Java future completes
/// exceptionally when a provider throws inside it. One documented difference:
/// Java's dispatcher catches exceptions thrown *while obtaining* the future
/// and pretends that node suggested nothing; a boxed future cannot throw at
/// creation time, so a failing provider surfaces its error through the
/// dispatcher's merge step instead of being silently swallowed — return
/// `Ok` with no suggestions if that is the behaviour you want.
pub type SuggestionsFuture =
    Pin<Box<dyn Future<Output = Result<Suggestions, CommandSyntaxException>> + Send>>;

/// Poll every future to completion concurrently, then merge them against
/// `full_input` — the `CompletableFuture.allOf(...).thenRun(merge)` shape of
/// Java's `CommandDispatcher#getCompletionSuggestions`. All providers run
/// even when one of them errors; the error that comes out is the first `Err`
/// in list order. Java's own behaviour on a failed provider future is worse:
/// its `allOf` completes exceptionally, which skips the `thenRun` merge, and
/// nobody observes the failed stage — so Java's merged future never completes
/// at all (the caller's `join` hangs). The port surfaces the error instead,
/// which is the deliberate divergence documented on [`SuggestionsFuture`].
pub(crate) fn join_all_and_merge(
    futures: Vec<SuggestionsFuture>,
    full_input: String,
) -> SuggestionsFuture {
    // one completion slot per future; `None` means "still running"
    let results = vec![None; futures.len()];
    Box::pin(JoinAllAndMerge {
        futures,
        results,
        full_input,
    })
}

/// The future behind [`join_all_and_merge`].
struct JoinAllAndMerge {
    futures: Vec<SuggestionsFuture>,
    results: Vec<Option<Result<Suggestions, CommandSyntaxException>>>,
    full_input: String,
}

// every field is Unpin (Pin<Box<...>> is unconditionally Unpin), so polling
// through `get_mut` needs no pin projection
impl Future for JoinAllAndMerge {
    type Output = Result<Suggestions, CommandSyntaxException>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut pending = false;
        for (future, slot) in this.futures.iter_mut().zip(this.results.iter_mut()) {
            if slot.is_none() {
                match future.as_mut().poll(cx) {
                    Poll::Ready(result) => *slot = Some(result),
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            return Poll::Pending;
        }

        let mut collected = Vec::with_capacity(this.results.len());
        for slot in &this.results {
            match slot {
                Some(Ok(suggestions)) => collected.push(suggestions.clone()),
                Some(Err(error)) => return Poll::Ready(Err(error.clone())),
                None => unreachable!("every future completed before merging"),
            }
        }
        Poll::Ready(Ok(Suggestions::merge(&this.full_input, &collected)))
    }
}
