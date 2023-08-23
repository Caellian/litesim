mod data;

#[proc_macro_attribute]
pub fn litesim_model(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = proc_macro2::token_stream::TokenStream::from(input);

    input.into()
}
