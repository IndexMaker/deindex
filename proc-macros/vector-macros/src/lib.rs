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
struct AmountList {
    items: Punctuated<Expr, Token![,]>,
}

impl Parse for AmountList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse a comma-separated list of expressions.
        let items = Punctuated::parse_terminated(input)?;
        Ok(AmountList { items })
    }
}

#[proc_macro]
pub fn amount_vec(input: TokenStream) -> TokenStream {
    // 1. Parse the input list
    let input_list = parse_macro_input!(input as AmountList);

    // 2. Generate a call to the procedural `amount!` macro for each item.
    let amount_tokens: Vec<TokenStream2> = input_list
        .items
        .into_iter()
        .map(|expr| {
            // Here, we generate the token stream: amount!(expr)
            // The procedural `amount!` macro will then run on this expression.
            quote! {
                amount_macros::amount!(#expr)
            }
        })
        .collect();

    // 3. Generate the final output code wrapped in the Vector and vec! structure.
    let output = quote! {
        Vector {
            data: vec![
                #(#amount_tokens),*
            ],
        }
    };

    output.into()
}
