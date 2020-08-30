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
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse2,
    visit::{self, Visit},
    Arm, DeriveInput, Ident, ImplItemMethod, ItemImpl,
};

pub(crate) fn make_derive_commandable(item: DeriveInput) -> TokenStream2 {
    let ident = item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    quote! {
        impl #impl_generics ::command::CommandHandler for #ident #ty_generics #where_clause {
            fn handle_command(&mut self, command: &::command::Command) {
                self.command_handler_inner(command);
            }
        }
    }
}

pub(crate) fn make_commandable_attribute(item: ItemImpl) -> TokenStream2 {
    let ty = &item.self_ty;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let mut visitor = CommandCollectorVisitor::new();
    visitor.visit_item_impl(&item);

    let mut arms = Vec::new();
    for item in visitor.commands {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { #name => self.#item(command) }).unwrap();
        arms.push(arm);
    }

    quote! {
        impl #impl_generics #ty #ty_generics #where_clause {
            fn command_handler_inner(&mut self, command: &::command::Command) {
                match command.command() {
                    #(#arms),*,
                    _ => {
                        log::warn!("Unknown command '{}' passed to {}", command.full(), stringify!(#ty));
                    }
                };
            }
        }

        #item
    }
}

struct CommandCollectorVisitor {
    commands: Vec<Ident>,
}

impl CommandCollectorVisitor {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for CommandCollectorVisitor {
    fn visit_impl_item_method(&mut self, node: &'ast ImplItemMethod) {
        for attr in &node.attrs {
            if attr.path.is_ident("command") {
                self.commands.push(node.sig.ident.clone());
                break;
            }
        }
        visit::visit_impl_item_method(self, node);
    }
}
