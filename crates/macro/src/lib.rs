use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, ItemFn, Path, Token, parse_macro_input};

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

struct Trace {
    observation: Expr,
    binding: Option<Ident>,
    sdk: Path,
}

impl Parse for Trace {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let field: Ident = input.parse()?;
        if field != "with" {
            return Err(syn::Error::new(field.span(), "trace expects `with`"));
        }
        input.parse::<Token![=]>()?;
        let observation = input.parse()?;
        let mut binding = None;
        let mut sdk = None;
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let field: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if field == "frame" && binding.is_none() {
                binding = Some(input.parse()?);
            } else if field == "sdk" && sdk.is_none() {
                sdk = Some(input.parse()?);
            } else {
                return Err(syn::Error::new(
                    field.span(),
                    "expected an unused `frame` or `sdk`",
                ));
            }
        }
        Ok(Self {
            observation,
            binding,
            sdk: sdk.unwrap_or_else(|| syn::parse_quote!(::locus)),
        })
    }
}

#[proc_macro_attribute]
pub fn trace(args: TokenStream, input: TokenStream) -> TokenStream {
    let Trace {
        observation,
        binding,
        sdk,
    } = parse_macro_input!(args as Trace);
    let mut function = parse_macro_input!(input as ItemFn);
    if let Some(token) = &function.sig.constness {
        return syn::Error::new_spanned(token, "trace does not support const functions")
            .into_compile_error()
            .into();
    }
    if let Some(token) = &function.sig.unsafety {
        return syn::Error::new_spanned(token, "trace does not support unsafe functions")
            .into_compile_error()
            .into();
    }
    if let Some(attribute) = function
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("track_caller"))
    {
        return syn::Error::new_spanned(
            attribute,
            "trace does not preserve track_caller semantics",
        )
        .into_compile_error()
        .into();
    }

    let name = function.sig.ident.to_string();
    let body = function.block;
    let expose = binding.map(|name| {
        quote! {
            let #name: ::std::option::Option<#sdk::Context> =
                frame.as_ref().map(|(_, context)| context.clone());
        }
    });
    let invoke = if function.sig.asyncness.is_some() {
        quote! { (async #body).await }
    } else {
        quote! { (|| #body)() }
    };
    function.block = syn::parse_quote!({
        let frame = (#observation).and_then(|(engine, context)| {
            let candidate = #sdk::Candidate::context()
                .ensure(#sdk::Role::trace())
                .ensure(#sdk::Role::span())
                .source(
                    #sdk::Source::code(file!(), line!(), column!(), module_path!())
                        .enter(#name),
                );
            engine
                .append(context, candidate)
                .ok()
                .map(|accepted| (engine, accepted.context()))
        });
        let result = {
            #expose
            #invoke
        };
        if let Some((engine, context)) = frame {
            let candidate = #sdk::Candidate::context()
                .ensure(#sdk::Role::trace())
                .ensure(#sdk::Role::span())
                .source(
                    #sdk::Source::code(file!(), line!(), column!(), module_path!())
                        .returned(#name),
                );
            let _ = engine.append(&context, candidate);
        }
        result
    });
    quote!(#function).into()
}
