pub use fauxgen::Generator;
pub use fauxgen::GeneratorState;
pub use fauxgen::GeneratorToken;
pub use fauxgen::__private as fauxgen_private;

use crate::{Effect, Error, Resume};

pub type Co<Ctx> = GeneratorToken<Effect<Ctx>, Resume<Ctx>>;

pub type CoState<Ctx> = GeneratorState<Effect<Ctx>, Result<(), Error<Ctx>>>;
