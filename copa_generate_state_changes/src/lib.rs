#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]

extern crate proc_macro;

use std::iter::Peekable;

use proc_macro2::TokenTree::{Group, Literal, Punct};
use proc_macro2::{token_stream, TokenStream, TokenTree};
use quote::quote;

/// Create a `const fn` which will return an array with all state changes.
#[proc_macro]
pub fn generate_state_changes(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Convert from proc_macro -> proc_macro2
    let item: TokenStream = item.into();
    let mut iter = item.into_iter().peekable();

    // Determine output function name
    let fn_name = iter.next().unwrap();

    // Separator between name and body with state changes
    expect_punct(&mut iter, ',');

    // Create token stream to assign each state change to the array
    let assignments_stream = states_stream(&mut iter);

    quote!(
        const fn #fn_name() -> [[u8; 256]; 16] {
            let mut state_changes = [[0; 256]; 16];

            #assignments_stream

            state_changes
        }
    )
    .into()
}

/// Generate the array assignment statements for all origin states.
fn states_stream(iter: &mut impl Iterator<Item = TokenTree>) -> TokenStream {
    let mut states_stream = next_group(iter).into_iter().peekable();

    // Loop over all origin state entries
    let mut tokens = quote!();
    while states_stream.peek().is_some() {
        // Add all mappings for this state
        tokens.extend(state_entry_stream(&mut states_stream));

        // Allow trailing comma
        optional_punct(&mut states_stream, ',');
    }
    tokens
}

/// Generate the array assignment statements for one origin state.
fn state_entry_stream(iter: &mut Peekable<token_stream::IntoIter>) -> TokenStream {
    // Origin state name
    let state = iter.next().unwrap();

    // Token stream with all the byte->target mappings
    let mut changes_stream = next_group(iter).into_iter().peekable();

    let mut tokens = quote!();
    while changes_stream.peek().is_some() {
        // Add next mapping for this state
        tokens.extend(change_stream(&mut changes_stream, &state));

        // Allow trailing comma
        optional_punct(&mut changes_stream, ',');
    }
    tokens
}

/// Generate the array assignment statement for a single byte->target mapping for one state.
fn change_stream(iter: &mut Peekable<token_stream::IntoIter>, state: &TokenTree) -> TokenStream {
    // Start of input byte range
    let start = next_usize(iter);

    // End of input byte range
    let end = if optional_punct(iter, '.') {
        // Read inclusive end of range
        expect_punct(iter, '.');
        expect_punct(iter, '=');
        next_usize(iter)
    } else {
        // Without range, end is equal to start
        start
    };

    // Separator between byte input range and output state
    expect_punct(iter, '=');
    expect_punct(iter, '>');

    // Token stream with target state and action
    let mut target_change_stream = next_group(iter).into_iter().peekable();

    let mut tokens = quote!();
    while target_change_stream.peek().is_some() {
        // Target state/action for all bytes in the range
        let (target_state, target_action) = target_change(&mut target_change_stream);

        // Create a new entry for every byte in the range
        for byte in start..=end {
            // TODO: Force adding `State::` and `Action::`?
            // TODO: Should we really use `pack` here without import?
            tokens.extend(quote!(
                state_changes[State::#state as usize][#byte] =
                    pack(State::#target_state, Action::#target_action);
            ));
        }
    }
    tokens
}

/// Get next target state and action.
fn target_change(iter: &mut Peekable<token_stream::IntoIter>) -> (TokenTree, TokenTree) {
    let target_state = iter.next().unwrap();

    // Separator between state and action
    expect_punct(iter, ',');

    let target_action = iter.next().unwrap();

    (target_state, target_action)
}

/// Check if next token matches specific punctuation.
fn optional_punct(iter: &mut Peekable<token_stream::IntoIter>, c: char) -> bool {
    match iter.peek() {
        Some(Punct(punct)) if punct.as_char() == c => iter.next().is_some(),
        _ => false,
    }
}

/// Ensure next token matches specific punctuation.
///
/// # Panics
///
/// Panics if the punctuation does not match.
fn expect_punct(iter: &mut impl Iterator<Item = TokenTree>, c: char) {
    match iter.next() {
        Some(Punct(ref punct)) if punct.as_char() == c => (),
        token => panic!("Expected punctuation '{}', but got {:?}", c, token),
    }
}

/// Get next token as [`usize`].
///
/// # Panics
///
/// Panics if the next token is not a [`usize`] in hex or decimal literal format.
fn next_usize(iter: &mut impl Iterator<Item = TokenTree>) -> usize {
    match iter.next() {
        Some(Literal(literal)) => {
            let literal = literal.to_string();
            if let Some(prefix) = literal.strip_prefix("0x") {
                usize::from_str_radix(prefix, 16).unwrap()
            } else {
                literal.parse::<usize>().unwrap()
            }
        },
        token => panic!("Expected literal, but got {:?}", token),
    }
}

/// Get next token as [`Group`].
///
/// # Panics
///
/// Panics if the next token is not a [`Group`].
fn next_group(iter: &mut impl Iterator<Item = TokenTree>) -> TokenStream {
    match iter.next() {
        Some(Group(group)) => group.stream(),
        token => panic!("Expected group, but got {:?}", token),
    }
}
