use malachitebft_core_types::Context;
use malachitebft_metrics::Metrics;

use crate::effect::{Effect, Resume};
use crate::error::Error;
use crate::handle;
use crate::input::Input;
use crate::r#gen::CoResult;
use crate::state::State;

pub async fn process<Ctx, F, E>(
    input: Input<Ctx>,
    state: &mut State<Ctx>,
    metrics: &Metrics,
    mut handler: F,
) -> Result<Result<(), Error<Ctx>>, E>
where
    Ctx: Context,
    F: AsyncFnMut(Effect<Ctx>) -> Result<Resume<Ctx>, E>,
    E: core::fmt::Debug,
{
    let mut gen = crate::gen::Gen::new(|co| handle(co, state, metrics, input));
    let mut co_result = gen.resume_with(Resume::Start);

    loop {
        match co_result {
            CoResult::Yielded(effect) => {
                let resume = handler(effect).await?;
                co_result = gen.resume_with(resume)
            }

            CoResult::Complete(result) => return Ok(result),
        }
    }
}
