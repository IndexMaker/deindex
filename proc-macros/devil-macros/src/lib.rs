use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, Span, TokenTree};
use quote::{quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    Error, Expr, Ident, Lit, Token,
};
use std::collections::HashMap;

// --- 1. Argument Type Enum ---

#[derive(Debug, PartialEq, Eq, Hash)]
enum ArgType {
    RegisterId, // <reg>
    Amount,     // <immediate (scalar)> for IMMS/VPUSH
    StackPos,   // <pos>, <pos_A>, <pos_B>
    StorageId,  // <label_id>, <vector_id>, <scalar_id>, <prg_id>
    Label,      // <immediate (label)>
    Size,       // <count>, <N>, <M>, <R>
}

// --- 2. Static Argument Type Map (Grouped by vis.rs Layout) ---

lazy_static::lazy_static! {
    // Map: Mnemonic -> Expected Argument Types
    static ref ARG_TYPES: HashMap<&'static str, Vec<ArgType>> = {
        use ArgType::*;
        let mut m = HashMap::new();
        
        // 1. Data Loading & Stack Access (10-14)
        m.insert("LDL", vec![StorageId]);  
        m.insert("LDV", vec![StorageId]);  
        m.insert("LDS", vec![StorageId]);  
        m.insert("LDD", vec![StackPos]);   
        m.insert("LDR", vec![RegisterId]); 

        // 2. Data Storage & Register Access (20-23)
        m.insert("STL", vec![StorageId]);  
        m.insert("STV", vec![StorageId]);  
        m.insert("STS", vec![StorageId]);  
        m.insert("STR", vec![RegisterId]);

        // 3. Data Structure Manipulation (30-35)
        m.insert("PKV", vec![Size]);       // <count>
        m.insert("PKL", vec![Size]);       // <count>
        m.insert("UNPK", vec![]);
        m.insert("VPUSH", vec![Amount]);   // <immediate (scalar)>
        m.insert("VPOP", vec![]);
        m.insert("T", vec![Size]);         // <count>

        // 4. Labels Manipulation (40-46)
        m.insert("LUNION", vec![StackPos]);
        m.insert("LPUSH", vec![Label]);    // <immediate (label)>
        m.insert("LPOP", vec![]);
        m.insert("JADD", vec![StackPos, StackPos]); 
        m.insert("JSSB", vec![StackPos, StackPos]);
        m.insert("JXPND", vec![StackPos, StackPos]);
        m.insert("JFLTR", vec![StackPos, StackPos]);
        
        // 5. Arithmetic & Core Math (50-55)
        m.insert("ADD", vec![StackPos]);   
        m.insert("SUB", vec![StackPos]);
        m.insert("SSB", vec![StackPos]);
        m.insert("MUL", vec![StackPos]);   
        m.insert("DIV", vec![StackPos]);   
        m.insert("SQRT", vec![]); 

        // 6. Logic & Comparison (60-61)
        m.insert("MIN", vec![StackPos]);   
        m.insert("MAX", vec![StackPos]);

        // 7. Vector Aggregation (70-72)
        m.insert("VSUM", vec![]); 
        m.insert("VMIN", vec![]); 
        m.insert("VMAX", vec![]); 

        // 8. Immediate Values & Vector Creation (80-83)
        m.insert("IMMS", vec![Amount]);    // <immediate (scalar)>
        m.insert("IMML", vec![Label]);     // <immediate (label)>
        m.insert("ZEROS", vec![StackPos]); // <pos> (vector length)
        m.insert("ONES", vec![StackPos]);  // <pos> (vector length)

        // 9. Stack Control & Program Flow (90-94)
        m.insert("POPN", vec![Size]);      // <count>
        m.insert("SWAP", vec![StackPos]);  // <pos>
        m.insert("B", vec![StorageId, Size, Size, Size]); // <prg_id> <N> <M> <R>
        m.insert("FOLD", vec![StorageId, Size, Size, Size]); // <prg_id> <N> <M> <R>
        
        m
    };
}
// ------------------------------------

// --- Parsing Structures ---

/// Holds the arguments and is what the ArgType is mapped to.
enum InstructionArg {
    Literal(Expr),
    Register(String), // e.g., "_weights"
    Constant(Ident),  // e.g., "POS_OFFSET"
}

/// Holds the structure of a single assembly instruction.
struct Instruction {
    mnemonic: Ident,
    args: Vec<InstructionArg>,
}

/// Holds the entire list of instructions from the macro invocation.
struct InstructionList {
    instructions: Vec<Instruction>,
}

impl Parse for InstructionList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut instructions = Vec::new();
        
        while !input.is_empty() {
            // Consume comments
            if input.peek(Token![/]) && input.peek2(Token![/]) {
                while !input.is_empty() { let _: TokenTree = input.parse()?; }
                break;
            }

            let mnemonic: Ident = input.parse()?;
            let mnemonic_str = mnemonic.to_string().to_uppercase();

            // 1. Look up expected argument types
            let expected_types = ARG_TYPES.get(mnemonic_str.as_str())
                .ok_or_else(|| input.error(format!("Unknown VIL mnemonic: {}", mnemonic_str)))?;
            
            let mut args = Vec::new();

            // 2. Consume exactly the expected arguments with validation
            for (i, expected_type) in expected_types.iter().enumerate() {
                // Ignore commas
                while input.peek(Token![,]) { let _: Token![,] = input.parse()?; }
                
                if input.is_empty() {
                    return Err(input.error(format!(
                        "Missing argument {} of {} for instruction {}", 
                        i + 1, expected_types.len(), mnemonic_str
                    )));
                }

                let (arg, is_register) = if input.peek(Lit) {
                    let lit: Lit = input.parse()?;
                    (InstructionArg::Literal(Expr::Lit(syn::ExprLit { attrs: Vec::new(), lit })), false)

                } else if input.peek(Ident) {
                    let ident: Ident = input.parse()?;
                    let ident_str = ident.to_string();

                    if ident_str.starts_with('_') {
                        (InstructionArg::Register(ident_str), true)
                    } else {
                        (InstructionArg::Constant(ident), false)
                    }
                } else {
                    return Err(input.error(format!(
                        "Argument {} of {} for {} must be a literal or identifier, found unexpected token.", 
                        i + 1, expected_types.len(), mnemonic_str
                    )));
                };
                
                // 3. Type Validation Check
                match expected_type {
                    ArgType::RegisterId if !is_register => {
                        return Err(input.error(format!(
                            "Argument {} for {} must be a register (e.g., _name).", 
                            i + 1, mnemonic_str
                        )));
                    }
                    ArgType::RegisterId if is_register => {} // OK
                    _ if is_register => { // All other types (Amount, StackPos, StorageId, Label, Size) must NOT be a register
                        return Err(input.error(format!(
                            "Argument {} for {} cannot be a register (_name). Expected a literal or constant.", 
                            i + 1, mnemonic_str
                        )));
                    }
                    _ => {} // OK for non-register types receiving Literal/Constant
                }

                args.push(arg);
            }
            
            instructions.push(Instruction { mnemonic, args });
            
            // Consume remaining inline comments
            if input.peek(Token![/]) && input.peek2(Token![/]) {
                while !input.is_empty() { let _: TokenTree = input.parse()?; }
            }
        }
        
        Ok(InstructionList { instructions })
    }
}

#[proc_macro]
pub fn devil(input: TokenStream) -> TokenStream {
    let instruction_list = match syn::parse::<InstructionList>(input) {
        Ok(list) => list,
        Err(e) => return e.to_compile_error().into(),
    };

    let mut final_tokens = TokenStream2::new();
    let mut reg_map: HashMap<String, u128> = HashMap::new();
    let mut next_reg_index: u128 = 0;

    // --- Phase 2: Allocation and Generation ---
    for instruction in instruction_list.instructions {
        let op_code = format!("OP_{}", instruction.mnemonic.to_string().to_uppercase());
        let op_code_ident = Ident::new(&op_code, Span::call_site());
        final_tokens.extend(quote! { #op_code_ident, });

        for arg in instruction.args {
            let arg_tokens = match arg {
                InstructionArg::Register(reg_name) => {
                    // Dynamic Register Allocation
                    let reg_index = *reg_map.entry(reg_name).or_insert_with(|| {
                        let index = next_reg_index;
                        next_reg_index += 1;
                        index
                    });
                    quote! { #reg_index as u128, }
                }
                InstructionArg::Literal(expr) => {
                    let mnemonic_str = instruction.mnemonic.to_string().to_uppercase();
                    if mnemonic_str == "IMMS" || mnemonic_str == "VPUSH" {
                        // Special handling for Amount literals using `amount_macros::amount!`
                        let literal_token = expr.to_token_stream();
                        quote! {  
                            { amount_macros::amount!(#literal_token) }.to_u128_raw() as u128,
                        }
                    } else {
                        // Standard literal (e.g., StackPos, integer IDs)
                        quote! { #expr as u128, }
                    }
                }
                InstructionArg::Constant(ident) => {
                    // Non-register identifier (e.g., POS_OFFSET, VIO ID constant)
                    quote! { #ident as u128, }
                }
            };
            final_tokens.extend(arg_tokens);
        }
    }

    // Wrap the results in vec![]
    let output = quote! {
        vec![
            #final_tokens
        ]
    };

    output.into()
}