use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Expr, LitInt};

fn process_literal_expr(expr: Expr) -> proc_macro2::TokenStream {
    let output = match expr {
        Expr::Lit(expr_lit) => match expr_lit.lit {
            syn::Lit::Float(lit_float) => {
                let value_str = lit_float.base10_digits();
                let parts: Vec<&str> = value_str.split('.').collect();

                // If there is no decimal point (e.g., input was 1.0), use scale 0.
                let (value_str_no_dot, scale) = if parts.len() == 2 {
                    let frac_part = parts[1];
                    let value_no_dot = format!("{}{}", parts[0], frac_part);
                    let scale_val = frac_part.len();
                    (value_no_dot, scale_val)
                } else if parts.len() == 1 {
                    (parts[0].to_string(), 0)
                } else {
                    panic!("Invalid float literal format.");
                };

                let raw_value: LitInt = syn::parse_str(&value_str_no_dot).unwrap();
                quote! {
                    Amount::from_u128_with_scale(#raw_value as u128, #scale as u8)
                }
            }
            syn::Lit::Int(lit_int) => {
                match lit_int.base10_parse::<u128>() {
                    Ok(0) => {
                        quote! { Amount::ZERO }
                    }
                    Ok(1) => {
                        quote! { Amount::ONE }
                    }
                    Ok(2) => {
                        quote! { Amount::TWO }
                    }
                    Ok(4) => {
                        quote! { Amount::FOUR }
                    }
                    Ok(_) => {
                        // Case 3: Other integers (e.g., 100)
                        quote! {
                            Amount::from_u128_with_scale(#lit_int as u128, 0)
                        }
                    }
                    Err(e) => panic!("Failed to parse integer literal: {}", e),
                }
            }
            e => panic!("amount! only accepts a floating-point literal. Instead got {:?}", e),
        },
        e => panic!("amount! only accepts a literal. Instead got: {:?}", e),
    };

    output.into()
}

#[proc_macro]
pub fn amount(input: TokenStream) -> TokenStream {
    let mut expr = parse_macro_input!(input as Expr);

    if let Expr::Group(group) = expr {
        expr = *group.expr; 
    }

    process_literal_expr(expr).into()
}