use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse::Parse, parse2, parse_quote, spanned::Spanned, Attribute, Block, Error, FnArg, Generics,
    ImplItemFn, ItemImpl, LitStr, MacroDelimiter, Meta, MetaList, Pat, PatIdent, PatType, Path,
    Signature, Token, Type, TypePath,
};

use crate::mapping::{OCMInfo, RenameIdent, SelfConnectorMapper};

const ATTRIB_NAME: &str = "litesim_model";

pub fn except_self_attrib(attributes: impl AsRef<[Attribute]>) -> Vec<Attribute> {
    let attr = attributes.as_ref();
    let mut result = Vec::with_capacity(attr.len());

    for curr in attr {
        match &curr.meta {
            Meta::Path(path) => {
                if path.segments.len() == 1
                    && path.segments.first().unwrap().ident.to_string().as_str() == ATTRIB_NAME
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

pub fn ident_to_pat(ident: Ident) -> Pat {
    Pat::Ident(PatIdent {
        attrs: vec![],
        by_ref: None,
        mutability: None,
        ident,
        subpat: None,
    })
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

pub struct InputConnector {
    pub attributes: Vec<Attribute>,
    pub name: String,
    pub event_name: Box<Pat>,
    pub event_ty: Box<Type>,
    pub ctx_name: Box<Pat>,
    pub handler: TokenStream,
}

impl TryFrom<ItemConnector> for InputConnector {
    type Error = Error;

    fn try_from(value: ItemConnector) -> Result<Self, Self::Error> {
        let sig = value.item.signature();
        let inputs = &sig.inputs;

        let event_name: Box<Pat>;
        let event_ty: Box<Type>;
        match value.kind {
            Some(ConnectorKind::Input) => {
                if value.attrib_args.signal {
                    if inputs.len() != 2 {
                        return Err(Error::new(
                            sig.span(),
                            "input signal handler should take in exactly 2 arguments: (&mut self, ctx: ModelCtx<'s>)",
                        ));
                    }
                    event_name = parse_quote!(_);
                    event_ty = parse_quote!(());
                } else {
                    if inputs.len() != 3 {
                        return Err(Error::new(
                            sig.span(),
                            "input handler should take in exactly 3 arguments: (&mut self, event: _, ctx: ModelCtx<'s>)",
                        ));
                    }
                    match &inputs[1] {
                        syn::FnArg::Typed(arg) => {
                            event_name = arg.pat.clone();
                            event_ty = arg.ty.clone();
                        }
                        _ => {
                            return Err(Error::new(
                                sig.span(),
                                "invalid 2nd argument; expected Event",
                            ))
                        }
                    }
                }
            }
            _ => {
                return Err(Error::new(sig.span(), "invalid connector type"));
            }
        };

        let ctx_name: Box<Pat> = match inputs.last().unwrap() {
            syn::FnArg::Typed(arg) => arg.pat.clone(),
            syn::FnArg::Receiver(_) => {
                return Err(Error::new(
                    sig.span(),
                    "invalid last argument; expected ModelCtx",
                ))
            }
        };

        let in_block = value.item.into_block().expect("missing function body");
        let mut handler = TokenStream::new();

        let stmts = RenameIdent::default().process_stmts(in_block.stmts);

        handler.extend(quote! {{
            #(#stmts)
            *
        }});

        let name = value
            .attrib_args
            .rename
            .unwrap_or_else(|| sig.ident.to_string());

        Ok(InputConnector {
            attributes: value.attributes,
            name,
            event_name,
            event_ty,
            ctx_name,
            handler,
        })
    }
}

pub struct OutputConnector {
    pub attributes: Vec<Attribute>,
    pub name: String,
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
            name,
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
    pub other: Vec<TokenStream>,
}

static GEN_BY_MACRO: &[&str] = &[
    "input_connectors",
    "output_connectors",
    "get_input_handler",
    "type_id",
];

impl Parse for ModelTraitImpl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let implementation = input.parse::<ItemImpl>()?;

        let (neg_impl, trait_path, for_token) = match implementation.trait_ {
            Some(it) => it,
            None => {
                return Err(Error::new(
                    implementation.impl_token.span(),
                    format!("{} should be applied to Model implementation", ATTRIB_NAME),
                ));
            }
        };

        if neg_impl.is_some() {
            return Err(Error::new(
                implementation.impl_token.span(),
                format!(
                    "{} doesn't work on negative Model implementation",
                    ATTRIB_NAME
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
        let mut other = Vec::with_capacity(details.len());

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
                        other.push(forked);
                    }
                }
                it => {
                    other.push(it.to_token_stream());
                }
            }
        }

        let mut connector_mapper = SelfConnectorMapper {
            receiver: Ident::new("self", Span::call_site()),
            methods: Vec::with_capacity(details.len()),
        };

        for out_fns in &details {
            if out_fns.kind.is_none() {
                continue;
            }
            let sig = out_fns.item.signature();
            let signal = out_fns.attrib_args.signal;
            let in_name = sig.ident.to_string();
            let out_name = out_fns
                .attrib_args
                .rename
                .clone()
                .unwrap_or_else(|| sig.ident.to_string());
            connector_mapper.methods.push(OCMInfo {
                in_name,
                out_name,
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
                                    connector_mapper.process_block(&item_fn.block, ctx_name);
                            } else {
                                let wild_ident = Ident::new("model_context_", Span::call_site());
                                item_fn.block =
                                    connector_mapper.process_block(&item_fn.block, &wild_ident);
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
                    if GEN_BY_MACRO.contains(&name.as_str()) {
                        return Err(Error::new(
                            item.sig.span(),
                            format!("{} is already implemented by {} macro", name, ATTRIB_NAME),
                        ));
                    }

                    if let Some(ctx_arg) = find_ctx_arg_mut(&mut item.sig) {
                        match &mut *ctx_arg.pat {
                            Pat::Ident(PatIdent { ident, .. }) => {
                                item.block = connector_mapper.process_block(&item.block, ident);
                            }
                            Pat::Wild(_) => {
                                let wild_ident = Ident::new("model_context_", Span::call_site());
                                item.block =
                                    connector_mapper.process_block(&item.block, &wild_ident);
                                ctx_arg.pat = Box::new(ident_to_pat(wild_ident))
                            }
                            _ => unreachable!(),
                        }
                    }

                    other.push(item.to_token_stream())
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
            other,
        })
    }
}

impl ModelTraitImpl {
    pub fn gen_input_connectors(&self) -> TokenStream {
        let inputs: Vec<&str> = self.inputs.iter().map(|it| it.name.as_str()).collect();
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
                let name = &output.name;
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

    pub fn gen_input_conn_handler(&self) -> TokenStream {
        let mut handlers: Vec<TokenStream> = Vec::with_capacity(self.inputs.len());

        let self_ty = &self.self_ty;
        for input in &self.inputs {
            let input_ty = &input.event_ty;
            let passed_attrib = &input.attributes;
            let block = &input.handler;

            let event_name = &input.event_name;
            let ctx_name = &input.ctx_name;
            handlers.push(quote! {
                0 => {
                    let handler: Box<
                        &dyn Fn(
                            &mut #self_ty,
                            ::litesim::event::Event<#input_ty>,
                            ::litesim::simulation::ModelCtx<'s>,
                        ) -> Result<(), ::litesim::error::SimulationError>,
                    > = Box::new(
                        #(#passed_attrib)
                        *
                        &|self_: &mut #self_ty, event_: ::litesim::event::Event<#input_ty>, #ctx_name: ::litesim::simulation::ModelCtx<'s>| {
                            let #event_name = event_.into_inner();
                            #block
                        },
                    );
                    return Some(handler);
                }
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

        let input_connectors: TokenStream = self.gen_input_connectors().to_token_stream();
        let output_connectors: TokenStream = self.gen_output_connectors().to_token_stream();
        let input_connector_handler: TokenStream = self.gen_input_conn_handler().to_token_stream();

        let other_fns = &self.other;

        tokens.extend(quote!({
            #input_connectors
            #output_connectors
            #input_connector_handler

            #(#other_fns)*

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
    Signature(Signature),
}

impl DetailContents {
    pub fn signature(&self) -> Signature {
        match self {
            DetailContents::ItemFn(item_fn) => item_fn.sig.clone(),
            DetailContents::Signature(sig) => sig.clone(),
        }
    }

    pub fn into_block(self) -> Option<Block> {
        match self {
            DetailContents::ItemFn(item_fn) => Some(item_fn.block),
            DetailContents::Signature(_) => None,
        }
    }
}

pub struct ItemConnector {
    pub kind: Option<ConnectorKind>,
    pub attributes: Vec<Attribute>,
    pub attrib_args: ConnectorArguments,
    pub item: DetailContents,
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

        Ok(ItemConnector {
            kind: connector_kind,
            attributes: passed,
            attrib_args: attrib_args.unwrap_or_default(),
            item: DetailContents::ItemFn(item),
        })
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
                Ok(kind) => match kind {
                    ConnectorKind::Input => {
                        return Err(Error::new(
                            item.signature.span(),
                            "stub methods must be output connectors (allow only #[output] attribute)",
                        ));
                    }
                    ConnectorKind::Output => connector_kind = Some(ConnectorKind::Output),
                },
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

        Ok(ItemConnector {
            kind: connector_kind,
            attributes: passed,
            attrib_args: attrib_args.unwrap_or_default(),
            item: DetailContents::Signature(item.signature),
        })
    }
}

pub struct ItemFnStub {
    pub attrs: Vec<Attribute>,
    pub signature: Signature,
    pub terminator: Token![;],
}

impl Parse for ItemFnStub {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(ItemFnStub {
            attrs: Attribute::parse_outer(input)?,
            signature: input.parse()?,
            terminator: input.parse()?,
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
