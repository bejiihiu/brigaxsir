//! The required-argument payload of a command node. Java:
//! `com.mojang.brigadier.tree.ArgumentCommandNode` — there it is a full node
//! subclass; here the tree keeps one `CommandNode` type and this struct is
//! the data of the argument kind (see
//! [`CommandNodeKind`](crate::tree::CommandNodeKind)).

use std::{any::Any, fmt, sync::Arc};

use crate::{
    arguments::ArgumentType,
    context::{CommandContext, ParsedValue},
    errors::CommandSyntaxException,
    string_reader::StringReader,
    suggestion::{SuggestionProvider, SuggestionsBuilder, SuggestionsFuture},
};

/// The data of a required-argument node: the name the value is stored under,
/// the parser, and the optional custom suggestion provider. Java:
/// `ArgumentCommandNode`'s `name`/`type`/`customSuggestions` fields.
pub struct ArgumentCommandNode<S, R = i32> {
    name: String,
    parser: Arc<dyn ArgumentType + Send + Sync>,
    // the argument-builder flips this through `suggests()`
    pub(crate) custom_suggestions: Option<Arc<dyn SuggestionProvider<S, R> + Send + Sync>>,
}

impl<S, R> ArgumentCommandNode<S, R> {
    /// An argument payload named `name`, parsed by `parser`, with optional
    /// custom suggestions. Java: `ArgumentCommandNode(name, type, ...,
    /// customSuggestions)` constructor.
    pub fn new(
        name: &str,
        parser: Arc<dyn ArgumentType + Send + Sync>,
        custom_suggestions: Option<Arc<dyn SuggestionProvider<S, R> + Send + Sync>>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            parser,
            custom_suggestions,
        }
    }

    /// The name the parsed value is stored under. Java: `getName`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// The parser for this argument. Java: `getType` returning the
    /// `ArgumentType<T>`; the type is erased behind the same `Arc` the tree
    /// stores, so identity (`Arc::ptr_eq`) survives.
    pub fn get_type(&self) -> &Arc<dyn ArgumentType + Send + Sync> {
        &self.parser
    }

    /// The custom suggestion provider, if any. Java: `getCustomSuggestions`.
    pub fn get_custom_suggestions(
        &self,
    ) -> Option<&Arc<dyn SuggestionProvider<S, R> + Send + Sync>> {
        self.custom_suggestions.as_ref()
    }

    /// Parse this argument out of `reader`, producing the type-erased value
    /// stored in the context. Java: `type.parse(reader)`.
    pub fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        self.parser.parse(reader)
    }

    /// Source-aware variant of [`Self::parse`], handed straight to the
    /// argument type. Java: `ArgumentCommandNode#parse` calls
    /// `type.parse(reader, source)`; the tree routes the context's source
    /// through here.
    pub fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &(dyn Any + Send + Sync),
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        self.parser.parse_with_source(reader, source)
    }

    /// Produce suggestions for this argument. Custom suggestions win when
    /// present; otherwise the parser's own suggestions are used, matching
    /// Java's `listSuggestions` priority.
    pub fn list_suggestions(
        &self,
        context: CommandContext<S, R>,
        builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        if let Some(s) = &self.custom_suggestions {
            s.get_suggestions(context, builder)
        } else {
            self.parser.list_suggestions(builder)
        }
    }

    /// Whether `input` parses fully as this argument (ending at a word
    /// boundary). Java: `isValidInput` — parse, then
    /// `!reader.canRead() || reader.peek() == ' '`; a parse error means
    /// `false`.
    pub(crate) fn is_valid_input(&self, input: &str) -> bool {
        let mut reader = StringReader::from(input);
        match self.parser.parse(&mut reader) {
            Ok(_) => !reader.can_read() || reader.peek() == ' ',
            Err(_) => false,
        }
    }

    /// Example values advertised for usage/help output. Java: `getExamples`
    /// delegates to the argument type.
    pub(crate) fn examples(&self) -> Vec<String> {
        self.parser.examples()
    }

    /// Sort key for the Comparable ordering. Java: `getSortedKey` returns
    /// `name`.
    pub(crate) fn sorted_key(&self) -> &str {
        &self.name
    }
}

impl<S, R> Clone for ArgumentCommandNode<S, R> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            parser: self.parser.clone(),
            custom_suggestions: self.custom_suggestions.clone(),
        }
    }
}

impl<S, R> PartialEq for ArgumentCommandNode<S, R> {
    /// Java: `name.equals(that.name) && type.equals(that.type)`. The erased
    /// parser is compared through the [`ArgumentType::type_equals`] hook,
    /// which dispatches into the concrete type's equality (Java's `equals`
    /// on the erased object).
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.parser.type_equals(other.parser.as_ref())
    }
}

impl<S, R> fmt::Debug for ArgumentCommandNode<S, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArgumentCommandNode")
            .field("name", &self.name)
            .finish()
    }
}
