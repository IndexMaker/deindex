use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Expr, Token,
};

// Define a struct to hold the parsed input: a list of expressions separated by commas.
struct LabelList {
    items: Punctuated<Expr, Token![,]>,
}

impl Parse for LabelList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let items = Punctuated::parse_terminated(input)?;
        Ok(LabelList { items })
    }
}

#[proc_macro]
pub fn label_vec(input: TokenStream) -> TokenStream {
    // 1. Parse the comma-separated list
    let input_list = parse_macro_input!(input as LabelList);

    // 2. Generate a call to the `label!` macro for each item.
    let label_tokens: Vec<TokenStream2> = input_list
        .items
        .into_iter()
        .map(|expr| {
            // Generate the token stream: label!(expr)
            quote! {
                deli::label!(#expr)
            }
        })
        .collect();

    // 3. Generate the final output code wrapped in the Labels and vec! structure.
    let output = quote! {
        Labels {
            data: vec![
                #(#label_tokens),*
            ],
        }
    };

    output.into()
}
