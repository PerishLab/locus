use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Token, parse_macro_input};

struct Record {
    engine: Expr,
    context: Expr,
    candidate: Expr,
}

impl Parse for Record {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let engine = input.parse()?;
        input.parse::<Token![,]>()?;
        let context = input.parse()?;
        input.parse::<Token![,]>()?;
        let candidate = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("record! accepts engine, context, candidate"));
        }
        Ok(Self {
            engine,
            context,
            candidate,
        })
    }
}

#[proc_macro]
pub fn record(input: TokenStream) -> TokenStream {
    let Record {
        engine,
        context,
        candidate,
    } = parse_macro_input!(input as Record);
    quote! {{
        let candidate = (#candidate).source(::locus::Source::code(
            file!(),
            line!(),
            column!(),
            module_path!(),
        ));
        (#engine).append(#context, candidate)
    }}
    .into()
}
