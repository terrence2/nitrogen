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
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse2, Generics, Ident};

pub(crate) type Ast = syn::DeriveInput;

pub(crate) struct ResourceModel {
    ident: Ident,
    generics: Generics,
}

pub(crate) fn parse(input: TokenStream) -> Ast {
    parse2(TokenStream2::from(input)).expect("parse result")
}

pub(crate) fn analyze(ast: Ast) -> ResourceModel {
    ResourceModel {
        ident: ast.ident.clone(),
        generics: ast.generics,
    }
}

pub(crate) fn lower(model: ResourceModel) -> ResourceModel {
    model
}

pub(crate) fn codegen(model: ResourceModel) -> TokenStream {
    let ident = model.ident;
    let (impl_generics, ty_generics, where_clause) = model.generics.split_for_impl();
    proc_macro::TokenStream::from(quote! {
        impl #impl_generics ::nitrous::ScriptResource for #ident #ty_generics #where_clause
        {
            fn resource_type_name(&self) -> String {
                stringify!(#ident).to_owned()
            }

            fn call_method(&mut self, name: &str, args: &[::nitrous::Value]) -> ::anyhow::Result<::nitrous::Value> {
                self.__call_method_inner__(name, args)
            }

            fn put(&mut self, name: &str, value: ::nitrous::Value) -> ::anyhow::Result<()> {
                self.__put_inner__(name, value)
            }

            fn get(&self, name: &str) -> ::anyhow::Result<::nitrous::Value> {
                self.__get_inner__(name)
            }

            fn names(&self) -> Vec<&str> {
                self.__names_inner__()
            }
        }
    })
}
