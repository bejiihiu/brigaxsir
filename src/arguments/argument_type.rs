//! The trait every argument type implements. Java:
//! `com.mojang.brigadier.arguments.ArgumentType`.

use std::{any::Any, sync::Arc};

use crate::{
    context::ParsedValue,
    errors::CommandSyntaxException,
    string_reader::StringReader,
    suggestion::{Suggestions, SuggestionsBuilder, SuggestionsFuture},
};

/// How one argument is read out of the input.
///
/// Parsed values are stored type-erased (`Arc<dyn ParsedValue>`); the typed
/// getters (`get_integer`, `get_string`, ...) downcast them back. Java's typed
/// `ArgumentType<T>` generic becomes this untyped trait plus downcasts, so
/// heterogeneous argument types can live behind one `Arc`.
///
/// Note three divergences from Java's trait shape:
///
/// - `listSuggestions` there receives the `CommandContext<S>` (and mostly
///   ignores it). This trait is not generic over the source, so the context
///   is not passed — argument types that need it should route through a
///   custom [`SuggestionProvider`](crate::suggestion::SuggestionProvider)
///   instead.
/// - The source passed to [`Self::parse_with_source`] is type-erased to
///   `&(dyn Any + Send + Sync)`. Java's overload is generic (`<S> T parse`),
///   which would keep this trait from being used as `dyn ArgumentType` — and
///   the tree stores argument types behind exactly such an `Arc`. Type-erasing
///   the source matches the crate's existing convention (parsed values are
///   `Arc<dyn ParsedValue>` too); argument types that need the source downcast it.
/// - Java's erased value is any object; here it has to be a
///   [`ParsedValue`] — automatically true for every
///   `Any + Send + Sync + PartialEq + Hash` type, which is what
///   [`CommandContext::get_argument`](crate::context::CommandContext::get_argument)
///   (Java's `getArgument(name, Class)`) and the Java `equals`/`hashCode`
///   parity of the context need from a value.
pub trait ArgumentType {
    /// Consume this argument from `reader`, returning the parsed value.
    /// Should leave `reader`'s cursor just past the argument. Java:
    /// `parse(StringReader)`.
    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException>;

    /// The erased view used by the type-equality hook. Every argument type
    /// implements it as `self` — the standard Rust downcast dance for trait
    /// objects. Java needs no such method (its `ArgumentType<T>` generics
    /// keep the concrete type around); the erased tree compares types
    /// through [`Self::type_equals`] instead.
    fn as_any(&self) -> &dyn Any;

    /// Structural equality against another erased argument type. Java
    /// compares `ArgumentCommandNode`s by `type.equals(that.type)` on the
    /// concrete type objects; with erased types this hook dispatches into
    /// that comparison.
    ///
    /// The default is reference equality (`Object.equals` semantics for
    /// types that do not override `equals`, like the string/bool argument
    /// types). Value-equal built-ins (the bounded numeric types) override it
    /// to compare through their `PartialEq`.
    fn type_equals(&self, other: &dyn ArgumentType) -> bool {
        let this_ptr: *const dyn Any = self.as_any();
        let other_ptr: *const dyn Any = other.as_any();
        // same data pointer = same object = Java's reference equality; the
        // vtable part is irrelevant (same allocation means same type)
        this_ptr.addr() == other_ptr.addr()
    }

    /// Source-aware variant of [`Self::parse`]. Java:
    /// `default <S> T parse(StringReader reader, S source)` — the default
    /// implementation ignores the source and calls `parse(reader)`, and so
    /// does this one. Override when the argument's meaning depends on the
    /// command source (Minecraft's `EntityArgumentType` style).
    ///
    /// The source arrives as a type-erased `&dyn Any`; downcast it to the
    /// concrete source type with `source.downcast_ref::<MySource>()`. Callers
    /// holding a `&S` need `S: Send + Sync + 'static` to coerce — the same
    /// bounds the crate already puts on command sources.
    fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &(dyn Any + Send + Sync),
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        let _ = source;
        self.parse(reader)
    }

    /// Suggest possible values for this argument at the current position.
    /// Java: `listSuggestions(SuggestionsBuilder)` — the Java default is
    /// `Suggestions.empty()`, i.e. nothing to offer.
    fn list_suggestions(&self, _builder: SuggestionsBuilder) -> SuggestionsFuture {
        Suggestions::empty()
    }

    /// Example values for usage/help output. Java: `getExamples`.
    fn examples(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arguments::bool_argument_type::bool;

    /// A type whose contextual parse reads the source, the way a custom
    /// source-dependent argument type would.
    struct SourceReading;
    impl ArgumentType for SourceReading {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn parse(
            &self,
            _reader: &mut StringReader,
        ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
            Ok(Arc::new(String::from("no source")))
        }

        fn parse_with_source(
            &self,
            _reader: &mut StringReader,
            source: &(dyn Any + Send + Sync),
        ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
            Ok(Arc::new(
                source
                    .downcast_ref::<i32>()
                    .map(|v| format!("source {v}"))
                    .unwrap_or_else(|| "unknown source".to_owned()),
            ))
        }
    }

    #[test]
    fn default_parse_with_source_ignores_the_source() {
        // Java's default: parse(reader, source) == parse(reader)
        let mut reader = StringReader::from("true");
        let value = bool().parse_with_source(&mut reader, &42i32).unwrap();
        assert_eq!(value.as_any().downcast_ref::<bool>(), Some(&true));
    }

    #[test]
    fn overriding_parse_with_source_sees_the_erased_source() {
        let mut reader = StringReader::from("ignored");
        let value = SourceReading.parse_with_source(&mut reader, &7i32).unwrap();
        assert_eq!(
            value.as_any().downcast_ref::<String>(),
            Some(&"source 7".to_owned())
        );
    }
}
