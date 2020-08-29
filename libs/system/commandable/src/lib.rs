// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
mod commandable;

use crate::commandable::{make_commandable_attribute, make_derive_commandable};

use proc_macro::TokenStream;
use syn::{parse2, DeriveInput, ItemImpl};

/// Adds a derivation of the Commandable trait and associated handle_command
/// method. This method dispatches to `handle_command_inner`, which is built
/// using #[commandable] and #[command] attributes on the impl block.
#[proc_macro_derive(Commandable)]
pub fn derive_commandable(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    let output: proc_macro2::TokenStream = {
        let item: DeriveInput = parse2(input).unwrap();
        make_derive_commandable(item)
    };

    proc_macro::TokenStream::from(output)
}

/// Add to the top of an impl block to collect all #[command] methods and build
/// a dispatch method named `handle_command_inner`. Note that this is not the
/// external trait, which is built from #[derive(Commandable)] above the struct.
#[proc_macro_attribute]
pub fn commandable(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    let output: proc_macro2::TokenStream = {
        let item: ItemImpl = parse2(input).unwrap();
        make_commandable_attribute(item)
    };

    proc_macro::TokenStream::from(output)
}

/// Just a tag for commandable.
#[proc_macro_attribute]
pub fn command(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}
