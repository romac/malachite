/// Process an [`Input`][input] and handle the emitted [`Effects`][effect].
///
/// [input]: crate::handle::Input
/// [effect]: crate::handle::Effect
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
        $crate::coroutine::process!(
            co: $crate::co::Co<_>,
            handle: $crate::handle::handle,
            state: $state,
            metrics: $metrics,
            input: $input,
            initial_resume: $crate::Resume::default(),
            default_resume: $crate::Resume::default(),
            with: $effect => $handle,
            complete: result => result.map_err(Into::into),
        )
    }};
}

/// Yield an effect, expecting a specific type of resume value.
///
/// Effects yielded by this macro must resume with a value that matches the provided pattern.
/// If not pattern is give, then the yielded effect must resume with [`Resume::Continue`][continue].
///
/// # Errors
/// This macro will abort the current function with a [`Error::UnexpectedResume`][error] error
/// if the effect does not resume with a value that matches the provided pattern.
///
/// # Example
/// ```rust,ignore
/// // If we do not need to extract the resume value
/// let () = perform!(co, effect, Resume::Continue => ());
///
/// // Or just
/// let () = perform!(co, effect, Resume::Continue);
///
/// /// If we need to extract the resume value
/// let value: Ctx::Value = perform!(co, effect, Resume::SentValueRequest(request_id) => request_id);
/// ```
///
/// [continue]: crate::handle::Resume::Continue
/// [error]: crate::handle::Error::UnexpectedResume
#[macro_export]
macro_rules! perform {
    ($co:expr, $effect:expr) => {
        perform!($co, $effect, $crate::Resume::Continue(_))
    };

    ($co:expr, $effect:expr, $pat:pat) => {
        perform!($co, $effect, $pat => ())
    };

    // TODO: Add support for multiple patterns + if guards
    ($co:expr, $effect:expr, $pat:pat => $expr:expr $(,)?) => {
        $crate::coroutine::perform!($co, $effect, $crate::Error::UnexpectedResume, $pat => $expr)
    };
}
