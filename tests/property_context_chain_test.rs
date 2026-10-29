//! Property tests for `ContextChain::execute_all` against a model of Java's
//! semantics. Random chains of redirect stages (each forked or not, each
//! widening the source set by 0–3×) are built through a real dispatcher
//! tree, executed, and compared with the same fork/sum rules Java's
//! `ContextChain#executeAll` implements:
//!
//! - the fork mode is sticky: set by any stage, it flips every later
//!   executable run into the folded `1`-per-success form;
//! - a stage producing zero sources short-circuits the chain at `0` (no
//!   command runs, no consumer events);
//! - otherwise the result is the sum over the surviving source set —
//!   `width * value` when not forked, `sources` when forked;
//! - the consumer sees exactly one success event per executed source, and
//!   the runtime source never reaches a command when a modifier ran.
//!
//! A second property walks the stage chain: one `Modify` stage per
//! redirect, ending in `Execute`, with `next_stage` returning the same
//! cached object on repeated calls. proptest's defaults are kept (256
//! cases, deterministic RNG).

use std::sync::Arc;

use parking_lot::Mutex;
use proptest::prelude::*;

use brigaxsir::{
    builder::literal_argument_builder::literal,
    command_dispatcher::CommandDispatcher,
    context::{CommandContext, ContextChain, Stage},
    modifier::RedirectModifier,
    result_consumer::ResultConsumer,
};

/// Recorded consumer call: `(context source, success, result)`.
#[derive(Default, Clone)]
struct RecordingConsumer {
    calls: Arc<Mutex<Vec<(i64, bool, i32)>>>,
}

impl ResultConsumer<i64, i32> for RecordingConsumer {
    fn on_command_complete(
        &self,
        context: Arc<CommandContext<i64, i32>>,
        success: bool,
        result: i32,
    ) {
        self.calls
            .lock()
            .push((*context.get_source(), success, result));
    }
}

/// The Java `executeAll` model for a chain whose modifier stages map every
/// incoming source to `width` fixed sources (`1000 + j`), never fail, and
/// whose executable always returns `value`. Returns the expected result and
/// the number of consumer events.
fn expected_outcome(stages: &[(bool, u32)], value: i32) -> (i32, usize) {
    let mut forked = false;
    let mut count = 1usize;
    for &(fork, width) in stages {
        forked |= fork;
        count *= width as usize;
        if count == 0 {
            break;
        }
    }
    if count == 0 {
        (0, 0)
    } else if forked {
        (count as i32, count)
    } else {
        (value * count as i32, count)
    }
}

/// Build the real tree behind the model: the `foo` executable plus one
/// redirect literal per stage, every redirect pointing back at the root,
/// and the input string walking all of them (`red0 red1 ... foo`).
fn build_chain(stages: &[(bool, u32)], value: i32) -> (CommandDispatcher<i64, i32>, String) {
    let mut subject = CommandDispatcher::<i64, i32>::new();
    subject.register(literal("foo").executes(move |_| value));

    for (i, &(fork, width)) in stages.iter().enumerate() {
        let modifier: Arc<RedirectModifier<i64, i32>> =
            Arc::new(move |_| Ok((0..width).map(|j| Arc::new(1000 + i64::from(j))).collect()));
        let builder = literal(&format!("red{i}")).forward(subject.get_root(), Some(modifier), fork);
        subject.register(builder);
    }

    let input: String = stages
        .iter()
        .enumerate()
        .map(|(i, _)| format!("red{i} "))
        .chain(std::iter::once("foo".to_owned()))
        .collect();
    (subject, input)
}

proptest! {
    #[test]
    fn execute_all_sums_like_java((stages, value) in (proptest::collection::vec((any::<bool>(), 0u32..=3), 0..=3), 1i32..=9)) {
        let (subject, input) = build_chain(&stages, value);
        let parse = subject.parse(input.as_str(), 0i64);
        let top_context = Arc::new(parse.context.build(input.as_str()));
        let chain = ContextChain::try_flatten(top_context)
            .expect("the chain ends in an executable command");
        let consumer = RecordingConsumer::default();

        let result = chain.execute_all(Arc::new(7i64), &consumer);
        let (expected, calls) = expected_outcome(&stages, value);
        prop_assert_eq!(result.ok(), Some(expected), "chain {:?} with value {}", stages, value);

        let recorded = consumer.calls.lock();
        prop_assert_eq!(
            recorded.len(),
            calls,
            "chain {:?}: wrong consumer event count",
            stages
        );
        for (source, success, result) in recorded.iter() {
            prop_assert!(*success, "all executed sources succeed");
            prop_assert_eq!(*result, value, "the command always returns its constant");
            if stages.is_empty() {
                // no modifier ran: the command saw the runtime source itself
                prop_assert_eq!(*source, 7);
            } else {
                // every stage replaces the source set wholesale, so the
                // runtime source can never reach the command
                prop_assert!(
                    (1000..=1002).contains(source),
                    "unexpected modified source {source}"
                );
            }
        }
    }

    #[test]
    fn next_stage_walk_matches_the_chain_shape((stages, value) in (proptest::collection::vec((any::<bool>(), 0u32..=3), 0..=3), 1i32..=9)) {
        let (subject, input) = build_chain(&stages, value);
        let parse = subject.parse(input.as_str(), 0i64);
        let top_context = Arc::new(parse.context.build(input.as_str()));
        let mut stage: Arc<ContextChain<i64, i32>> = Arc::new(
            ContextChain::try_flatten(top_context).expect("the chain ends in an executable command"),
        );

        let mut visited = 0;
        while let Some(next) = stage.next_stage() {
            prop_assert!(matches!(stage.get_stage(), Stage::Modify));
            // the cache: a repeated call hands back the same stage object
            let cached = stage.next_stage().expect("the cached stage stays");
            prop_assert!(Arc::ptr_eq(&cached, &next));
            stage = next;
            visited += 1;
            prop_assert!(visited <= stages.len(), "the stage walk ran away");
        }
        prop_assert!(matches!(stage.get_stage(), Stage::Execute));
        prop_assert_eq!(visited, stages.len());
    }
}
