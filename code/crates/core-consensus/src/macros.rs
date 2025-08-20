/// Process an [`Input`][input] and handle the emitted [`Effects`][effect].
///
/// [input]: crate::input::Input
/// [effect]: crate::effect::Effect
///
/// # Example
///
/// ```rust,ignore
/// malachitebft_core_consensus::process!(
///     // Input to process
///     input: input,
///     // Consensus state
///     state: &mut state,
///     // Metrics
///     metrics: &metrics,
///    // Effect handler
///     on: effect => handle_effect(effect).await
/// )
/// ```
#[macro_export]
macro_rules! process {
    (input: $input:expr, state: $state:expr, metrics: $metrics:expr, with: $effect:ident => $handle:expr) => {{
        let mut gen = $crate::gen::Gen::new(|co| $crate::handle(co, $state, $metrics, $input));
        let mut co_result = gen.resume_with($crate::Resume::Start);

        'proc: loop {
            match co_result {
                $crate::gen::CoResult::Yielded($effect) => {
                    $crate::tracing::debug!("Processing effect: {effect:?}");
                    let resume = match $handle {
                        Ok(resume) => resume,
                        Err(error) => {
                            $crate::tracing::error!("Error when processing effect: {error:?}");
                            $crate::Resume::Continue
                        }
                    };
                    co_result = gen.resume_with(resume)
                }
                $crate::gen::CoResult::Complete(result) => break 'proc result.map_err(Into::into),
            }
        }
    }};
}

/// Yield an effect, expecting a specific type of resume value.
///
/// Effects yielded by this macro must resume with a value that matches the provided pattern.
/// If not pattern is give, then the yielded effect must resume with [`Resume::Continue`][continue].
///
/// [continue]: crate::effect::Resume::Continue
///
/// # Errors
/// This macro will abort the current function with a [`Error::UnexpectedResume`][error] error
/// if the effect does not resume with a value that matches the provided pattern.
///
/// # Example
/// ```rust,ignore
/// // If we do not need to extract the resume value
/// let () = perform!(co, effect, Resume::ProposeValue(_, _));
///
/// /// If we need to extract the resume value
/// let value: Ctx::Value = perform!(co, effect, Resume::ProposeValue(_, value) => value);
/// ```
///
/// [error]: crate::error::Error::UnexpectedResume
#[macro_export]
macro_rules! perform {
    ($co:expr, $effect:expr) => {
        perform!($co, $effect, $crate::effect::Resume::Continue)
    };

    ($co:expr, $effect:expr, $pat:pat) => {
        perform!($co, $effect, $pat => ())
    };

    // TODO: Add support for multiple patterns + if guards
    ($co:expr, $effect:expr, $pat:pat => $expr:expr $(,)?) => {
        match $co.yield_($effect).await {
            $pat => $expr,
            resume => {
                return ::core::result::Result::Err($crate::error::Error::UnexpectedResume(
                    resume,
                    stringify!($pat)
                )
                .into())
            }
        }
    };
}
