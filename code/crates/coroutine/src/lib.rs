pub use fauxgen::Generator;
pub use fauxgen::GeneratorState;
pub use fauxgen::GeneratorToken;
pub use fauxgen::__private as fauxgen_private;

// Re-export tracing for use in macros
#[doc(hidden)]
pub use tracing;

/// Process an input and handle the emitted effects using a coroutine.
///
/// This macro creates a generator coroutine, resumes it with the given input,
/// and dispatches yielded effects to a handler. The caller provides all
/// crate-specific types and functions as parameters.
#[macro_export]
macro_rules! process {
    (
        co: $co_ty:ty,
        handle: $handle_fn:expr,
        state: $state:expr,
        metrics: $metrics:expr,
        input: $input:expr,
        initial_resume: $initial_resume:expr,
        default_resume: $default_resume:expr,
        with: $effect:ident => $handle:expr,
        complete: $result:ident => $complete:expr $(,)?
    ) => {{
        let token = $crate::fauxgen_private::token();
        let gen = $crate::fauxgen_private::gen_sync(token.marker(), async {
            let co: $co_ty = $crate::fauxgen_private::register_owned(token).await;
            ($handle_fn)(co, $state, $metrics, $input).await
        });
        let mut gen = ::core::pin::pin!(gen);
        let mut co_result = $crate::Generator::resume(gen.as_mut(), $initial_resume);

        loop {
            match co_result {
                $crate::GeneratorState::Yielded($effect) => {
                    let resume = match $handle {
                        Ok(resume) => resume,
                        Err(error) => {
                            $crate::tracing::error!("Error when processing effect: {error:?}");
                            $default_resume
                        }
                    };
                    co_result = $crate::Generator::resume(gen.as_mut(), resume)
                }
                $crate::GeneratorState::Complete($result) => break $complete,
            }
        }
    }};
}

/// Yield an effect and match the resume value against a pattern.
///
/// This is the core form of the `perform!` macro. Each consumer crate wraps
/// this with convenience forms that fill in the crate-specific error constructor.
#[macro_export]
macro_rules! perform {
    ($co:expr, $effect:expr, $error:path, $pat:pat => $expr:expr $(,)?) => {
        match $co.yield_($effect).await {
            $pat => $expr,
            resume => return ::core::result::Result::Err($error(resume, stringify!($pat)).into()),
        }
    };
}
