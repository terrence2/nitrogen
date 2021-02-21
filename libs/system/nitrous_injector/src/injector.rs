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
    Arm, DeriveInput, Ident, ImplItemMethod, ItemFn, ItemImpl,
};

pub(crate) fn make_derive_nitrous_module(item: DeriveInput) -> TokenStream2 {
    let ident = item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    quote! {
        impl #impl_generics ::nitrous::Module for #ident #ty_generics #where_clause {
            fn module_name(&self) -> String {
                stringify!(#ident).to_owned()
            }

            fn call_method(&self, name: &str, args: &[Value]) -> Fallible<Value> {
                self.__call_method_inner__(name, args)
            }

            fn put(&mut self, module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Fallible<()> {
                self.__put_inner__(module, name, value)
            }

            fn get(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Fallible<Value> {
                self.__get_inner__(module, name)
            }
        }
    }
}

pub(crate) fn make_augment_method(item: ItemFn) -> TokenStream2 {
    quote! {
        #[allow(clippy::unnecessary_wraps)]
        #item
    }
}

pub(crate) fn make_inject_attribute(item: ItemImpl) -> TokenStream2 {
    let ty = &item.self_ty;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let mut visitor = CollectorVisitor::new();
    visitor.visit_item_impl(&item);

    let mut method_arms = Vec::new();
    let mut get_arms = Vec::new();
    let mut put_arms = Vec::new();

    for item in visitor.methods {
        let name = format!("{}", item);
        let call_arm: Arm = parse2(quote! { #name => self.#item() }).unwrap();
        method_arms.push(call_arm);
        let lookup_arm: Arm =
            parse2(quote! { #name => Value::Method(module, #name.to_owned()) }).unwrap();
        get_arms.push(lookup_arm);
    }

    for item in visitor.getters {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { #name => self.#item() }).unwrap();
        get_arms.push(arm);
    }

    for item in visitor.setters {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { #name => self.#item(value) }).unwrap();
        put_arms.push(arm);
    }

    quote! {
        impl #impl_generics #ty #ty_generics #where_clause {
            fn __call_method_inner__(&self, name: &str, args: &[Value]) -> Fallible<Value> {
                match name {
                    // Note: this first case makes the trailing comma valid if there are no
                    //       actual arms found above.
                    "" => {
                        log::warn!("Empty name passed to call_method on {}", stringify!(#ty));
                        bail!("Empty name passed to call_method on {}", stringify!(#ty))
                    }
                    #(#method_arms),*,
                    _ => {
                        log::warn!("Unknown call_method '{}' passed to {}", name, stringify!(#ty));
                        bail!("Unknown call_method '{}' passed to {}", name, stringify!(#ty))
                    }
                }
            }

            fn __get_inner__(&self, module: Arc<RwLock<dyn Module>>, name: &str) -> Fallible<Value> {
                Ok(match name {
                    // Note: this first case makes the trailing comma valid if there are no
                    //       actual arms found above.
                    "" => {
                        log::warn!("Empty name passed to get on {}", stringify!(#ty));
                        bail!("Empty name passed to get on {}", stringify!(#ty))
                    }
                    #(#get_arms),*,
                    _ => {
                        log::warn!("Unknown get '{}' passed to {}", name, stringify!(#ty));
                        bail!("Unknown get '{}' passed to {}", name, stringify!(#ty))
                    }
                })
            }

            fn __put_inner__(&mut self, module: Arc<RwLock<dyn Module>>, name: &str, value: Value) -> Fallible<()> {
                match name {
                    // Note: this first case makes the trailing comma valid if there are no
                    //       actual arms found above.
                    "" => {
                        log::warn!("Empty name passed to put on {}", stringify!(#ty));
                        bail!("Empty name passed to put on {}", stringify!(#ty))
                    }
                    #(#put_arms),*,
                    _ => {
                        log::warn!("Unknown put '{}' passed to {}", name, stringify!(#ty));
                        bail!("Unknown put '{}' passed to {}", name, stringify!(#ty))
                    }
                }
            }
        }

        #item
    }
}

struct CollectorVisitor {
    methods: Vec<Ident>,
    getters: Vec<Ident>,
    setters: Vec<Ident>,
}

impl CollectorVisitor {
    fn new() -> Self {
        Self {
            methods: Vec::new(),
            getters: Vec::new(),
            setters: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for CollectorVisitor {
    fn visit_impl_item_method(&mut self, node: &'ast ImplItemMethod) {
        for attr in &node.attrs {
            if attr.path.is_ident("method") {
                self.methods.push(node.sig.ident.clone());
                break;
            } else if attr.path.is_ident("getter") {
                self.getters.push(node.sig.ident.clone());
                break;
            } else if attr.path.is_ident("setter") {
                self.setters.push(node.sig.ident.clone());
                break;
            }
        }
        visit::visit_impl_item_method(self, node);
    }
}
