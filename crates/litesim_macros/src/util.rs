use syn::punctuated::Punctuated;

pub trait PunctuatedGetExt<I> {
    fn get(&self, index: usize) -> Option<&I>;
    fn get_mut(&mut self, index: usize) -> Option<&mut I>;
}

impl<I, P> PunctuatedGetExt<I> for Punctuated<I, P> {
    fn get(&self, index: usize) -> Option<&I> {
        if self.len() > index {
            Some(&self[index])
        } else {
            None
        }
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut I> {
        if self.len() > index {
            Some(&mut self[index])
        } else {
            None
        }
    }
}

pub fn ident_to_pat(ident: syn::Ident) -> syn::Pat {
    syn::Pat::Ident(syn::PatIdent {
        attrs: vec![],
        by_ref: None,
        mutability: None,
        ident,
        subpat: None,
    })
}

/*
Extract T from Event<T>
{
    let last_segment = path
        .segments
        .last()
        .ok_or_else(|| Error::new(path.span(), "handler missing model argument"))?;
    if let PathArguments::AngleBracketed(AngleBracketedGenericArguments {
        args,
        ..
    }) = last_segment.arguments
    {
        let t = args.first().ok_or_else(|| {
            Error::new(args.span(), "handler missing model argument")
        })?;
    } else {
        Error::new(last_segment.arguments.span(), "expected angled brackets")
    }
}
*/
