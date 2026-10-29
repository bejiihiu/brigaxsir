//! The self-describing erased value stored by [`ParsedArgument`](super::ParsedArgument).//!
//! Java's `ParsedArgument<S, T>` knows its value type `T` at compile time and
//! `CommandContext#getArgument(name, Class)` checks it with
//! `Class.isAssignableFrom`, rendering `result.getClass().getSimpleName()` in
//! the mismatch message. The crate erases parsed values behind one `Arc`, so
//! the value object has to carry what the erasure would otherwise lose: its
//! type name (for the Java-style error messages) and its value equality/hash
//! (Java's `CommandContext.equals` compares the parsed-argument maps, and
//! `ParsedArgument.equals`/`hashCode` compare the result objects).

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    hash::Hasher,
    sync::LazyLock,
};

/// Java-style display names for the value types the built-in argument types
/// produce. Java renders these from `getClass().getSimpleName()` (`Integer`,
/// `Long`, `Double`, `Float`, `Boolean`, `String`); the Rust types are
/// `i32`, `i64`, `f64`, `f32`, `bool` and `String`, so the names are keyed by
/// [`TypeId`] here instead of coming from the runtime class. This is the
/// single source for those names — the built-in [`ParsedValue`] impls read it
/// too, so the stored value's name and the requested type's name in an error
/// message always agree.
static JAVA_STYLE_VALUE_NAMES: LazyLock<HashMap<TypeId, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        (TypeId::of::<i32>(), "Integer"),
        (TypeId::of::<i64>(), "Long"),
        (TypeId::of::<f64>(), "Double"),
        (TypeId::of::<f32>(), "Float"),
        (TypeId::of::<bool>(), "Boolean"),
        (TypeId::of::<String>(), "String"),
    ])
});

/// The last path segment of a `std::any::type_name` string, best-effort.
///
/// Cuts at the rightmost `::` that sits outside any `<...>` or `(...)`, so
/// `alloc::string::String` becomes `String` while generic tails like
/// `alloc::vec::Vec<alloc::string::String>` keep their arguments visible
/// (`Vec<alloc::string::String>`). This only ever feeds fallback names for
/// types outside [`JAVA_STYLE_VALUE_NAMES`] — the exact rendering of a
/// non-built-in type is best-effort by design.
fn simple_name(full: &'static str) -> &'static str {
    let bytes = full.as_bytes();
    let mut depth = 0i32;
    let mut cut = 0usize;
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'>' | b')' => depth += 1,
            b'<' | b'(' => depth -= 1,
            b':' if depth == 0 && i > 0 && bytes[i - 1] == b':' => {
                cut = i + 1;
                break;
            }
            _ => {}
        }
    }
    &full[cut..]
}

/// The Java-style simple name of `T`: the [`JAVA_STYLE_VALUE_NAMES`] entry
/// when `T` is one of the built-in parsed value types, otherwise the
/// best-effort [`simple_name`] of `std::any::type_name`.
pub(crate) fn display_name_of<T: ?Sized + 'static>() -> &'static str {
    match JAVA_STYLE_VALUE_NAMES.get(&TypeId::of::<T>()) {
        Some(name) => name,
        None => simple_name(std::any::type_name::<T>()),
    }
}

/// A parsed argument value: type-erased, but still able to name and compare
/// itself, which is what
/// [`ParsedArgument`](crate::context::ParsedArgument) (and through it
/// `CommandContext#getArgument`) needs.
///
/// The six built-in value types (`i32`, `i64`, `f64`, `f32`, `bool`,
/// `String`) implement it below — the same explicit downcast dance the
/// [`ArgumentType`](crate::arguments::ArgumentType) trait already asks of
/// every parser (its `as_any`). A custom argument type's result implements
/// it the same way, one small method each:
///
/// ```text
/// impl ParsedValue for MyValue {
///     fn as_any(&self) -> &dyn Any { self }
///     fn type_name(&self) -> &'static str { display_name_of::<Self>() }
///     fn value_eq(&self, other: &dyn ParsedValue) -> bool {
///         other.as_any().downcast_ref::<Self>() == Some(self)
///     }
///     fn value_hash(&self, mut state: &mut dyn Hasher) {
///         self.hash(&mut state) // `&mut dyn Hasher` is itself a `Hasher`
///     }
/// }
/// ```
///
/// There is deliberately no blanket impl over all `T: PartialEq`: the float
/// types must NOT ride it — std gives them no `Hash`, and their boxed
/// equality is Java's, not Rust's — Java's `Double.equals`/`Float.equals`
/// compare the raw bits, so `NaN` equals `NaN` and `0.0` differs from
/// `-0.0`, the opposite of Rust's IEEE `PartialEq` for the same types.
/// Parsed values are compared the Java way (`CommandContext.equals` walks
/// the parsed-argument maps), so the parsed floats hash and compare through
/// their bits.
pub trait ParsedValue: Any + Send + Sync {
    /// The value as `&dyn Any`, for downcasting back to the concrete type.
    /// Java needs no such method (its generic `T` is right there); this is
    /// the standard Rust dance for trait objects.
    fn as_any(&self) -> &dyn Any;

    /// The Java-style simple name of this value's concrete type. Feeds the
    /// `Argument '<name>' is defined as <X>, not <Y>` message, where Java
    /// prints `result.getClass().getSimpleName()`.
    fn type_name(&self) -> &'static str;

    /// Value equality against another erased value. Java:
    /// `Objects.equals(result, that.result)` — different concrete types are
    /// simply not equal.
    fn value_eq(&self, other: &dyn ParsedValue) -> bool;

    /// Value hash, the [`Hash`] half of `Objects.hashCode(result)`.
    fn value_hash(&self, state: &mut dyn Hasher);
}

impl ParsedValue for i32 {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        display_name_of::<Self>()
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<i32>() == Some(self)
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write_i32(*self);
    }
}

impl ParsedValue for i64 {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        display_name_of::<Self>()
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<i64>() == Some(self)
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write_i64(*self);
    }
}

impl ParsedValue for f64 {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        display_name_of::<Self>()
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        match other.as_any().downcast_ref::<f64>() {
            Some(other) => self.to_bits() == other.to_bits(),
            None => false,
        }
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write_u64(self.to_bits());
    }
}

impl ParsedValue for f32 {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        display_name_of::<Self>()
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        match other.as_any().downcast_ref::<f32>() {
            Some(other) => self.to_bits() == other.to_bits(),
            None => false,
        }
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write_u32(self.to_bits());
    }
}

impl ParsedValue for bool {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        display_name_of::<Self>()
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<bool>() == Some(self)
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write_u8(*self as u8);
    }
}

impl ParsedValue for String {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_name(&self) -> &'static str {
        display_name_of::<Self>()
    }
    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<String>() == Some(self)
    }
    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write(self.as_bytes());
    }
}

impl fmt::Debug for dyn ParsedValue {
    /// The erased value cannot render itself, so the debug output names the
    /// concrete type — which is also everything `try_get_argument`'s error
    /// message ever needs from it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ParsedValue {{ type: {} }}", self.type_name())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn built_in_value_names_are_java_style() {
        // the exact `getSimpleName` strings the Java error messages use
        assert_eq!(display_name_of::<i32>(), "Integer");
        assert_eq!(display_name_of::<i64>(), "Long");
        assert_eq!(display_name_of::<f64>(), "Double");
        assert_eq!(display_name_of::<f32>(), "Float");
        assert_eq!(display_name_of::<bool>(), "Boolean");
        assert_eq!(display_name_of::<String>(), "String");
    }

    #[test]
    fn custom_value_names_fall_back_to_the_rust_simple_name() {
        struct CustomValue;
        assert_eq!(display_name_of::<CustomValue>(), "CustomValue");
        // generics keep their shape; only the module path is stripped
        assert_eq!(display_name_of::<Vec<i32>>(), "Vec<i32>");
    }

    #[test]
    fn value_eq_and_hash_agree_with_the_concrete_type() {
        let a: Arc<dyn ParsedValue> = Arc::new(123i32);
        let b: Arc<dyn ParsedValue> = Arc::new(123i32);
        let c: Arc<dyn ParsedValue> = Arc::new(124i32);
        let s: Arc<dyn ParsedValue> = Arc::new("123".to_owned());

        assert!(a.value_eq(b.as_ref()));
        assert!(b.value_eq(a.as_ref()));
        assert!(!a.value_eq(c.as_ref()));
        // different concrete types are never equal, even with equal text
        assert!(!a.value_eq(s.as_ref()));

        let hash = |v: &Arc<dyn ParsedValue>| {
            let mut state = std::collections::hash_map::DefaultHasher::new();
            v.value_hash(&mut state);
            state.finish()
        };
        assert_eq!(hash(&a), hash(&b));
    }

    #[test]
    fn floats_evaluate_like_java_boxed_doubles() {
        // Java's Double.equals: NaN equals NaN, 0.0 and -0.0 differ — the
        // exact opposite of Rust's IEEE `PartialEq` for the same values
        let nan_a: Arc<dyn ParsedValue> = Arc::new(f64::NAN);
        let nan_b: Arc<dyn ParsedValue> = Arc::new(f64::NAN);
        assert!(nan_a.value_eq(nan_b.as_ref()));

        let zero: Arc<dyn ParsedValue> = Arc::new(0.0f64);
        let neg_zero: Arc<dyn ParsedValue> = Arc::new(-0.0f64);
        assert!(!zero.value_eq(neg_zero.as_ref()));

        let f32_nan_a: Arc<dyn ParsedValue> = Arc::new(f32::NAN);
        let f32_nan_b: Arc<dyn ParsedValue> = Arc::new(f32::NAN);
        assert!(f32_nan_a.value_eq(f32_nan_b.as_ref()));
        assert!(!nan_a.value_eq(f32_nan_a.as_ref()));
    }
}
