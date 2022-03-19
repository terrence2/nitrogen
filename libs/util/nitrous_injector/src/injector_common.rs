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
use quote::{quote, ToTokens};
use std::borrow::Borrow;
use syn::{
    parse2,
    visit::{self, Visit},
    Arm, Expr, FnArg, GenericArgument, Ident, ImplItemMethod, ItemFn, ItemImpl, Pat, PathArguments,
    ReturnType, Type, TypePath,
};

pub(crate) fn make_augment_method(item: ItemFn) -> TokenStream2 {
    quote! {
        #[allow(clippy::unnecessary_wraps)]
        #item
    }
}

pub(crate) type Ast = ItemImpl;

pub(crate) struct InjectModel {
    pub(crate) item: ItemImpl,
    pub(crate) methods: Vec<(Ident, Vec<ArgDef>, RetType)>,
    pub(crate) _getters: Vec<Ident>,
    pub(crate) _setters: Vec<Ident>,
}

impl InjectModel {
    pub fn new(ast: Ast, visitor: CollectorVisitor) -> Self {
        Self {
            item: ast,
            methods: visitor.methods,
            _getters: visitor.getters,
            _setters: visitor.setters,
        }
    }
}

pub(crate) struct Ir {
    pub(crate) item: ItemImpl,
    pub(crate) method_arms: Vec<Arm>,
    pub(crate) get_arms: Vec<Arm>,
    pub(crate) put_arms: Vec<Arm>,
    pub(crate) names: Vec<String>,
    pub(crate) list_items: Vec<String>,
}

impl Ir {
    pub(crate) fn new(item: ItemImpl) -> Self {
        Self {
            item,
            method_arms: Vec::new(),
            get_arms: Vec::new(),
            put_arms: Vec::new(),
            names: Vec::new(),
            list_items: Vec::new(),
        }
    }
}

pub(crate) fn lower_methods<F>(
    methods: Vec<(Ident, Vec<ArgDef>, RetType)>,
    ir: &mut Ir,
    make_get_arm: F,
) where
    F: Fn(&str, &str) -> Arm,
{
    let type_name = ir.item.self_ty.clone().into_token_stream().to_string();
    for (ident, args, ret) in methods {
        let name = format!("{}", ident);
        ir.names.push(name.clone());
        ir.list_items.push(format!(
            "{}({})",
            name,
            args.iter()
                .map(|a| format!("{}", a.name))
                .collect::<Vec<String>>()
                .join(", ")
        ));
        ir.get_arms.push(make_get_arm(&type_name, &name));
        let arg_exprs = args
            .iter()
            .enumerate()
            .map(|(i, arg)| lower_arg(i, arg))
            .collect::<Vec<_>>();
        ir.method_arms
            .push(lower_method_call(&name, ident, &arg_exprs, ret));
    }
}

pub(crate) fn lower_list<F>(ir: &mut Ir, make_get_arm: F)
where
    F: Fn(&str, &str) -> Arm,
{
    let type_name = ir.item.self_ty.clone().into_token_stream().to_string();
    ir.method_arms
        .push(parse2(quote! { "list" => { self.__show_list__() } }).unwrap());
    ir.get_arms.push(make_get_arm(&type_name, "list"));
    ir.list_items.push("list()".to_owned());
}

fn lower_arg(i: usize, arg: &ArgDef) -> Expr {
    match arg.ty {
        Scalar::Boolean => {
            parse2(quote! { args.get(#i).expect("not enough args").to_bool()? }).unwrap()
        }
        Scalar::Integer => {
            parse2(quote! { args.get(#i).expect("not enough args").to_int()? }).unwrap()
        }
        Scalar::Float => {
            parse2(quote! { args.get(#i).expect("not enough args").to_float()? }).unwrap()
        }
        Scalar::StrRef => {
            parse2(quote! { args.get(#i).expect("not enough args").to_str()? }).unwrap()
        }
        Scalar::String => {
            parse2(quote! { args.get(#i).expect("not enough args").to_str()?.to_owned() }).unwrap()
        }
        Scalar::GraticuleSurface => {
            parse2(quote! { args.get(#i).expect("not enough args").to_grat_surface()? }).unwrap()
        }
        Scalar::GraticuleTarget => {
            parse2(quote! { args.get(#i).expect("not enough args").to_grat_target()? }).unwrap()
        }
        Scalar::Value => parse2(quote! { args.get(#i).expect("not enough args").clone() }).unwrap(),
        Scalar::Unit => parse2(quote! { ::nitrous::Value::True() }).unwrap(),
        Scalar::HeapMut => parse2(quote! { heap }).unwrap(),
        Scalar::HeapRef => parse2(quote! { heap.as_ref() }).unwrap(),
    }
}

fn lower_method_call(name: &str, item: Ident, arg_exprs: &[Expr], ret: RetType) -> Arm {
    parse2(match ret {
        RetType::Nothing => {
            quote! { #name => { self.#item( #(#arg_exprs),* ); Ok(::nitrous::Value::True()) } }
        }
        RetType::Raw(llty) => match llty {
            Scalar::Boolean => {
                quote! { #name => { Ok(::nitrous::Value::Boolean(self.#item( #(#arg_exprs),* ))) } }
            }
            Scalar::Integer => {
                quote! { #name => { Ok(::nitrous::Value::Integer(self.#item( #(#arg_exprs),* ))) } }
            }
            Scalar::Float => {
                quote! { #name => { Ok(::nitrous::Value::Float(::nitrous::ordered_float::OrderedFloat(self.#item( #(#arg_exprs),* )))) } }
            }
            Scalar::String => {
                quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_exprs),* ))) } }
            }
            Scalar::StrRef => {
                quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_exprs),* ).to_owned())) } }
            }
            Scalar::GraticuleSurface => {
                quote! { #name => { Ok(::nitrous::Value::Graticule(self.#item( #(#arg_exprs),* ))) } }
            }
            Scalar::GraticuleTarget => {
                quote! { #name => { Ok(::nitrous::Value::Graticule(self.#item( #(#arg_exprs),* ).with_origin::<::geodesy::GeoSurface>())) } }
            }
            Scalar::Value => {
                quote! { #name => { Ok(self.#item( #(#arg_exprs),* )) } }
            }
            Scalar::Unit => {
                quote! { #name => { self.#item( #(#arg_exprs),* ); Ok(::nitrous::Value::True()) } }
            }
            Scalar::HeapMut | Scalar::HeapRef => {
                panic!("invalid return of heap type from method")
            }
        },
        RetType::ResultRaw(llty) => match llty {
            Scalar::Boolean => {
                quote! { #name => { Ok(::nitrous::Value::Boolean(self.#item( #(#arg_exprs),* )?)) } }
            }
            Scalar::Integer => {
                quote! { #name => { Ok(::nitrous::Value::Integer(self.#item( #(#arg_exprs),* )?)) } }
            }
            Scalar::Float => {
                quote! { #name => { Ok(::nitrous::Value::Float(::nitrous::ordered_float::OrderedFloat(self.#item( #(#arg_exprs),* )?))) } }
            }
            Scalar::String => {
                quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_exprs),* )?)) } }
            }
            Scalar::StrRef => {
                quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_exprs),* )?.to_owned())) } }
            }
            Scalar::GraticuleSurface => {
                quote! { #name => { Ok(::nitrous::Value::Graticule(self.#item( #(#arg_exprs),* )?)) } }
            }
            Scalar::GraticuleTarget => {
                quote! { #name => { Ok(::nitrous::Value::Graticule(self.#item( #(#arg_exprs),* )?.with_origin::<::geodesy::GeoSurface>())) } }
            }
            Scalar::Value => {
                quote! { #name => { self.#item( #(#arg_exprs),* ) } }
            }
            Scalar::Unit => {
                quote! { #name => { self.#item( #(#arg_exprs),* )?; Ok(::nitrous::Value::True()) } }
            }
            Scalar::HeapMut | Scalar::HeapRef => {
                panic!("invalid return of heap type from method")
            }
        },
    }).unwrap()
}

#[derive(Debug)]
pub(crate) struct ArgDef {
    pub(crate) name: Ident,
    pub(crate) ty: Scalar,
}

pub(crate) struct CollectorVisitor {
    pub(crate) methods: Vec<(Ident, Vec<ArgDef>, RetType)>,
    pub(crate) getters: Vec<Ident>,
    pub(crate) setters: Vec<Ident>,
}

impl CollectorVisitor {
    pub(crate) fn new() -> Self {
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
                let args = node
                    .sig
                    .inputs
                    .iter()
                    .filter(|arg| matches!(arg, FnArg::Typed(_)))
                    .map(|arg| match arg {
                        FnArg::Receiver(_) => panic!("already filtered out receivers"),
                        FnArg::Typed(pat_type) => {
                            if let Pat::Ident(ident) = pat_type.pat.borrow() {
                                ArgDef {
                                    name: ident.ident.clone(),
                                    ty: Scalar::from_type(pat_type.ty.borrow()),
                                }
                            } else {
                                panic!("only identifier patterns supported as nitrous method args")
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let ret = RetType::from_return_type(&node.sig.output);
                self.methods.push((node.sig.ident.clone(), args, ret));
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

#[derive(Debug)]
pub(crate) enum Scalar {
    Boolean,
    Integer,
    Float,
    String,
    StrRef,
    GraticuleSurface,
    GraticuleTarget,
    Value,
    Unit,
    HeapMut,
    HeapRef,
}

impl Scalar {
    pub(crate) fn from_type(ty: &Type) -> Self {
        if let Type::Path(p) = ty {
            Self::from_type_path(p)
        } else if let Type::Reference(r) = ty {
            match Self::from_type(&r.elem) {
                Scalar::StrRef => Scalar::StrRef,
                v => panic!(
                    "nitrous Scalar only support references to str, not: {:#?}",
                    v
                ),
            }
        } else if let Type::Tuple(tt) = ty {
            if tt.elems.is_empty() {
                Scalar::Unit
            } else {
                panic!("nitrous Scalar only supports the unit tuple type")
            }
        } else {
            panic!(
                "nitrous Scalar only supports path and ref types, not: {:#?}",
                ty
            );
        }
    }

    pub(crate) fn type_path_name(p: &TypePath) -> String {
        assert_eq!(
            p.path.segments.len(),
            1,
            "nitrous Scalar paths must have length 1 (e.g. builtins)"
        );
        p.path.segments.first().unwrap().ident.to_string()
    }

    pub(crate) fn from_type_path(p: &TypePath) -> Self {
        match Self::type_path_name(p).as_str() {
            "bool" => Scalar::Boolean,
            "i64" => Scalar::Integer,
            "f64" => Scalar::Float,
            "str" => Scalar::StrRef,
            "String" => Scalar::String,
            "Value" => Scalar::Value,
            "HeapMut" => Scalar::HeapMut,
            "HeapRef" => Scalar::HeapRef,
            "Graticule" => {
                if let PathArguments::AngleBracketed(args) =
                    &p.path.segments.first().unwrap().arguments
                {
                    let grat_arg = args.args.first().unwrap();
                    if let GenericArgument::Type(ty_inner) = grat_arg {
                        if let Type::Path(p_inner) = ty_inner {
                            match Self::type_path_name(p_inner).as_str() {
                                "GeoSurface" => Scalar::GraticuleSurface,
                                "Target" => Scalar::GraticuleTarget,
                                _ => panic!("nitrous scale Graticule does not know {:#?}", p_inner),
                            }
                        } else {
                            panic!("nitrous scalar Graticule expected one type path argument")
                        }
                    } else {
                        panic!("Result parameter must be a type");
                    }
                } else {
                    panic!("nitrous scalar Graticule expected one type argument")
                }
            }
            _ => panic!(
                "nitrous Scalar only supports basic types, not: {:#?}",
                p.path.segments.first().unwrap()
            ),
        }
    }
}

#[derive(Debug)]
pub(crate) enum RetType {
    Nothing,
    Raw(Scalar),
    ResultRaw(Scalar),
}

impl RetType {
    pub(crate) fn from_return_type(ret_ty: &ReturnType) -> Self {
        match ret_ty {
            ReturnType::Default => RetType::Nothing,
            ReturnType::Type(_, ref ty) => {
                if let Type::Path(p) = ty.borrow() {
                    match Scalar::type_path_name(p).as_str() {
                        "Result" => {
                            if let PathArguments::AngleBracketed(ab_generic_args) =
                                &p.path.segments.first().unwrap().arguments
                            {
                                let fallible_arg = ab_generic_args.args.first().unwrap();
                                if let GenericArgument::Type(ty_inner) = fallible_arg {
                                    RetType::ResultRaw(Scalar::from_type(ty_inner))
                                } else {
                                    panic!("Result parameter must be a type");
                                }
                            } else {
                                panic!("Result must have an angle bracketed argument");
                            }
                        }
                        _ => Self::Raw(Scalar::from_type(ty.borrow())),
                    }
                } else {
                    panic!("nitrous RetType only supports Result, None, Value, and basic types")
                }
            }
        }
    }
}
