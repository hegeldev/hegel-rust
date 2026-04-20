RELEASE_TYPE: patch

This patch loosens restrictions on using threads in hegel-rust. `TestCase` now implements `Send`
(and already implemented Clone) so data generation can now occur from multiple threads.

> [!IMPORTANT]
>
> This feature should be used only with extreme caution at present. Please consult the `TestCase`
> documentation for details, but it can only be correctly used with extreme care. We intend
> for threading support to get significantly better over time without the API changing,
> and this is an initial release of the intended API with the worst possible implementation
> of it. We are releasing it because even with these caveats it is useful in some cases,
> but it is highly likely not to be ready for you to use yet.
