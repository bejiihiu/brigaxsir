//! Argument types: parsing user input tokens into values. Mirrors Java's
//! `com.mojang.brigadier.arguments` package.

mod argument_type;
pub mod bool_argument_type;
pub mod double_argument_type;
pub mod float_argument_type;
pub mod integer_argument_type;
pub mod long_argument_type;
pub mod string_argument_type;

pub use argument_type::ArgumentType;
pub use bool_argument_type::{BoolArgumentType, bool, get_bool};
pub use double_argument_type::{DoubleArgumentType, double, double_min, double_range, get_double};
pub use float_argument_type::{FloatArgumentType, float, float_min, float_range, get_float};
pub use integer_argument_type::{
    IntegerArgumentType, get_integer, integer, integer_min, integer_range,
};
pub use long_argument_type::{LongArgumentType, get_long, long, long_min, long_range};
pub use string_argument_type::{
    StringArgumentType, StringType, escape_if_required, get_string, greedy_string, string, word,
};
