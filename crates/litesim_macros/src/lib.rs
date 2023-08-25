use handler::InputHandler;
use model::ModelTraitImpl;
use quote::ToTokens;
use syn::parse_macro_input;

mod handler;
mod mapping;
mod model;
mod util;

#[proc_macro_attribute]
pub fn litesim_model(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let model: ModelTraitImpl = parse_macro_input!(input as ModelTraitImpl);
    model.into_token_stream().into()
}

#[proc_macro_attribute]
pub fn input_handler(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let model: InputHandler = parse_macro_input!(input as InputHandler);
    model.into_token_stream().into()
}
