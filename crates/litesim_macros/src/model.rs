use std::collections::VecDeque;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{
    parse::Parse, parse2, spanned::Spanned, token::Semi, Attribute, Block, Error, FnArg, Generics,
    ImplItemFn, ItemImpl, LitStr, MacroDelimiter, Meta, MetaList, Pat, PatIdent, PatType, Path,
    Receiver, Signature, Token, Type, TypePath, parse_quote,
};

use crate::{
    handler::InputHandler,
    mapping::{OCMInfo, RenameIdent, SelfConnectorMapper},
    util::*,
};

const MACRO_NAME: &str = "litesim_model";

pub fn except_self_attrib(attributes: impl AsRef<[Attribute]>) -> Vec<Attribute> {
    let attr = attributes.as_ref();
    let mut result = Vec::with_capacity(attr.len());

    for curr in attr {
        match &curr.meta {
            Meta::Path(path) => {
                if path.segments.len() == 1
                    && path.segments.first().unwrap().ident.to_string().as_str() == MACRO_NAME
                {
                    continue;
                }
            }
            _ => {}
        }
        result.push(curr.clone());
    }

    result
}

pub fn pat_to_string(pattern: &Pat) -> Option<String> {
    let result = match pattern {
        Pat::Ident(PatIdent { ident, .. }) => ident.to_string(),
        Pat::Wild(_) => "_".to_string(),
        _ => return None,
    };

    Some(result)
}

pub fn signal_ty() -> Type {
    parse_quote!(())
}

pub fn find_ctx_arg_mut(sig: &mut Signature) -> Option<&mut PatType> {
    let mut found_ctx = None;

    for input in &mut sig.inputs {
        match input {
            syn::FnArg::Typed(pat_t) => {
                let ident_name = pat_to_string(&pat_t.pat)
                    .expect("pat_to_string must support all find_ctx_name cases");

                match &*pat_t.ty {
                    Type::Path(TypePath { path, .. }) => {
                        if path.segments.last().map(|seg| seg.ident == "ModelCtx") == Some(true) {
                            return Some(pat_t);
                        }
                    }
                    _ => {}
                }

                match ident_name.as_str() {
                    "model_context" | "model_ctx" => found_ctx = Some(pat_t),
                    _ => {}
                }
            }
            syn::FnArg::Receiver(_) => {}
        }
    }

    found_ctx
}

#[derive(Clone)]
pub struct InputConnector {
    pub attributes: Vec<Attribute>,
    pub name: Ident,
    pub event_name: Box<Pat>,
    pub event_ty: Box<Type>,
    pub ctx_name: Box<Pat>,
    pub signal: bool,
    pub handler: Block,
}

impl TryFrom<ItemConnector> for InputConnector {
    type Error = Error;

    fn try_from(value: ItemConnector) -> Result<Self, Self::Error> {
        let sig = value.item.signature();
        let inputs = &sig.inputs;

        let event_name: Box<Pat>;
        let event_ty: Box<Type>;
        if value.attrib_args.signal {
            event_name = parse_quote!(_);
            event_ty = parse_quote!(());
        } else {
            if let syn::FnArg::Typed(arg) = &inputs[1] {
                event_name = arg.pat.clone();
                event_ty = arg.ty.clone();
            } else {
                unreachable!()
            }
        }

        let ctx_name = if let syn::FnArg::Typed(PatType { pat, .. }) = inputs.last().unwrap() {
            (*pat).clone()
        } else {
            unreachable!()
        };
        let in_block = value.item.block().expect("missing function body");
        let handler = RenameIdent::default().process_block(&in_block);

        let name = value
            .attrib_args
            .rename
            .unwrap_or_else(|| sig.ident.to_string());

        Ok(InputConnector {
            attributes: value.attributes,
            name: Ident::new(&name, sig.ident.span()),
            event_name,
            event_ty,
            ctx_name,
            signal: value.attrib_args.signal,
            handler,
        })
    }
}

pub struct OutputConnector {
    pub attributes: Vec<Attribute>,
    pub name: Ident,
    pub ty: Box<Type>,
}

impl TryFrom<ItemConnector> for OutputConnector {
    type Error = Error;

    fn try_from(value: ItemConnector) -> Result<Self, Self::Error> {
        let sig = value.item.signature();
        if value.kind != Some(ConnectorKind::Output) {
            return Err(Error::new(sig.span(), "invalid connector type"));
        }

        let ty = if value.attrib_args.signal {
            if sig.inputs.len() != 1 {
                return Err(Error::new(
                    sig.span(),
                    "output signal stub should only have a &self argument",
                ));
            }
            parse_quote!(())
        } else {
            if sig.inputs.len() != 2 {
                return Err(Error::new(
                    sig.span(),
                    "output stub should receive exactly 2 arguments: (&self, output_type: _)",
                ));
            }
            let first = sig.inputs.iter().nth(1).unwrap();

            match first {
                syn::FnArg::Typed(arg) => arg.ty.clone(),
                _ => {
                    return Err(Error::new(
                        sig.span(),
                        "output handler 2nd argument can't be a self reference",
                    ))
                }
            }
        };

        let name = value
            .attrib_args
            .rename
            .unwrap_or_else(|| sig.ident.to_string());

        Ok(OutputConnector {
            attributes: value.attributes,
            name: Ident::new(name.as_str(), sig.ident.span()),
            ty,
        })
    }
}

pub struct ModelTraitImpl {
    pub attrs: Vec<Attribute>,
    pub defaultness: Option<Token![default]>,
    pub impl_token: Token![impl],
    pub generics: Generics,
    pub trait_path: Path,
    pub for_token: Token![for],
    pub self_ty: Box<Type>,
    pub inputs: Vec<InputConnector>,
    pub outputs: Vec<OutputConnector>,
    pub other_impls: Vec<ImplItemFn>,
    pub unhandled: Vec<TokenStream>,
}

static AVOID_MANUAL_IMPL: &[&str] = &["type_id"];

impl Parse for ModelTraitImpl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let implementation = input.parse::<ItemImpl>()?;

        let (neg_impl, trait_path, for_token) = match implementation.trait_ {
            Some(it) => it,
            None => {
                return Err(Error::new(
                    implementation.impl_token.span(),
                    format!("{} should be applied to Model implementation", MACRO_NAME),
                ));
            }
        };

        if neg_impl.is_some() {
            return Err(Error::new(
                implementation.impl_token.span(),
                format!(
                    "{} doesn't work on negative Model implementation",
                    MACRO_NAME
                ),
            ));
        }

        if implementation.generics.params.is_empty() {
            return Err(Error::new(
                implementation.impl_token.span(),
                "a Model trait must have at least a generic Model lifetime",
            ));
        };

        let mut details: Vec<ItemConnector> = Vec::with_capacity(implementation.items.len());

        let mut inputs = Vec::with_capacity(details.len());
        let mut outputs = Vec::with_capacity(details.len());
        let mut other_impls = Vec::with_capacity(details.len());
        let mut unhandled = Vec::with_capacity(details.len());

        for item in implementation.items {
            match item {
                syn::ImplItem::Fn(item_fn) => {
                    let detail = ItemConnector::try_from(item_fn)?;
                    details.push(detail);
                }
                syn::ImplItem::Verbatim(verb) => {
                    let forked = verb.clone();
                    if let Ok(stub) = parse2::<ItemFnStub>(verb) {
                        details.push(ItemConnector::try_from(stub)?);
                    } else {
                        unhandled.push(forked);
                    }
                }
                it => {
                    unhandled.push(it.to_token_stream());
                }
            }
        }

        let mut connector_mapper = SelfConnectorMapper {
            receiver: Ident::new("self", Span::call_site()),
            methods: Vec::with_capacity(details.len()),
        };

        for out_fns in &details {
            let kind = match out_fns.kind {
                Some(kind) => kind,
                None => continue,
            };
            let sig = out_fns.item.signature();
            let signal = out_fns.attrib_args.signal;
            let ty = out_fns.event_ty().unwrap();
            let in_name = sig.ident.to_string();
            let out_name = out_fns
                .attrib_args
                .rename
                .clone()
                .unwrap_or_else(|| sig.ident.to_string());
            connector_mapper.methods.push(OCMInfo {
                kind,
                in_name,
                out_name,
                ty,
                signal,
            });
        }

        for mut detail in details {
            match detail.kind {
                Some(ConnectorKind::Input) => {
                    match &mut detail.item {
                        DetailContents::ItemFn(item_fn) => {
                            let item_span = item_fn.span();
                            let last_arg = item_fn.sig.inputs.last_mut().ok_or_else(|| {
                                Error::new(item_span, "input connector missing arguments")
                            })?;

                            let ctx_name = match last_arg {
                                syn::FnArg::Typed(arg) => match &*arg.pat {
                                    Pat::Ident(it) => Some(&it.ident),
                                    Pat::Wild(_) => None,
                                    Pat::Struct(_) => {
                                        return Err(Error::new(
                                            arg.span(),
                                            "ModelCtx must not be destructured",
                                        ))
                                    }
                                    _ => {
                                        return Err(Error::new(
                                            arg.span(),
                                            "unhandled ModelCtx name pattern; use literal or _",
                                        ))
                                    }
                                },
                                _ => {
                                    return Err(Error::new(
                                        item_fn.span(),
                                        "invalid last argument; expected a ModelCtx<'s> instead got self reference",
                                    ))
                                }
                            };

                            if let Some(ctx_name) = ctx_name {
                                item_fn.block =
                                    connector_mapper.process_block(&item_fn.block, ctx_name)?;
                            } else {
                                let wild_ident = Ident::new("model_context_", Span::call_site());
                                item_fn.block =
                                    connector_mapper.process_block(&item_fn.block, &wild_ident)?;
                                match last_arg {
                                    FnArg::Typed(PatType { pat, .. }) => {
                                        *pat = Box::new(ident_to_pat(wild_ident))
                                    }
                                    FnArg::Receiver(_) => unreachable!(),
                                }
                            }
                        }
                        DetailContents::Signature(_) => unreachable!("missing function body"),
                    };

                    inputs.push(detail.try_into()?)
                }
                Some(ConnectorKind::Output) => {
                    outputs.push(detail.try_into()?);
                }
                None => {
                    let mut item = match detail.item {
                        DetailContents::ItemFn(it) => it,
                        DetailContents::Signature(sig) => {
                            return Err(Error::new(sig.span(), "missing function body"));
                        }
                    };
                    let name = item.sig.ident.to_string();

                    if AVOID_MANUAL_IMPL.contains(&name.as_str()) {
                        return Err(Error::new(
                            item.sig.span(),
                            format!("{} should be implemented by {} macro", name, MACRO_NAME),
                        ));
                    }

                    if let Some(ctx_arg) = find_ctx_arg_mut(&mut item.sig) {
                        match &mut *ctx_arg.pat {
                            Pat::Ident(PatIdent { ident, .. }) => {
                                item.block = connector_mapper.process_block(&item.block, ident)?;
                            }
                            Pat::Wild(_) => {
                                let wild_ident = Ident::new("model_context_", Span::call_site());
                                item.block =
                                    connector_mapper.process_block(&item.block, &wild_ident)?;
                                ctx_arg.pat = Box::new(ident_to_pat(wild_ident))
                            }
                            _ => unreachable!(),
                        }
                    }

                    other_impls.push(item)
                }
            }
        }

        Ok(ModelTraitImpl {
            attrs: implementation.attrs,
            impl_token: implementation.impl_token,
            defaultness: implementation.defaultness,
            generics: implementation.generics,
            trait_path,
            for_token,
            self_ty: implementation.self_ty,
            inputs,
            outputs,
            other_impls,
            unhandled,
        })
    }
}

impl ModelTraitImpl {
    pub fn gen_input_connectors(&self) -> TokenStream {
        let inputs: Vec<_> = self.inputs.iter().map(|it| it.name.to_string()).collect();
        quote! {
            fn input_connectors(&self) -> Vec<&'static str> {
                vec![#(#inputs),*]
            }
        }
    }

    pub fn gen_output_connectors(&self) -> TokenStream {
        let outputs: Vec<TokenStream> = self
            .outputs
            .iter()
            .map(|output| {
                let ty = &output.ty;
                let name = output.name.to_string();
                quote! {
                    ::litesim::routes::OutputConnectorInfo::new::<#ty>(#name)
                }
            })
            .collect();
        quote! {
            fn output_connectors(&self) -> Vec<OutputConnectorInfo> {
                vec![#(#outputs),*]
            }
        }
    }

    pub fn gen_input_handlers(&self) -> TokenStream {
        let mut handlers: Vec<TokenStream> = Vec::with_capacity(self.inputs.len());

        let model_type = &self.self_ty;
        for (i, input) in self.inputs.iter().enumerate() {
            let handler = InputHandler::new(model_type.clone(), input.clone());

            handlers.push(quote! {
                #i => #handler
            })
        }
        quote! {
            fn get_input_handler<'h>(&self, index_: usize) -> Option<Box<dyn ErasedInputHandler<'h, 's>>>
            where
                's: 'h,
            {
                match index_ {
                    #(#handlers),
                    *
                    _ => return None,
                }
            }
        }
    }
}

impl ToTokens for ModelTraitImpl {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        except_self_attrib(&self.attrs)
            .iter()
            .for_each(|attr| attr.to_tokens(tokens));
        if self.defaultness.is_some() {
            tokens.extend(quote!(default));
        }
        tokens.extend(quote!(impl));
        self.generics.to_tokens(tokens);
        self.trait_path.to_tokens(tokens);
        tokens.extend(quote!(for));
        self.self_ty.to_tokens(tokens);

        let other_fns = &self.other_impls;

        let manual_outputs_impl = other_fns
            .iter()
            .any(|it| it.sig.ident.to_string() == "output_connectors");

        let output_connectors: TokenStream =
            if !manual_outputs_impl {
                self.gen_output_connectors().to_token_stream()
            } else {
                if !self.outputs.is_empty() {
                    let mut errors = Error::new(
                        self.outputs.first().unwrap().name.span(),
                        "can't combine with output_connectors",
                    );
                    errors.extend(self.outputs.iter().skip(1).map(|it| {
                        Error::new(it.name.span(), "can't combine with output_connectors")
                    }));
                    errors.to_compile_error()
                } else {
                    TokenStream::new()
                }
            };

        let manual_inputs_impl = other_fns
            .iter()
            .map(|it| it.sig.ident.to_string())
            .any(|it| it == "input_connectors" || it == "get_input_handler");

        let input_connectors: TokenStream =
            if !manual_inputs_impl {
                let mut result = self.gen_input_connectors().to_token_stream();
                result.extend(self.gen_input_handlers().to_token_stream());
                result
            } else {
                if !self.inputs.is_empty() {
                    let mut errors = Error::new(
                        self.inputs.first().unwrap().name.span(),
                        "can't combine with output_connectors",
                    );
                    errors.extend(self.inputs.iter().skip(1).map(|it| {
                        Error::new(it.name.span(), "can't combine with output_connectors")
                    }));
                    errors.to_compile_error()
                } else {
                    TokenStream::new()
                }
            };

        let unhandled = &self.unhandled;

        tokens.extend(quote!({
            #input_connectors
            #output_connectors

            #(#other_fns)*
            #(#unhandled)*

            fn type_id(&self) -> std::any::TypeId {
                ::litesim::prelude::const_type_id::<Self>()
            }
        }));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    Input,
    Output,
}

impl TryFrom<&Attribute> for ConnectorKind {
    type Error = ();

    fn try_from(value: &Attribute) -> Result<Self, Self::Error> {
        let attribute = match &value.meta {
            Meta::Path(path) => path,
            Meta::List(MetaList {
                path,
                delimiter: MacroDelimiter::Paren(_),
                ..
            }) => path,
            _ => return Err(()),
        };

        if attribute.segments.len() != 1 {
            return Err(());
        }

        let segment = attribute.segments.first().unwrap();

        let kind = match segment.ident.to_string().as_ref() {
            "input" => ConnectorKind::Input,
            "output" => ConnectorKind::Output,
            _ => return Err(()),
        };

        Ok(kind)
    }
}

pub enum DetailContents {
    ItemFn(ImplItemFn),
    Signature(ItemFnStub),
}

impl DetailContents {
    pub fn signature(&self) -> &Signature {
        match self {
            DetailContents::ItemFn(item_fn) => &item_fn.sig,
            DetailContents::Signature(stub) => &stub.signature,
        }
    }

    pub fn block(&self) -> Option<&Block> {
        match self {
            DetailContents::ItemFn(item_fn) => Some(&item_fn.block),
            DetailContents::Signature(_) => None,
        }
    }

    pub fn stub_semi(&self) -> Option<&Semi> {
        match self {
            DetailContents::ItemFn(_) => None,
            DetailContents::Signature(stub) => Some(&stub.semi),
        }
    }
}

pub struct ItemConnector {
    pub kind: Option<ConnectorKind>,
    pub attributes: Vec<Attribute>,
    pub attrib_args: ConnectorArguments,
    pub item: DetailContents,
}

impl ItemConnector {
    pub fn is_signal(&self) -> bool {
        self.attrib_args.signal
    }

    pub fn validate(self) -> Result<Self, Error> {
        let kind = match self.kind {
            Some(it) => it,
            None => return Ok(self),
        };

        let signature = self.item.signature();
        let ident = &signature.ident;
        let inputs = &signature.inputs;

        let argument_errors = match kind {
            ConnectorKind::Input => {
                if self.is_signal() {
                    vec![
                        match inputs.get(0) {
                            Some(FnArg::Receiver(Receiver {
                                mutability: Some(_),
                                ..
                            })) => None,
                            Some(other) => Some(Error::new(
                                other.span(),
                                "first argument should be a mutable self reference: &mut self",
                            )),
                            None => Some(Error::new(
                                signature.span(),
                                "missing required mutable self reference first argument: &mut self",
                            )),
                        },
                        match inputs.get(1) {
                            Some(syn::FnArg::Typed(_)) => None,
                            Some(_) | None => Some(Error::new(
                                ident.span(),
                                "missing required ModelCtx<'s> second argument",
                            )),
                        },
                    ]
                } else {
                    vec![
                        match inputs.get(0) {
                            Some(FnArg::Receiver(Receiver {
                                mutability: Some(_),
                                ..
                            })) => None,
                            Some(other) => Some(Error::new(
                                other.span(),
                                "first argument should be a mutable self reference: &mut self",
                            )),
                            None => Some(Error::new(
                                signature.span(),
                                "missing required mutable self reference first argument: &mut self",
                            )),
                        },
                        match inputs.get(1) {
                            Some(syn::FnArg::Typed(_)) => None,
                            Some(_) | None => Some(Error::new(
                                inputs.span(),
                                "missing required event type second argument",
                            )),
                        },
                        match inputs.get(2) {
                            Some(syn::FnArg::Typed(_)) => None,
                            Some(_) | None => Some(Error::new(
                                inputs.span(),
                                "missing required ModelCtx<'s> third argument",
                            )),
                        },
                    ]
                }
            }
            ConnectorKind::Output => {
                if self.is_signal() {
                    vec![match inputs.get(0) {
                        Some(FnArg::Receiver(Receiver {
                            mutability: None, ..
                        })) => None,
                        Some(other) => Some(Error::new(
                            other.span(),
                            "first argument should be a self reference: &self",
                        )),
                        _ => Some(Error::new(
                            signature.span(),
                            "missing required self reference first argument: &self",
                        )),
                    }]
                } else {
                    vec![
                        match inputs.get(0) {
                            Some(FnArg::Receiver(Receiver {
                                mutability: None, ..
                            })) => None,
                            Some(other) => Some(Error::new(
                                other.span(),
                                "first argument should be a self reference: &self",
                            )),
                            _ => Some(Error::new(
                                signature.span(),
                                "missing required self reference first argument: &self",
                            )),
                        },
                        match inputs.get(1) {
                            Some(syn::FnArg::Typed(_)) => None,
                            Some(_) | None => Some(Error::new(
                                inputs.span(),
                                "missing required event type second argument",
                            )),
                        },
                    ]
                }
            }
        };

        let extra_args = if argument_errors.len() < inputs.len() {
            let mut span_tokens = TokenStream::new();
            span_tokens.extend(
                inputs
                    .iter()
                    .skip(argument_errors.len())
                    .map(|it| it.to_token_stream()),
            );

            Some(Error::new(
                span_tokens.span(),
                format!("connector only takes {} arguments", argument_errors.len()),
            ))
        } else {
            None
        };

        let mut signature_errors: VecDeque<_> =
            argument_errors.into_iter().filter_map(|it| it).collect();

        if let Some(extra) = extra_args {
            signature_errors.push_back(extra);
        }

        match kind {
            ConnectorKind::Input => {
                if let Some(semi) = self.item.stub_semi() {
                    signature_errors.push_back(Error::new(
                        semi.span(),
                        "only output connectors can be stub; inputs must have a body returning Result<(), SimulationError>",
                    ));
                }
            }
            ConnectorKind::Output => {
                if self.attributes.len() > 0 {
                    signature_errors.push_back(Error::new(
                        self.attributes.first().unwrap().span(),
                        "output connectors aren't real functions; attribute will be erased",
                    ));
                    for attr in self.attributes.iter().skip(1) {
                        signature_errors
                            .push_back(Error::new(attr.span(), "attribute will be erased"))
                    }
                }
            }
        }

        const BAD_TYPE_MSG: &str =
            "connector return type is Result<(), SimulationError> (or wildcart)";
        match &signature.output {
            syn::ReturnType::Default => {
                signature_errors.push_back(Error::new(signature.span(), BAD_TYPE_MSG))
            }
            syn::ReturnType::Type(_, found_ty) => match &**found_ty {
                Type::Group(_) | Type::Paren(_) | Type::Path(_) => {
                    // can't enforce typing due to type aliasing
                }
                Type::Infer(_) => {
                    // allow inferring, we don't need the type anyway
                }
                other => signature_errors.push_back(Error::new(other.span(), BAD_TYPE_MSG)),
            },
        }

        if signature_errors.len() > 0 {
            let mut errors = signature_errors.pop_front().unwrap();
            errors.extend(signature_errors.into_iter());
            return Err(errors);
        }

        Ok(self)
    }

    pub fn event_ty(&self) -> Option<Type> {
        if self.is_signal() {
            return Some(signal_ty());
        }
        if let Some(FnArg::Typed(PatType { ty, .. })) = &self.item.signature().inputs.iter().nth(1)
        {
            return Some((**ty).clone());
        } else {
            return None;
        }
    }
}

impl TryFrom<ImplItemFn> for ItemConnector {
    type Error = Error;

    fn try_from(item: ImplItemFn) -> Result<Self, Self::Error> {
        let mut connector_kind = None;
        let mut attrib_args = None;
        let mut passed = vec![];

        for a in &item.attrs {
            match ConnectorKind::try_from(a) {
                Ok(kind) => {
                    connector_kind = Some(kind);
                    if let Meta::List(MetaList { tokens, .. }) = &a.meta {
                        attrib_args = Some(parse2(tokens.clone())?);
                    }
                }
                _ => {
                    passed.push(a.clone());
                }
            }
        }

        ItemConnector {
            kind: connector_kind,
            attributes: passed,
            attrib_args: attrib_args.unwrap_or_default(),
            item: DetailContents::ItemFn(item),
        }
        .validate()
    }
}

impl TryFrom<ItemFnStub> for ItemConnector {
    type Error = Error;

    fn try_from(item: ItemFnStub) -> Result<Self, Self::Error> {
        let mut connector_kind = None;
        let mut attrib_args = None;
        let mut passed = vec![];

        for a in &item.attrs {
            match ConnectorKind::try_from(a) {
                Ok(kind) => {
                    connector_kind = Some(kind);
                }
                _ => {
                    passed.push(a.clone());
                }
            }

            if let Meta::List(MetaList { tokens, .. }) = &a.meta {
                attrib_args = Some(parse2(tokens.clone())?);
            }
        }

        if connector_kind.is_none() {
            return Err(Error::new(
                item.signature.span(),
                "missing #[output] attribute or function body",
            ));
        }

        ItemConnector {
            kind: connector_kind,
            attributes: passed,
            attrib_args: attrib_args.unwrap_or_default(),
            item: DetailContents::Signature(item),
        }
        .validate()
    }
}

pub struct ItemFnStub {
    pub attrs: Vec<Attribute>,
    pub signature: Signature,
    pub semi: Token![;],
}

impl Parse for ItemFnStub {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(ItemFnStub {
            attrs: Attribute::parse_outer(input)?,
            signature: input.parse()?,
            semi: input.parse()?,
        })
    }
}

#[derive(Default)]
pub struct ConnectorArguments {
    pub signal: bool,
    pub rename: Option<String>,
}

impl Parse for ConnectorArguments {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut result = ConnectorArguments::default();
        while !input.is_empty() {
            if input.peek(syn::Ident) && input.peek2(Token![=]) {
                let name = input.parse::<Ident>()?;
                input.parse::<Token![=]>()?;
                match name.to_string().as_str() {
                    "name" | "rename" => {
                        let renamed = input.parse::<LitStr>()?;
                        result.rename = Some(renamed.value());
                    }
                    _ => {
                        return Err(Error::new(name.span(), "unknown connector argument"));
                    }
                }
            } else {
                let flag = input.parse::<Ident>()?;
                match flag.to_string().as_str() {
                    "signal" => {
                        result.signal = true;
                    }
                    _ => {
                        return Err(Error::new(flag.span(), "unknown connector flag"));
                    }
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(result)
    }
}

impl ToTokens for ItemFnStub {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append_all(self.attrs.clone());
        tokens.extend(self.signature.clone().into_token_stream());
        tokens.extend(self.semi.into_token_stream());
    }
}
