pub use fauxgen::Generator;
pub use fauxgen::GeneratorState;
pub use fauxgen::GeneratorToken;
pub use fauxgen::__private as fauxgen_private;

use crate::effect::{Effect, Resume};
use crate::error::Error;

pub type Co<Ctx> = GeneratorToken<Effect<Ctx>, Resume<Ctx>>;

pub type CoResult<Ctx> = GeneratorState<Effect<Ctx>, Result<(), Error<Ctx>>>;
