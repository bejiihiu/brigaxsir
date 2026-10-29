//! Redirects, source modifiers and forks: three ways one input can reach a
//! different command — or the same command several times — without
//! duplicating its registration.
//!
//! Run with `cargo run --example redirect_fork`.

use std::sync::Arc;

use brigaxsir::{
    context::CommandContext,
    errors::CommandSyntaxException,
    modifier::{RedirectModifier, SingleRedirectModifier},
    prelude::*,
    result_consumer::ResultConsumer,
};

#[derive(Debug)]
struct Source(u32);

/// Prints one line per executed command, from whichever thread ran it.
struct PrintConsumer;

impl ResultConsumer<Source, i32> for PrintConsumer {
    fn on_command_complete(
        &self,
        context: Arc<CommandContext<Source, i32>>,
        success: bool,
        result: i32,
    ) {
        println!(
            "  done: source {}, success={success}, result={result}",
            context.get_source().0
        );
    }
}

fn main() {
    let mut dispatcher = CommandDispatcher::<Source>::new();
    dispatcher.set_consumer(PrintConsumer);

    // The target command everything below redirects to. Note the command
    // lives on the argument child: a redirect only engages when there is
    // trailing input after the alias (Java behaves the same way), so a bare
    // `hi` reports "Unknown command" while `hi Bob` runs greet.
    let greet = dispatcher.register(literal("greet").then(argument("name", string()).executes(
        |ctx: &CommandContext<Source>| {
            println!(
                "greet ran for source {}, name {}",
                ctx.get_source().0,
                get_string(ctx, "name")
            );
            1
        },
    )));

    // Plain redirect: `hi Bob` behaves exactly as if the user had typed
    // `greet Bob` — same source, same command.
    dispatcher.register(literal("hi").redirect(greet.clone()));

    // Redirect with a single-source modifier: the command still runs once,
    // but the source it sees is swapped first.
    let swap: Arc<SingleRedirectModifier<Source, i32>> =
        Arc::new(|ctx| Ok(Arc::new(Source(ctx.get_source().0 + 100))));
    dispatcher.register(literal("secret").redirect_with(greet.clone(), swap));

    // Fork: the modifier returns several sources and the target command runs
    // once per source, independently.
    let fanout: Arc<RedirectModifier<Source, i32>> = Arc::new(|ctx| {
        let base = ctx.get_source_arc().0;
        Ok(vec![Arc::new(Source(base + 1)), Arc::new(Source(base + 2))])
    });
    dispatcher.register(literal("fanout").fork(greet.clone(), fanout));

    // Redirecting to the root re-enters the whole tree: `again hi Bob` runs
    // like `hi Bob`. The usage renderer shows such a target as `...`.
    dispatcher.register(literal("again").redirect(dispatcher.get_root()));

    run(&dispatcher, "hi Bob", Source(1)).unwrap();
    run(&dispatcher, "secret Bob", Source(1)).unwrap();
    // forked execution: both sources run, the reported result is the sum.
    run(&dispatcher, "fanout Bob", Source(1)).unwrap();
    run(&dispatcher, "again hi Bob", Source(1)).unwrap();

    // A fork modifier may also return no sources at all: nothing runs, and
    // the caller still gets a well-defined "nothing happened" answer.
    let empty: Arc<RedirectModifier<Source, i32>> = Arc::new(|_| Ok(vec![]));
    dispatcher.register(literal("nothing").fork(greet.clone(), empty));
    let result = run(&dispatcher, "nothing Bob", Source(1)).unwrap();
    println!("fanout over zero sources ran {result} commands");
}

fn run(
    dispatcher: &CommandDispatcher<Source>,
    input: &str,
    source: Source,
) -> Result<i32, CommandSyntaxException> {
    println!("running `{input}` for source {}", source.0);
    dispatcher.execute(input, source)
}
