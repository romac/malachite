pub use malachitebft_coroutine::fauxgen_private;
pub use malachitebft_coroutine::Generator;
pub use malachitebft_coroutine::GeneratorState;
pub use malachitebft_coroutine::GeneratorToken;

use crate::effect::{Effect, Resume};
use crate::error::Error;

pub type Co<Ctx> = GeneratorToken<Effect<Ctx>, Resume<Ctx>>;

pub type CoResult<Ctx> = GeneratorState<Effect<Ctx>, Result<(), Error<Ctx>>>;
