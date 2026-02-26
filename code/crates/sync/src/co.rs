use genawaiter::GeneratorState;
use genawaiter::sync as r#gen;

use crate::{Effect, Error, Resume};

pub type Gen<Ctx, F> = r#gen::Gen<Effect<Ctx>, Resume<Ctx>, F>;
pub type Co<Ctx> = r#gen::Co<Effect<Ctx>, Resume<Ctx>>;
pub type CoState<Ctx> = GeneratorState<Effect<Ctx>, Result<(), Error<Ctx>>>;
