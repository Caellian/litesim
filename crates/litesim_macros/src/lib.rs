use model::ModelTraitImpl;
use quote::ToTokens;
use syn::parse_macro_input;

mod mapping;
mod model;

#[proc_macro_attribute]
pub fn litesim_model(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let model: ModelTraitImpl = parse_macro_input!(input as ModelTraitImpl);
    model.into_token_stream().into()
}
