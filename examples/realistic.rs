//! A small game-style command system, written the way a real application
//! would: permission-gated commands, two custom argument types (players and
//! items) with completion suggestions, tooltips, an optional argument, and a
//! fork that delivers a chat message to every other player.
//!
//! 16 tree nodes (the root plus 15), every headline feature of the library
//! in one place. Run with `cargo run --example realistic`.

use std::{
    any::Any,
    hash::Hasher,
    sync::{Arc, LazyLock},
};

use brigaxsir::{
    arguments::ArgumentType,
    context::{CommandContext, ParsedValue},
    errors::{CommandSyntaxException, DynamicCommandExceptionType, SimpleCommandExceptionType},
    message::LiteralMessage,
    modifier::RedirectModifier,
    prelude::*,
    result_consumer::ResultConsumer,
    string_reader::StringReader,
    suggestion::{SuggestionsBuilder, SuggestionsFuture},
};

/// A logged-in player. The dispatcher is generic over the source, so this is
/// the game's own type — no library types leak into it.
#[derive(Clone, Debug)]
struct Player {
    name: &'static str,
    admin: bool,
}

/// Prints one line per executed command, so forks are visible.
struct AuditLog;

impl ResultConsumer<Player, i32> for AuditLog {
    fn on_command_complete(
        &self,
        context: Arc<CommandContext<Player, i32>>,
        success: bool,
        result: i32,
    ) {
        println!(
            "    [audit] {} success={} result={}",
            context.get_source().name,
            success,
            result
        );
    }
}

/// The players currently online. A real game would ask its session manager.
const ONLINE_PLAYERS: [&str; 4] = ["Nova", "Alice", "Bob", "Zed"];

/// (item name, tooltip shown in completions).
const ITEMS: [(&str, &str); 4] = [
    ("sword", "A trusty blade"),
    ("pickaxe", "Mines stone faster"),
    ("bread", "Restores 5 hunger"),
    ("torch", "Lights up caves"),
];

/// "Unknown player" with the bad name baked in — Java's
/// `DynamicCommandExceptionType` shape.
static UNKNOWN_PLAYER: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|name| {
        Arc::new(LiteralMessage::new(format!("Unknown player '{name}'")))
    })
});

/// "Unknown item", same idea.
static UNKNOWN_ITEM: LazyLock<DynamicCommandExceptionType<String>> = LazyLock::new(|| {
    DynamicCommandExceptionType::new(|name| {
        Arc::new(LiteralMessage::new(format!("Unknown item '{name}'")))
    })
});

/// A fixed programmer-game error — Java's `SimpleCommandExceptionType`.
static CANNOT_KICK_SELF: LazyLock<SimpleCommandExceptionType> = LazyLock::new(|| {
    SimpleCommandExceptionType::new(Arc::new(LiteralMessage::new("You cannot kick yourself")))
});

/// The value the player argument parses into.
#[derive(PartialEq, Eq)]
struct PlayerName(String);

impl ParsedValue for PlayerName {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        "PlayerName"
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<PlayerName>() == Some(self)
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write(self.0.as_bytes())
    }
}

/// Parses an online player's name and completes the online list.
struct PlayerNameArgument;

impl ArgumentType for PlayerNameArgument {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        let word = reader.read_unquoted_string().to_owned();
        if ONLINE_PLAYERS.contains(&word.as_str()) {
            Ok(Arc::new(PlayerName(word)))
        } else {
            Err(UNKNOWN_PLAYER.create_with_context(reader, word))
        }
    }

    fn list_suggestions(&self, builder: SuggestionsBuilder) -> SuggestionsFuture {
        let remaining = builder.remaining_lowercase().to_owned();
        let mut builder = builder;
        for player in ONLINE_PLAYERS {
            // prefix-filtering is the custom argument's job (Java's builder
            // doesn't do it either — only literal nodes check the prefix
            // themselves)
            if player.to_lowercase().starts_with(&remaining) {
                builder = builder.suggest(player);
            }
        }
        builder.build_future()
    }

    fn examples(&self) -> Vec<String> {
        ONLINE_PLAYERS.iter().map(|name| name.to_string()).collect()
    }
}

/// The value the item argument parses into.
#[derive(PartialEq, Eq)]
struct Item {
    name: String,
}

impl ParsedValue for Item {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        "Item"
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<Item>() == Some(self)
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write(self.name.as_bytes())
    }
}

/// Parses an item id and completes the registry with tooltips.
struct ItemArgument;

impl ArgumentType for ItemArgument {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        let word = reader.read_unquoted_string().to_owned();
        if ITEMS.iter().any(|(name, _)| *name == word) {
            Ok(Arc::new(Item { name: word }))
        } else {
            Err(UNKNOWN_ITEM.create_with_context(reader, word))
        }
    }

    fn list_suggestions(&self, builder: SuggestionsBuilder) -> SuggestionsFuture {
        let remaining = builder.remaining_lowercase().to_owned();
        let mut builder = builder;
        for (name, tooltip) in ITEMS {
            if name.starts_with(&remaining) {
                builder =
                    builder.suggest_with_tooltip(name, Arc::new(LiteralMessage::new(tooltip)));
            }
        }
        builder.build_future()
    }

    fn examples(&self) -> Vec<String> {
        ITEMS.iter().map(|(name, _)| name.to_string()).collect()
    }
}

fn main() {
    let mut dispatcher = CommandDispatcher::<Player>::new();
    dispatcher.set_consumer(AuditLog);

    // --- registration -----------------------------------------------------

    // Admin-only: teleport. Bounded integer arguments reject out-of-range
    // input with Java's exact message.
    dispatcher.register(literal("tp").requires(|player: &Player| player.admin).then(
        argument("x", integer_range(0, 1000)).then(argument("y", integer_range(0, 1000)).executes(
            |ctx: &CommandContext<Player>| {
                println!(
                    "  tp {} -> ({}, {})",
                    ctx.get_source().name,
                    get_integer(ctx, "x"),
                    get_integer(ctx, "y")
                );
                1
            },
        )),
    ));

    // Admin-only: walk speed as a bounded float.
    dispatcher.register(
        literal("speed")
            .requires(|player: &Player| player.admin)
            .then(argument("mult", float_range(0.1, 10.0)).executes(
                |ctx: &CommandContext<Player>| {
                    println!(
                        "  speed of {} is now {}",
                        ctx.get_source().name,
                        get_float(ctx, "mult")
                    );
                    1
                },
            )),
    );

    // Everyone: `give <player> <item> [count]` — two custom argument types
    // and an optional trailing count (registering a deeper chain makes the
    // shorter input stay valid).
    dispatcher.register(
        literal("give").then(
            argument("player", PlayerNameArgument).then(
                argument("item", ItemArgument)
                    .then(argument("count", integer_range(1, 64)).executes(
                        |ctx: &CommandContext<Player>| {
                            give(ctx.get_source().name, ctx, get_integer(ctx, "count"))
                        },
                    ))
                    .executes(|ctx| give(ctx.get_source().name, ctx, 1)),
            ),
        ),
    );

    // Everyone chats; admins can also kick. Chat messages may contain
    // spaces, so the argument is a greedy string, not a single word.
    let say = dispatcher.register(
        literal("say").then(argument("msg", greedy_string()).executes(
            |ctx: &CommandContext<Player>| {
                println!(
                    "  [chat] {}: {}",
                    ctx.get_source().name,
                    get_string(ctx, "msg")
                );
                1
            },
        )),
    );

    dispatcher.register(
        literal("kick")
            .requires(|player: &Player| player.admin)
            .then(argument("player", PlayerNameArgument).executes_result(
                |ctx: &CommandContext<Player>| {
                    let target = &ctx.get_argument::<PlayerName>("player").0;
                    if target == ctx.get_source().name {
                        return Err(CANNOT_KICK_SELF.create());
                    }
                    println!("  kicked {target}");
                    Ok(1)
                },
            )),
    );

    // `tellall <msg>` forks into `say <msg>` once per other online player:
    // the modifier replaces the source with everyone except the sender.
    let everyone_else: Arc<RedirectModifier<Player, i32>> =
        Arc::new(|ctx: &CommandContext<Player, i32>| {
            let sender = ctx.get_source_arc();
            Ok(ONLINE_PLAYERS
                .iter()
                .filter(|name| **name != sender.name)
                .map(|name| {
                    Arc::new(Player {
                        name,
                        admin: name == &"Nova",
                    })
                })
                .collect())
        });
    dispatcher.register(literal("tellall").fork(say, everyone_else));

    // --- a few rounds of input ---------------------------------------------

    run(
        &dispatcher,
        "tp 50 60",
        Player {
            name: "Nova",
            admin: true,
        },
    );
    run(
        &dispatcher,
        "speed 2.5",
        Player {
            name: "Nova",
            admin: true,
        },
    );
    run(
        &dispatcher,
        "give Alice sword 2",
        Player {
            name: "Nova",
            admin: true,
        },
    );
    run(
        &dispatcher,
        "say hello world",
        Player {
            name: "Bob",
            admin: false,
        },
    );

    // Requirements hide admin commands from regular players — an
    // unrecognizable command reports Java's "Unknown command".
    run(
        &dispatcher,
        "tp 5 5",
        Player {
            name: "Bob",
            admin: false,
        },
    );

    // Custom errors carry the input snippet like every built-in one.
    run(
        &dispatcher,
        "kick Nova",
        Player {
            name: "Nova",
            admin: true,
        },
    );
    run(
        &dispatcher,
        "give Bob notanitem",
        Player {
            name: "Nova",
            admin: true,
        },
    );

    // Fork: three audit lines, one per delivered message.
    run(
        &dispatcher,
        "tellall meeting in 5",
        Player {
            name: "Bob",
            admin: false,
        },
    );

    // --- completions -------------------------------------------------------

    let admin = Player {
        name: "Nova",
        admin: true,
    };

    let parse = dispatcher.parse("give Al", admin.clone());
    let suggestions = block_on(CommandDispatcher::get_completion_suggestions(parse)).unwrap();
    println!("  completions for `give Al`:");
    print_suggestions(&suggestions);

    let parse = dispatcher.parse("give Alice ", admin);
    let suggestions = block_on(CommandDispatcher::get_completion_suggestions(parse)).unwrap();
    println!("  completions for `give Alice `:");
    print_suggestions(&suggestions);

    // --- usage / help output ----------------------------------------------

    let root = dispatcher.get_root();
    println!("  admin sees:");
    for usage in dispatcher.get_all_usage(
        &root.read(),
        &Player {
            name: "Nova",
            admin: true,
        },
        true,
    ) {
        println!("    {usage}");
    }
    println!("  regular player sees:");
    for usage in dispatcher.get_all_usage(
        &root.read(),
        &Player {
            name: "Bob",
            admin: false,
        },
        true,
    ) {
        println!("    {usage}");
    }
}

fn give(sender: &str, ctx: &CommandContext<Player, i32>, count: i32) -> i32 {
    let target = ctx.get_argument::<PlayerName>("player");
    let item = ctx.get_argument::<Item>("item");
    println!("  gave {count} {} to {}", item.name, target.0);
    println!("    (this ran for {sender})");
    1
}

fn run(dispatcher: &CommandDispatcher<Player>, input: &str, source: Player) {
    println!("> {input} (as {})", source.name);
    match dispatcher.execute(input, source) {
        Ok(result) => println!("  = {result}"),
        Err(error) => println!("  ! {}", error.get_message()),
    }
}

fn print_suggestions(suggestions: &brigaxsir::suggestion::Suggestions) {
    for suggestion in suggestions.list() {
        match suggestion.tooltip() {
            Some(tooltip) => println!("    {}  ({})", suggestion.text(), tooltip.get_string()),
            None => println!("    {}", suggestion.text()),
        }
    }
}

/// Minimal `block_on`, std-only: a channel-based waker. The library's own
/// futures complete on the first poll, so this never actually blocks. In a
/// real application you would await the future inside your own runtime
/// instead (`futures::executor::block_on`, tokio, smol, ...).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::{
        sync::mpsc,
        task::{Context, Poll, Wake, Waker},
    };

    struct ThreadWaker(mpsc::Sender<()>);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    let mut future = Box::pin(future);
    let (tx, rx) = mpsc::channel();
    let waker = Waker::from(Arc::new(ThreadWaker(tx)));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {
                if rx.recv().is_err() {
                    panic!("waker sender dropped before the future completed");
                }
            }
        }
    }
}
