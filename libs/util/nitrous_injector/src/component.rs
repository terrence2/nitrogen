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
use crate::injector_common::{
    find_properties_in_struct, make_property_get_arm, make_property_put_arm, Scalar,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse2, Arm, Generics, Ident};

pub(crate) type Ast = syn::DeriveInput;

pub(crate) struct ComponentModel {
    component_name: String,
    ident: Ident,
    generics: Generics,
    properties: Vec<(Ident, Scalar)>,
    getter_arms: Vec<Arm>,
    putter_arms: Vec<Arm>,
}

pub(crate) fn parse(input: TokenStream) -> Ast {
    parse2(TokenStream2::from(input)).expect("parse result")
}

pub(crate) fn analyze(ast: Ast) -> ComponentModel {
    let properties = find_properties_in_struct(&ast);
    let mut component_name = String::new();
    for attr in ast.attrs {
        if attr.path.is_ident("Name") {
            let tmp = attr.tokens.into_iter().skip(1).collect::<Vec<_>>()[0].to_string();
            component_name = tmp[1..tmp.len() - 1].to_owned();
        }
    }
    ComponentModel {
        component_name,
        ident: ast.ident.clone(),
        generics: ast.generics,
        properties,
        getter_arms: Vec::new(),
        putter_arms: Vec::new(),
    }
}

pub(crate) fn lower(mut model: ComponentModel) -> ComponentModel {
    model.getter_arms = model
        .properties
        .iter()
        .map(|(name, ty)| make_property_get_arm(&name.to_string(), name, ty))
        .collect::<Vec<Arm>>();
    model.putter_arms = model
        .properties
        .iter()
        .map(|(name, ty)| make_property_put_arm(&name.to_string(), name, ty))
        .collect::<Vec<Arm>>();
    model
}

pub(crate) fn codegen(model: ComponentModel) -> TokenStream {
    let ComponentModel {
        ident,
        component_name,
        getter_arms,
        putter_arms,
        ..
    } = model;
    let (impl_generics, ty_generics, where_clause) = model.generics.split_for_impl();
    proc_macro::TokenStream::from(quote! {
        impl #impl_generics ::nitrous::ScriptComponent for #ident #ty_generics #where_clause
        {
            fn component_name(&self) -> &'static str {
                #component_name
            }

            fn call_method(
                &mut self,
                name: &str,
                args: &[::nitrous::Value],
                heap: ::nitrous::HeapMut
            ) -> ::nitrous::anyhow::Result<::nitrous::CallResult> {
                self.__call_method_inner__(name, args, heap)
            }

            fn put(&mut self, entity: Entity, name: &str, value: ::nitrous::Value) -> ::nitrous::anyhow::Result<()> {
                match name {
                    #(#putter_arms)*
                    _ => {
                        self.__put_inner__(entity, name, value)
                    }
                }
            }

            fn get(&self, entity: Entity, name: &str) -> ::nitrous::anyhow::Result<::nitrous::Value> {
                match name {
                    #(#getter_arms)*
                    _ => {
                        self.__get_inner__(entity, name)
                    }
                }
            }

            fn names(&self) -> Vec<&str> {
                self.__names_inner__()
            }
        }
    })
}
