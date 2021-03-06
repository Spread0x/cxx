use crate::syntax::atom::Atom::{self, *};
use crate::syntax::report::Errors;
use crate::syntax::types::TrivialReason;
use crate::syntax::{
    error, ident, Api, Array, Enum, ExternFn, ExternType, Impl, Lang, Receiver, Ref, Signature,
    SliceRef, Struct, Trait, Ty1, Type, TypeAlias, Types,
};
use proc_macro2::{Delimiter, Group, Ident, TokenStream};
use quote::{quote, ToTokens};
use std::fmt::Display;

pub(crate) struct Check<'a> {
    apis: &'a [Api],
    types: &'a Types<'a>,
    errors: &'a mut Errors,
}

pub(crate) fn typecheck(cx: &mut Errors, apis: &[Api], types: &Types) {
    do_typecheck(&mut Check {
        apis,
        types,
        errors: cx,
    });
}

fn do_typecheck(cx: &mut Check) {
    ident::check_all(cx, cx.apis);

    for ty in cx.types {
        match ty {
            Type::Ident(ident) => check_type_ident(cx, &ident.rust),
            Type::RustBox(ptr) => check_type_box(cx, ptr),
            Type::RustVec(ty) => check_type_rust_vec(cx, ty),
            Type::UniquePtr(ptr) => check_type_unique_ptr(cx, ptr),
            Type::SharedPtr(ptr) => check_type_shared_ptr(cx, ptr),
            Type::CxxVector(ptr) => check_type_cxx_vector(cx, ptr),
            Type::Ref(ty) => check_type_ref(cx, ty),
            Type::Array(array) => check_type_array(cx, array),
            Type::Fn(ty) => check_type_fn(cx, ty),
            Type::SliceRef(ty) => check_type_slice_ref(cx, ty),
            Type::Str(_) | Type::Void(_) => {}
        }
    }

    for api in cx.apis {
        match api {
            Api::Include(_) => {}
            Api::Struct(strct) => check_api_struct(cx, strct),
            Api::Enum(enm) => check_api_enum(cx, enm),
            Api::CxxType(ety) | Api::RustType(ety) => check_api_type(cx, ety),
            Api::CxxFunction(efn) | Api::RustFunction(efn) => check_api_fn(cx, efn),
            Api::TypeAlias(alias) => check_api_type_alias(cx, alias),
            Api::Impl(imp) => check_api_impl(cx, imp),
        }
    }
}

impl Check<'_> {
    pub(crate) fn error(&mut self, sp: impl ToTokens, msg: impl Display) {
        self.errors.error(sp, msg);
    }
}

fn check_type_ident(cx: &mut Check, ident: &Ident) {
    if Atom::from(ident).is_none()
        && !cx.types.structs.contains_key(ident)
        && !cx.types.enums.contains_key(ident)
        && !cx.types.cxx.contains(ident)
        && !cx.types.rust.contains(ident)
    {
        let msg = format!("unsupported type: {}", ident);
        cx.error(ident, &msg);
    }
}

fn check_type_box(cx: &mut Check, ptr: &Ty1) {
    if let Type::Ident(ident) = &ptr.inner {
        if cx.types.cxx.contains(&ident.rust)
            && !cx.types.aliases.contains_key(&ident.rust)
            && !cx.types.structs.contains_key(&ident.rust)
            && !cx.types.enums.contains_key(&ident.rust)
        {
            cx.error(ptr, error::BOX_CXX_TYPE.msg);
        }

        if Atom::from(&ident.rust).is_none() {
            return;
        }
    }

    cx.error(ptr, "unsupported target type of Box");
}

fn check_type_rust_vec(cx: &mut Check, ty: &Ty1) {
    if let Type::Ident(ident) = &ty.inner {
        if cx.types.cxx.contains(&ident.rust)
            && !cx.types.aliases.contains_key(&ident.rust)
            && !cx.types.structs.contains_key(&ident.rust)
            && !cx.types.enums.contains_key(&ident.rust)
        {
            cx.error(ty, "Rust Vec containing C++ type is not supported yet");
            return;
        }

        match Atom::from(&ident.rust) {
            None | Some(Char) | Some(U8) | Some(U16) | Some(U32) | Some(U64) | Some(Usize)
            | Some(I8) | Some(I16) | Some(I32) | Some(I64) | Some(Isize) | Some(F32)
            | Some(F64) | Some(RustString) => return,
            Some(Bool) => { /* todo */ }
            Some(CxxString) => {}
        }
    }

    cx.error(ty, "unsupported element type of Vec");
}

fn check_type_unique_ptr(cx: &mut Check, ptr: &Ty1) {
    if let Type::Ident(ident) = &ptr.inner {
        if cx.types.rust.contains(&ident.rust) {
            cx.error(ptr, "unique_ptr of a Rust type is not supported yet");
            return;
        }

        match Atom::from(&ident.rust) {
            None | Some(CxxString) => return,
            _ => {}
        }
    } else if let Type::CxxVector(_) = &ptr.inner {
        return;
    }

    cx.error(ptr, "unsupported unique_ptr target type");
}

fn check_type_shared_ptr(cx: &mut Check, ptr: &Ty1) {
    if let Type::Ident(ident) = &ptr.inner {
        if cx.types.rust.contains(&ident.rust) {
            cx.error(ptr, "shared_ptr of a Rust type is not supported yet");
            return;
        }

        match Atom::from(&ident.rust) {
            None => return,
            Some(CxxString) => {
                cx.error(ptr, "std::shared_ptr<std::string> is not supported yet");
                return;
            }
            _ => {}
        }
    } else if let Type::CxxVector(_) = &ptr.inner {
        cx.error(ptr, "std::shared_ptr<std::vector> is not supported yet");
        return;
    }

    cx.error(ptr, "unsupported shared_ptr target type");
}

fn check_type_cxx_vector(cx: &mut Check, ptr: &Ty1) {
    if let Type::Ident(ident) = &ptr.inner {
        if cx.types.rust.contains(&ident.rust) {
            cx.error(
                ptr,
                "C++ vector containing a Rust type is not supported yet",
            );
            return;
        }

        match Atom::from(&ident.rust) {
            None | Some(U8) | Some(U16) | Some(U32) | Some(U64) | Some(Usize) | Some(I8)
            | Some(I16) | Some(I32) | Some(I64) | Some(Isize) | Some(F32) | Some(F64)
            | Some(CxxString) => return,
            Some(Char) => { /* todo */ }
            Some(Bool) | Some(RustString) => {}
        }
    }

    cx.error(ptr, "unsupported vector target type");
}

fn check_type_ref(cx: &mut Check, ty: &Ref) {
    if ty.mutable && !ty.pinned {
        if let Some(requires_pin) = match &ty.inner {
            Type::Ident(ident) if ident.rust == CxxString || is_opaque_cxx(cx, &ident.rust) => {
                Some(ident.rust.to_string())
            }
            Type::CxxVector(_) => Some("CxxVector<...>".to_owned()),
            _ => None,
        } {
            cx.error(
                ty,
                format!(
                    "mutable reference to C++ type requires a pin -- use Pin<&mut {}>",
                    requires_pin,
                ),
            );
        }
    }

    match ty.inner {
        Type::Fn(_) | Type::Void(_) => {}
        Type::Ref(_) => {
            cx.error(ty, "C++ does not allow references to references");
            return;
        }
        _ => return,
    }

    cx.error(ty, "unsupported reference type");
}

fn check_type_slice_ref(cx: &mut Check, ty: &SliceRef) {
    let supported = match &ty.inner {
        Type::Str(_) | Type::SliceRef(_) => false,
        element => !is_unsized(cx, element),
    };

    if !supported {
        let mutable = if ty.mutable { "mut " } else { "" };
        let mut msg = format!("unsupported &{}[T] element type", mutable);
        if let Type::Ident(ident) = &ty.inner {
            if cx.types.rust.contains(&ident.rust) {
                msg += ": opaque Rust type is not supported yet";
            } else if is_opaque_cxx(cx, &ident.rust) {
                msg += ": opaque C++ type is not supported yet";
            }
        }
        cx.error(ty, msg);
    }
}

fn check_type_array(cx: &mut Check, ty: &Array) {
    let supported = match &ty.inner {
        Type::Str(_) | Type::SliceRef(_) => false,
        element => !is_unsized(cx, element),
    };

    if !supported {
        cx.error(ty, "unsupported array element type");
    }
}

fn check_type_fn(cx: &mut Check, ty: &Signature) {
    if ty.throws {
        cx.error(ty, "function pointer returning Result is not supported yet");
    }
}

fn check_api_struct(cx: &mut Check, strct: &Struct) {
    let name = &strct.name;
    check_reserved_name(cx, &name.rust);

    if strct.fields.is_empty() {
        let span = span_for_struct_error(strct);
        cx.error(span, "structs without any fields are not supported");
    }

    if cx.types.cxx.contains(&name.rust) {
        if let Some(ety) = cx.types.untrusted.get(&name.rust) {
            let msg = "extern shared struct must be declared in an `unsafe extern` block";
            cx.error(ety, msg);
        }
    }

    for derive in &strct.derives {
        if derive.what == Trait::ExternType {
            let msg = format!("derive({}) on shared struct is not supported", derive);
            cx.error(derive, msg);
        }
    }

    for field in &strct.fields {
        if let Type::Fn(_) = field.ty {
            cx.error(
                field,
                "function pointers in a struct field are not implemented yet",
            );
        } else if is_unsized(cx, &field.ty) {
            let desc = describe(cx, &field.ty);
            let msg = format!("using {} by value is not supported", desc);
            cx.error(field, msg);
        }
    }
}

fn check_api_enum(cx: &mut Check, enm: &Enum) {
    check_reserved_name(cx, &enm.name.rust);

    if enm.variants.is_empty() && !enm.explicit_repr {
        let span = span_for_enum_error(enm);
        cx.error(
            span,
            "explicit #[repr(...)] is required for enum without any variants",
        );
    }

    for derive in &enm.derives {
        if derive.what == Trait::Default || derive.what == Trait::ExternType {
            let msg = format!("derive({}) on shared enum is not supported", derive);
            cx.error(derive, msg);
        }
    }
}

fn check_api_type(cx: &mut Check, ety: &ExternType) {
    check_reserved_name(cx, &ety.name.rust);

    for derive in &ety.derives {
        if derive.what == Trait::ExternType && ety.lang == Lang::Rust {
            continue;
        }
        let lang = match ety.lang {
            Lang::Rust => "Rust",
            Lang::Cxx => "C++",
        };
        let msg = format!(
            "derive({}) on opaque {} type is not supported yet",
            derive, lang,
        );
        cx.error(derive, msg);
    }

    if !ety.bounds.is_empty() {
        let bounds = &ety.bounds;
        let span = quote!(#(#bounds)*);
        cx.error(span, "extern type bounds are not implemented yet");
    }

    if let Some(reason) = cx.types.required_trivial.get(&ety.name.rust) {
        let what = match reason {
            TrivialReason::StructField(strct) => format!("a field of `{}`", strct.name.rust),
            TrivialReason::FunctionArgument(efn) => format!("an argument of `{}`", efn.name.rust),
            TrivialReason::FunctionReturn(efn) => format!("a return value of `{}`", efn.name.rust),
            TrivialReason::BoxTarget => format!("Box<{}>", ety.name.rust),
            TrivialReason::VecElement => format!("a vector element in Vec<{}>", ety.name.rust),
        };
        let msg = format!(
            "needs a cxx::ExternType impl in order to be used as {}",
            what,
        );
        cx.error(ety, msg);
    }
}

fn check_api_fn(cx: &mut Check, efn: &ExternFn) {
    match efn.lang {
        Lang::Cxx => {
            if !efn.generics.params.is_empty() && !efn.trusted {
                let ref span = span_for_generics_error(efn);
                cx.error(span, "extern C++ function with lifetimes must be declared in `unsafe extern \"C++\"` block");
            }
        }
        Lang::Rust => {
            if !efn.generics.params.is_empty() && efn.unsafety.is_none() {
                let ref span = span_for_generics_error(efn);
                let message = format!(
                    "must be `unsafe fn {}` in order to expose explicit lifetimes to C++",
                    efn.name.rust,
                );
                cx.error(span, message);
            }
        }
    }

    if let Some(receiver) = &efn.receiver {
        let ref span = span_for_receiver_error(receiver);

        if receiver.ty.is_self() {
            let mutability = match receiver.mutable {
                true => "mut ",
                false => "",
            };
            let msg = format!(
                "unnamed receiver type is only allowed if the surrounding \
                 extern block contains exactly one extern type; \
                 use `self: &{mutability}TheType`",
                mutability = mutability,
            );
            cx.error(span, msg);
        } else if !cx.types.structs.contains_key(&receiver.ty.rust)
            && !cx.types.cxx.contains(&receiver.ty.rust)
            && !cx.types.rust.contains(&receiver.ty.rust)
        {
            cx.error(span, "unrecognized receiver type");
        } else if receiver.mutable && !receiver.pinned && is_opaque_cxx(cx, &receiver.ty.rust) {
            cx.error(
                span,
                format!(
                    "mutable reference to C++ type requires a pin -- use `self: Pin<&mut {}>`",
                    receiver.ty.rust,
                ),
            );
        }
    }

    for arg in &efn.args {
        if let Type::Fn(_) = arg.ty {
            if efn.lang == Lang::Rust {
                cx.error(
                    arg,
                    "passing a function pointer from C++ to Rust is not implemented yet",
                );
            }
        } else if is_unsized(cx, &arg.ty) {
            let desc = describe(cx, &arg.ty);
            let msg = format!("passing {} by value is not supported", desc);
            cx.error(arg, msg);
        }
    }

    if let Some(ty) = &efn.ret {
        if let Type::Fn(_) = ty {
            cx.error(ty, "returning a function pointer is not implemented yet");
        } else if is_unsized(cx, ty) {
            let desc = describe(cx, ty);
            let msg = format!("returning {} by value is not supported", desc);
            cx.error(ty, msg);
        }
    }

    if efn.lang == Lang::Cxx {
        check_mut_return_restriction(cx, efn);
    }

    check_multiple_arg_lifetimes(cx, efn);
}

fn check_api_type_alias(cx: &mut Check, alias: &TypeAlias) {
    for derive in &alias.derives {
        let msg = format!("derive({}) on extern type alias is not supported", derive);
        cx.error(derive, msg);
    }
}

fn check_api_impl(cx: &mut Check, imp: &Impl) {
    let ty = &imp.ty;

    if let Some(negative) = imp.negative_token {
        let span = quote!(#negative #ty);
        cx.error(span, "negative impl is not supported yet");
        return;
    }

    match ty {
        Type::RustBox(ty)
        | Type::RustVec(ty)
        | Type::UniquePtr(ty)
        | Type::SharedPtr(ty)
        | Type::CxxVector(ty) => {
            if let Type::Ident(inner) = &ty.inner {
                if Atom::from(&inner.rust).is_none() {
                    return;
                }
            }
        }
        _ => {}
    }

    cx.error(imp, "unsupported Self type of explicit impl");
}

fn check_mut_return_restriction(cx: &mut Check, efn: &ExternFn) {
    match &efn.ret {
        Some(Type::Ref(ty)) if ty.mutable => {}
        _ => return,
    }

    for arg in &efn.args {
        if let Type::Ref(ty) = &arg.ty {
            if ty.mutable {
                return;
            }
        }
    }

    cx.error(
        efn,
        "&mut return type is not allowed unless there is a &mut argument",
    );
}

fn check_multiple_arg_lifetimes(cx: &mut Check, efn: &ExternFn) {
    if efn.lang == Lang::Cxx && efn.trusted {
        return;
    }

    match &efn.ret {
        Some(Type::Ref(_)) => {}
        _ => return,
    }

    let mut reference_args = 0;
    for arg in &efn.args {
        if let Type::Ref(_) = &arg.ty {
            reference_args += 1;
        }
    }

    if efn.receiver.is_some() {
        reference_args += 1;
    }

    if reference_args != 1 {
        cx.error(
            efn,
            "functions that return a reference must take exactly one input reference",
        );
    }
}

fn check_reserved_name(cx: &mut Check, ident: &Ident) {
    if ident == "Box"
        || ident == "UniquePtr"
        || ident == "SharedPtr"
        || ident == "Vec"
        || ident == "CxxVector"
        || ident == "str"
        || Atom::from(ident).is_some()
    {
        cx.error(ident, "reserved name");
    }
}

fn is_unsized(cx: &mut Check, ty: &Type) -> bool {
    match ty {
        Type::Ident(ident) => {
            let ident = &ident.rust;
            ident == CxxString || is_opaque_cxx(cx, ident) || cx.types.rust.contains(ident)
        }
        Type::Array(array) => is_unsized(cx, &array.inner),
        Type::CxxVector(_) | Type::Fn(_) | Type::Void(_) => true,
        Type::RustBox(_)
        | Type::RustVec(_)
        | Type::UniquePtr(_)
        | Type::SharedPtr(_)
        | Type::Ref(_)
        | Type::Str(_)
        | Type::SliceRef(_) => false,
    }
}

fn is_opaque_cxx(cx: &mut Check, ty: &Ident) -> bool {
    cx.types.cxx.contains(ty)
        && !cx.types.structs.contains_key(ty)
        && !cx.types.enums.contains_key(ty)
        && !(cx.types.aliases.contains_key(ty) && cx.types.required_trivial.contains_key(ty))
}

fn span_for_struct_error(strct: &Struct) -> TokenStream {
    let struct_token = strct.struct_token;
    let mut brace_token = Group::new(Delimiter::Brace, TokenStream::new());
    brace_token.set_span(strct.brace_token.span);
    quote!(#struct_token #brace_token)
}

fn span_for_enum_error(enm: &Enum) -> TokenStream {
    let enum_token = enm.enum_token;
    let mut brace_token = Group::new(Delimiter::Brace, TokenStream::new());
    brace_token.set_span(enm.brace_token.span);
    quote!(#enum_token #brace_token)
}

fn span_for_receiver_error(receiver: &Receiver) -> TokenStream {
    let ampersand = receiver.ampersand;
    let lifetime = &receiver.lifetime;
    let mutability = receiver.mutability;
    if receiver.shorthand {
        let var = receiver.var;
        quote!(#ampersand #lifetime #mutability #var)
    } else {
        let ty = &receiver.ty;
        quote!(#ampersand #lifetime #mutability #ty)
    }
}

fn span_for_generics_error(efn: &ExternFn) -> TokenStream {
    let unsafety = efn.unsafety;
    let fn_token = efn.fn_token;
    let generics = &efn.generics;
    quote!(#unsafety #fn_token #generics)
}

fn describe(cx: &mut Check, ty: &Type) -> String {
    match ty {
        Type::Ident(ident) => {
            if cx.types.structs.contains_key(&ident.rust) {
                "struct".to_owned()
            } else if cx.types.enums.contains_key(&ident.rust) {
                "enum".to_owned()
            } else if cx.types.aliases.contains_key(&ident.rust) {
                "C++ type".to_owned()
            } else if cx.types.cxx.contains(&ident.rust) {
                "opaque C++ type".to_owned()
            } else if cx.types.rust.contains(&ident.rust) {
                "opaque Rust type".to_owned()
            } else if Atom::from(&ident.rust) == Some(CxxString) {
                "C++ string".to_owned()
            } else if Atom::from(&ident.rust) == Some(Char) {
                "C char".to_owned()
            } else {
                ident.rust.to_string()
            }
        }
        Type::RustBox(_) => "Box".to_owned(),
        Type::RustVec(_) => "Vec".to_owned(),
        Type::UniquePtr(_) => "unique_ptr".to_owned(),
        Type::SharedPtr(_) => "shared_ptr".to_owned(),
        Type::Ref(_) => "reference".to_owned(),
        Type::Str(_) => "&str".to_owned(),
        Type::CxxVector(_) => "C++ vector".to_owned(),
        Type::SliceRef(_) => "slice".to_owned(),
        Type::Fn(_) => "function pointer".to_owned(),
        Type::Void(_) => "()".to_owned(),
        Type::Array(_) => "array".to_owned(),
    }
}
