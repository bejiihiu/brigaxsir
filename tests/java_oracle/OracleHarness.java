import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.LiteralMessage;
import com.mojang.brigadier.Message;
import com.mojang.brigadier.ParseResults;
import com.mojang.brigadier.RedirectModifier;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.arguments.BoolArgumentType;
import com.mojang.brigadier.arguments.DoubleArgumentType;
import com.mojang.brigadier.arguments.FloatArgumentType;
import com.mojang.brigadier.arguments.IntegerArgumentType;
import com.mojang.brigadier.arguments.LongArgumentType;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.builder.ArgumentBuilder;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.brigadier.builder.RequiredArgumentBuilder;
import com.mojang.brigadier.context.CommandContext;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.brigadier.suggestion.Suggestion;
import com.mojang.brigadier.suggestion.SuggestionProvider;
import com.mojang.brigadier.suggestion.Suggestions;
import com.mojang.brigadier.tree.CommandNode;
import com.mojang.brigadier.tree.RootCommandNode;

import java.io.BufferedReader;
import java.io.BufferedOutputStream;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.Predicate;

/**
 * Differential-testing harness for brigaxsir-rs (task 20 golden oracle).
 *
 * <p>Reads scenario lines on stdin (UTF-8), builds one Brigadier command tree
 * per scenario, runs the requested dispatcher operations, and prints a
 * structured result block per scenario on stdout (UTF-8). The wire protocol is
 * documented in README.md next to this file. The only dependency is the
 * library under test itself.
 */
public class OracleHarness {

    private static final class Fail extends RuntimeException {
        Fail(String message) {
            super(message);
        }
    }

    private static <T> T fail(String message) {
        throw new Fail(message);
    }

    /** One path segment: a literal keyword or a typed argument. */
    private static final class Segment {
        final boolean literal;
        final String name;
        final String type;
        final String min;
        final String max;

        Segment(boolean literal, String name, String type, String min, String max) {
            this.literal = literal;
            this.name = name;
            this.type = type;
            this.min = min;
            this.max = max;
        }

        static Segment parse(String raw) {
            if (!raw.startsWith("@")) {
                return new Segment(true, raw, null, null, null);
            }
            final String[] parts = raw.substring(1).split(":", -1);
            if (parts.length < 2 || parts.length > 4) {
                return fail("bad argument segment: " + raw);
            }
            String min = null;
            String max = null;
            if (parts.length >= 3 && !parts[2].equals("-")) {
                min = parts[2];
            }
            if (parts.length >= 4 && !parts[3].equals("-")) {
                max = parts[3];
            }
            return new Segment(false, parts[1], parts[0], min, max);
        }
    }

    public static void main(String[] args) throws Exception {
        final BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        final PrintStream out = new PrintStream(new BufferedOutputStream(new FileOutputStream(FileDescriptor.out), 1 << 16), false, StandardCharsets.UTF_8);

        try {
            String line = in.readLine();
            if (!"BRIGAXSIR-ORACLE v1".equals(line)) {
                out.println("ERR bad handshake: " + line);
                out.flush();
                return;
            }
            out.println("OK BRIGAXSIR-ORACLE v1");
            out.flush();

            String scenarioId = null;
            List<String> nodeSpecs = new ArrayList<>();
            List<String> opSpecs = new ArrayList<>();
            while ((line = in.readLine()) != null) {
                if (line.equals("DONE")) {
                    break;
                } else if (line.equals("E")) {
                    runScenario(out, scenarioId, nodeSpecs, opSpecs);
                    scenarioId = null;
                    nodeSpecs = new ArrayList<>();
                    opSpecs = new ArrayList<>();
                } else if (line.startsWith("S ")) {
                    scenarioId = line.substring(2);
                } else if (line.startsWith("N ")) {
                    nodeSpecs.add(line.substring(2));
                } else if (line.startsWith("O ")) {
                    opSpecs.add(line.substring(2));
                } else {
                    fail("unexpected request line: " + line);
                }
            }
            out.println("DONE");
            out.flush();
        } catch (final Fail f) {
            out.println("ERR " + f.getMessage());
            out.flush();
            System.exit(1);
        }
    }

    private static void runScenario(PrintStream out, String id, List<String> nodeSpecs, List<String> opSpecs) {
        final CommandDispatcher<Integer> dispatcher = new CommandDispatcher<>();
        final RootCommandNode<Integer> root = dispatcher.getRoot();
        final Map<String, CommandNode<Integer>> byPath = new HashMap<>();
        final Map<CommandNode<Integer>, String> reverse = new IdentityHashMap<>();

        out.println("S " + id);
        for (final String spec : nodeSpecs) {
            buildNode(dispatcher, root, byPath, reverse, spec);
        }
        for (final String op : opSpecs) {
            runOp(out, dispatcher, root, byPath, reverse, op);
        }
        out.println("E");
        out.flush();
    }

    // ------------------------------------------------------------------
    // tree building
    // ------------------------------------------------------------------

    private static void buildNode(CommandDispatcher<Integer> dispatcher, RootCommandNode<Integer> root,
                                  Map<String, CommandNode<Integer>> byPath, Map<CommandNode<Integer>, String> reverse,
                                  String spec) {
        final String[] parts = spec.split(" ");
        final String path = parts[0];
        final Map<String, String> flags = new HashMap<>();
        for (int i = 1; i < parts.length; i++) {
            final int eq = parts[i].indexOf('=');
            if (eq <= 0) {
                fail("bad node flag: " + parts[i]);
            }
            flags.put(parts[i].substring(0, eq), parts[i].substring(eq + 1));
        }

        final List<Segment> segments = new ArrayList<>();
        for (final String raw : path.split("/")) {
            segments.add(Segment.parse(raw));
        }

        // The deepest segment carries the flags; parents chain up with then()
        // (which builds eagerly, exactly like the Rust port's then_built).
        ArgumentBuilder<Integer, ?> current = lastBuilder(segments.get(segments.size() - 1), flags, byPath, root);
        for (int i = segments.size() - 2; i >= 1; i--) {
            final ArgumentBuilder<Integer, ?> parent = baseBuilder(segments.get(i));
            parent.then(current);
            current = parent;
        }

        if (segments.size() == 1) {
            // current is already the top builder (flags included)
            attachTop(dispatcher, root, byPath, reverse, segments.get(0), current, path, segments);
        } else {
            final ArgumentBuilder<Integer, ?> top = baseBuilder(segments.get(0));
            top.then(current);
            attachTop(dispatcher, root, byPath, reverse, segments.get(0), top, path, segments);
        }
    }

    private static void attachTop(CommandDispatcher<Integer> dispatcher, RootCommandNode<Integer> root,
                                  Map<String, CommandNode<Integer>> byPath, Map<CommandNode<Integer>, String> reverse,
                                  Segment first, ArgumentBuilder<Integer, ?> top, String path, List<Segment> segments) {
        if (first.literal) {
            @SuppressWarnings("unchecked")
            final LiteralArgumentBuilder<Integer> literal = (LiteralArgumentBuilder<Integer>) top;
            dispatcher.register(literal);
        } else {
            // argument at the root: build and addChild directly
            root.addChild(top.build());
        }
        // record every node along the path (intermediates too: getSmartUsage
        // keys are the direct children, which are usually the top literals)
        final String[] rawSegments = path.split("/");
        CommandNode<Integer> walk = root;
        final StringBuilder prefix = new StringBuilder();
        for (final String raw : rawSegments) {
            walk = walk.getChild(Segment.parse(raw).name);
            if (walk == null) {
                fail("node missing after registration: " + raw);
            }
            if (prefix.length() > 0) {
                prefix.append('/');
            }
            prefix.append(raw);
            reverse.putIfAbsent(walk, prefix.toString());
            byPath.put(prefix.toString(), walk);
        }
    }

    private static CommandNode<Integer> resolve(RootCommandNode<Integer> root, List<Segment> segments) {
        CommandNode<Integer> node = root;
        for (final Segment s : segments) {
            node = node.getChild(s.name);
            if (node == null) {
                return fail("node missing after registration: " + s.name);
            }
        }
        return node;
    }

    private static ArgumentBuilder<Integer, ?> baseBuilder(Segment s) {
        if (s.literal) {
            return LiteralArgumentBuilder.<Integer>literal(s.name);
        }
        return RequiredArgumentBuilder.argument(s.name, typeFor(s));
    }

    private static ArgumentBuilder<Integer, ?> lastBuilder(Segment s, Map<String, String> flags,
                                                           Map<String, CommandNode<Integer>> byPath,
                                                           RootCommandNode<Integer> root) {
        ArgumentBuilder<Integer, ?> b = baseBuilder(s);
        if (!s.literal && flags.containsKey("sug")) {
            @SuppressWarnings("unchecked")
            final RequiredArgumentBuilder<Integer, ?> arg = (RequiredArgumentBuilder<Integer, ?>) b;
            arg.suggests(providerFor(flags.get("sug")));
        }
        if (flags.containsKey("exec")) {
            b.executes(commandFor(flags.get("exec")));
        }
        if (flags.containsKey("req")) {
            b.requires(requirementFor(flags.get("req")));
        }
        if (flags.containsKey("red")) {
            final String targetSpec = flags.get("red");
            final CommandNode<Integer> target = targetSpec.equals(".")
                ? root
                : Objects.requireNonNull(byPath.get(targetSpec), "redirect target not defined yet: " + targetSpec);
            RedirectModifierHolder h = new RedirectModifierHolder();
            if (flags.containsKey("mod")) {
                final String m = flags.get("mod");
                if (m.equals("identity")) {
                    h.modifier = context -> Collections.singleton(context.getSource());
                } else if (m.startsWith("dup:")) {
                    final int n = Integer.parseInt(m.substring(4));
                    h.modifier = context -> {
                        final List<Integer> out = new ArrayList<>();
                        for (int i = 0; i < n; i++) {
                            out.add(context.getSource() + i);
                        }
                        return out;
                    };
                } else if (m.equals("empty")) {
                    h.modifier = context -> Collections.emptyList();
                } else {
                    fail("bad redirect modifier: " + m);
                }
            }
            final boolean fork = "1".equals(flags.get("fork"));
            b.forward(target, h.modifier, fork);
        }
        return b;
    }

    /** holder so the lambda assignments above keep their exact types */
    private static final class RedirectModifierHolder {
        com.mojang.brigadier.RedirectModifier<Integer> modifier;
    }

    private static com.mojang.brigadier.Command<Integer> commandFor(String spec) {
        if (spec.startsWith("int:")) {
            final String name = spec.substring(4);
            return context -> context.getArgument(name, Integer.class);
        }
        if (spec.startsWith("long:")) {
            final String name = spec.substring(5);
            return context -> (int) (long) context.getArgument(name, Long.class);
        }
        if (spec.startsWith("strlen:")) {
            final String name = spec.substring(7);
            return context -> context.getArgument(name, String.class).length();
        }
        if (spec.startsWith("bool:")) {
            final String name = spec.substring(5);
            return context -> Boolean.TRUE.equals(context.getArgument(name, Boolean.class)) ? 1 : 0;
        }
        final int value = Integer.parseInt(spec);
        return context -> value;
    }

    private static Predicate<Integer> requirementFor(String spec) {
        if (spec.equals("true")) {
            return s -> true;
        }
        if (spec.equals("false")) {
            return s -> false;
        }
        if (spec.startsWith("eq:")) {
            final int n = Integer.parseInt(spec.substring(3));
            return s -> s == n;
        }
        return fail("bad requirement: " + spec);
    }

    private static ArgumentType<?> typeFor(Segment s) {
        switch (s.type) {
            case "int": {
                if (s.min == null && s.max == null) {
                    return IntegerArgumentType.integer();
                }
                if (s.max == null) {
                    return IntegerArgumentType.integer(Integer.parseInt(s.min));
                }
                return IntegerArgumentType.integer(Integer.parseInt(s.min), Integer.parseInt(s.max));
            }
            case "long": {
                if (s.min == null && s.max == null) {
                    return LongArgumentType.longArg();
                }
                if (s.max == null) {
                    return LongArgumentType.longArg(Long.parseLong(s.min));
                }
                return LongArgumentType.longArg(Long.parseLong(s.min), Long.parseLong(s.max));
            }
            case "float": {
                if (s.min == null && s.max == null) {
                    return FloatArgumentType.floatArg();
                }
                if (s.max == null) {
                    return FloatArgumentType.floatArg(Float.parseFloat(s.min));
                }
                return FloatArgumentType.floatArg(Float.parseFloat(s.min), Float.parseFloat(s.max));
            }
            case "double": {
                if (s.min == null && s.max == null) {
                    return DoubleArgumentType.doubleArg();
                }
                if (s.max == null) {
                    return DoubleArgumentType.doubleArg(Double.parseDouble(s.min));
                }
                return DoubleArgumentType.doubleArg(Double.parseDouble(s.min), Double.parseDouble(s.max));
            }
            case "bool":
                return BoolArgumentType.bool();
            case "word":
                return StringArgumentType.word();
            case "string":
                return StringArgumentType.string();
            case "greedy":
                return StringArgumentType.greedyString();
            default:
                return fail("unknown argument type: " + s.type);
        }
    }

    private static SuggestionProvider<Integer> providerFor(String spec) {
        if (!spec.startsWith("list:")) {
            return fail("bad suggestion provider: " + spec);
        }
        final String[] items = spec.substring(5).split("\\|", -1);
        return (context, builder) -> {
            for (final String item : items) {
                // "i:<int>" items exercise SuggestionsBuilder#suggest(int) and
                // the IntegerSuggestion subtype (tooltip after '=')
                if (item.startsWith("i:")) {
                    final int eq = item.indexOf('=');
                    if (eq >= 0) {
                        builder.suggest(Integer.parseInt(item.substring(2, eq)),
                            new LiteralMessage(item.substring(eq + 1)));
                    } else {
                        builder.suggest(Integer.parseInt(item.substring(2)));
                    }
                } else {
                    final int eq = item.indexOf('=');
                    if (eq >= 0) {
                        builder.suggest(item.substring(0, eq), new LiteralMessage(item.substring(eq + 1)));
                    } else {
                        builder.suggest(item);
                    }
                }
            }
            return builder.buildFuture();
        };
    }

    // ------------------------------------------------------------------
    // operations
    // ------------------------------------------------------------------

    private static CommandNode<Integer> resolveNode(RootCommandNode<Integer> root,
                                                    Map<String, CommandNode<Integer>> byPath, String spec) {
        if (spec.equals(".")) {
            return root;
        }
        final CommandNode<Integer> node = byPath.get(spec);
        if (node == null) {
            return fail("op references unknown node path: " + spec);
        }
        return node;
    }

    private static void runOp(PrintStream out, CommandDispatcher<Integer> dispatcher, RootCommandNode<Integer> root,
                              Map<String, CommandNode<Integer>> byPath, Map<CommandNode<Integer>, String> reverse,
                              String op) {
        final String[] head = op.split(" ", 2);
        final String kind = head[0];
        final String rest = head.length > 1 ? head[1] : "";
        switch (kind) {
            case "parse": {
                final int sp = rest.indexOf(' ');
                if (sp < 0) {
                    fail("bad parse op: " + op);
                }
                final int source = Integer.parseInt(rest.substring(0, sp));
                final String input = sp + 1 < rest.length() ? rest.substring(sp + 1) : "";
                final ParseResults<Integer> parse = dispatcher.parse(input, source);
                out.println("X " + parse.getExceptions().size());
                for (final Map.Entry<CommandNode<Integer>, CommandSyntaxException> e : parse.getExceptions().entrySet()) {
                    final CommandSyntaxException ex = e.getValue();
                    out.println("Xe " + e.getKey().getName() + " " + ex.getCursor());
                    final String context = ex.getContext();
                    out.println("Xc " + (context == null ? "-" : context));
                    out.println("Xm " + ex.getRawMessage().getString());
                }
                out.println("Xr " + parse.getReader().getCursor() + " " + (parse.getReader().canRead() ? 1 : 0));
                out.println("Xg " + parse.getContext().getRange().getStart() + " " + parse.getContext().getRange().getEnd());
                break;
            }
            case "exec": {
                final int sp = rest.indexOf(' ');
                if (sp < 0) {
                    fail("bad exec op: " + op);
                }
                final int source = Integer.parseInt(rest.substring(0, sp));
                final String input = sp + 1 < rest.length() ? rest.substring(sp + 1) : "";
                try {
                    out.println("R " + dispatcher.execute(input, source));
                } catch (final CommandSyntaxException ex) {
                    out.println("Rerr " + ex.getCursor());
                    final String context = ex.getContext();
                    out.println("Rc " + (context == null ? "-" : context));
                    out.println("Rm " + ex.getRawMessage().getString());
                }
                break;
            }
            case "sug": {
                final int sp1 = rest.indexOf(' ');
                final int sp2 = sp1 < 0 ? -1 : rest.indexOf(' ', sp1 + 1);
                if (sp1 < 0 || sp2 < 0) {
                    fail("bad sug op: " + op);
                }
                final int source = Integer.parseInt(rest.substring(0, sp1));
                final int cursor = Integer.parseInt(rest.substring(sp1 + 1, sp2));
                final String input = rest.substring(sp2 + 1);
                final ParseResults<Integer> parse = dispatcher.parse(input, source);
                final Suggestions suggestions = dispatcher.getCompletionSuggestions(parse, cursor).join();
                out.println("Y " + suggestions.getRange().getStart() + " " + suggestions.getRange().getEnd());
                for (final Suggestion s : suggestions.getList()) {
                    out.println("Ys " + s.getText());
                    final Message tooltip = s.getTooltip();
                    out.println("Yt " + (tooltip == null ? "-" : tooltip.getString()));
                }
                break;
            }
            case "usage": {
                final String[] t = rest.split(" ");
                if (t.length != 3) {
                    fail("bad usage op: " + op);
                }
                final boolean restricted = t[0].equals("1");
                final int source = Integer.parseInt(t[1]);
                final CommandNode<Integer> node = resolveNode(root, byPath, t[2]);
                final String[] usage = dispatcher.getAllUsage(node, source, restricted);
                out.println("U " + usage.length);
                for (final String u : usage) {
                    out.println("Us " + u);
                }
                break;
            }
            case "smart": {
                final String[] t = rest.split(" ");
                if (t.length != 2) {
                    fail("bad smart op: " + op);
                }
                final int source = Integer.parseInt(t[0]);
                final CommandNode<Integer> node = resolveNode(root, byPath, t[1]);
                final Map<CommandNode<Integer>, String> smart = dispatcher.getSmartUsage(node, source);
                out.println("M " + smart.size());
                for (final Map.Entry<CommandNode<Integer>, String> e : smart.entrySet()) {
                    final String childPath = reverse.get(e.getKey());
                    if (childPath == null) {
                        fail("smart usage key has no spec path");
                    }
                    out.println("Ms " + childPath + " " + e.getValue());
                }
                break;
            }
            case "path": {
                final CommandNode<Integer> node = resolveNode(root, byPath, rest);
                final List<String> path = new ArrayList<>(dispatcher.getPath(node));
                out.println("P " + (path.isEmpty() ? "-" : String.join(",", path)));
                break;
            }
            case "find": {
                final List<String> names = new ArrayList<>();
                if (!rest.equals("-")) {
                    names.addAll(Arrays.asList(rest.split(",", -1)));
                }
                out.println("F " + (dispatcher.findNode(names) == null ? 0 : 1));
                break;
            }
            default:
                fail("unknown op kind: " + kind);
        }
    }
}
