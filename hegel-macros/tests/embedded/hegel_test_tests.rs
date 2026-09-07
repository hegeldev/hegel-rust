use super::*;
use quote::quote;

#[test]
fn test_expansion_passes_compile_time_database_root() {
    let out = expand_test(
        quote! {},
        quote! {
            fn prop(tc: TestCase) {
                let b: bool = tc.draw(gs::booleans());
                assert!(b);
            }
        },
    )
    .to_string();
    let expected = quote! { .__database_root(env!("CARGO_MANIFEST_DIR").to_string()) }.to_string();
    assert!(
        out.contains(&expected),
        "expected expansion to contain `{expected}`, got: {out}"
    );
}
