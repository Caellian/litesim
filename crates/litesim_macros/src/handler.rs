use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse::Parse, parse_quote, punctuated::Punctuated, spanned::Spanned, token, Error, Expr,
    ExprBlock, ExprClosure, Ident, Pat, PatType, Type, TypeGroup, TypeParen, TypeReference,
};

use crate::{
    model::InputConnector,
    util::{ident_to_pat, PunctuatedGetExt},
};

pub struct InputHandler {
    pub is_return: Option<token::Return>,
    pub handler: ExprClosure,
    pub semi: Option<token::Semi>,
}

impl InputHandler {
    pub fn new(model_type: Box<Type>, connector: InputConnector) -> Self {
        let input_ty = connector.event_ty;
        let mut block = connector.handler;

        let event_name = connector.event_name;

        let cb_event_name = if connector.signal {
            Ident::new("_", Span::call_site())
        } else {
            let name = Ident::new("event_", Span::call_site());
            block.stmts.insert(
                0,
                parse_quote! {
                    let #event_name = #name.into_inner();
                },
            );
            name
        };

        let mut inputs = Punctuated::new();

        inputs.push(Pat::Type(PatType {
            attrs: vec![],
            pat: Box::new(ident_to_pat(Ident::new("self_", Span::call_site()))),
            colon_token: token::Colon {
                spans: [Span::call_site()],
            },
            ty: Box::new(Type::Reference(TypeReference {
                and_token: token::And {
                    spans: [Span::call_site()],
                },
                lifetime: None,
                mutability: Some(token::Mut {
                    span: Span::call_site(),
                }),
                elem: model_type,
            })),
        }));
        inputs.push(Pat::Type(PatType {
            attrs: vec![],
            pat: Box::new(ident_to_pat(cb_event_name)),
            colon_token: token::Colon {
                spans: [Span::call_site()],
            },
            ty: parse_quote! {::litesim::event::Event<#input_ty>},
        }));
        inputs.push(Pat::Type(PatType {
            attrs: vec![],
            pat: connector.ctx_name,
            colon_token: token::Colon {
                spans: [Span::call_site()],
            },
            ty: parse_quote! {::litesim::simulation::ModelCtx<'s>},
        }));

        let handler: ExprClosure = ExprClosure {
            attrs: connector.attributes,
            lifetimes: None,
            constness: None,
            movability: None,
            asyncness: None,
            capture: None,
            or1_token: token::Or {
                spans: [Span::call_site()],
            },
            inputs,
            or2_token: token::Or {
                spans: [Span::call_site()],
            },
            output: syn::ReturnType::Default,
            body: Box::new(Expr::Block(ExprBlock {
                attrs: vec![],
                label: None,
                block,
            })),
        };

        InputHandler {
            is_return: Some(token::Return {
                span: Span::call_site(),
            }),
            handler,
            semi: Some(token::Semi {
                spans: [Span::call_site()],
            }),
        }
    }

    pub fn model_type(&self) -> Result<&Type, Error> {
        match self.handler.inputs.get(0) {
            Some(it) => match it {
                Pat::Type(PatType { ty, .. }) => Ok(&**ty),
                other => Err(Error::new(other.span(), "expected typed model argument")),
            },
            None => Err(Error::new(
                self.handler.span(),
                "handler missing model argument",
            )),
        }
    }

    pub fn event_type(&self) -> Result<&Type, Error> {
        fn handle_nested_ty(nested: &Type) -> Result<&Type, Error> {
            match nested {
                Type::Group(TypeGroup { elem, .. }) | Type::Paren(TypeParen { elem, .. }) => {
                    handle_nested_ty(&**elem)
                }
                Type::Path(_) => Ok(nested),
                other => Err(Error::new(other.span(), "unexpected event type")),
            }
        }
        match self.handler.inputs.get(1) {
            Some(it) => match it {
                Pat::Type(PatType { ty, .. }) => handle_nested_ty(&**ty),
                other => Err(Error::new(other.span(), "expected typed event argument")),
            },
            None => Err(Error::new(
                self.handler.span(),
                "handler missing event argument",
            )),
        }
    }

    pub fn validate(self) -> Result<Self, Error> {
        self.model_type()?;
        self.event_type()?;

        match self.handler.inputs.get(2).cloned() {
            Some(Pat::Type(_)) => {}
            Some(other) => return Err(Error::new(other.span(), "expected ModelCtx<'s> argument")),
            None => {
                return Err(Error::new(
                    self.handler.inputs.span(),
                    "missing ModelCtx<'s> argument",
                ))
            }
        }

        if self.handler.inputs.len() > 3 {
            let mut extra_args = TokenStream::new();
            extra_args.extend(
                self.handler
                    .inputs
                    .iter()
                    .skip(3)
                    .map(|it| it.to_token_stream()),
            );
            return Err(Error::new(
                extra_args.span(),
                "provided too many arguments; expected 3",
            ));
        }

        Ok(self)
    }
}

impl Parse for InputHandler {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        InputHandler {
            is_return: input.parse()?,
            handler: input.parse()?,
            semi: input.parse()?,
        }
        .validate()
    }
}

impl ToTokens for InputHandler {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let model_type = match self.model_type() {
            Ok(it) => it,
            Err(err) => return tokens.extend([err.to_compile_error()].into_iter()),
        };

        let event_type = match self.event_type() {
            Ok(it) => it,
            Err(err) => return tokens.extend([err.to_compile_error()].into_iter()),
        };

        let is_return = &self.is_return;
        let handler = &self.handler;
        let semi = &self.semi;

        tokens.extend(
            [quote! {{
                let handler: Box<
                    &dyn Fn(
                        #model_type,
                        #event_type,
                        ::litesim::simulation::ModelCtx<'s>,
                    ) -> Result<(), ::litesim::error::SimulationError>,
                > = Box::new(&
                    #handler
                );
                #is_return Some(handler)#semi
            }}]
            .into_iter(),
        )
    }
}
