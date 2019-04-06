/*!
Contains a function for extracting information about the lifetime parameters
of an `extern fn`,and requires all lifetimes to be unelided
to some extent(for non-reference types at least),

*/

use std::{collections::BTreeSet, mem};

use syn::{
    punctuated::Punctuated,
    visit_mut::{self, VisitMut},
    Abi, GenericParam, Generics, Ident, Lifetime, LitStr, Type, TypeBareFn, TypeReference,
};

use arrayvec::ArrayString;

use proc_macro2::{Span,TokenStream};

use quote::{ToTokens, TokenStreamExt};

use core_extensions::prelude::*;

use hashbrown::HashSet;

use crate::{
    lifetimes::LifetimeIndex,
    to_token_fn::ToTokenFnMut,
    ignored_wrapper::Ignored,
};
use crate::*;

#[derive(Clone, Debug, PartialEq, Hash)]
pub(crate) struct FnInfo<'a> {
    /// The generics of the struct this function pointer type is used inside of.
    parent_generics: &'a Generics,

    /// The identifiers for all the lifetimes of the
    /// struct this function pointer type is used inside of
    env_lifetimes: Vec<&'a Ident>,

    /// The index of first lifetime declared by all functions.
    /// (not one it references from the struct/enum definition it is used inside of).
    initial_bound_lifetime: usize,

    pub functions: Vec<Function<'a>>,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub(crate) struct Function<'a> {
    pub(crate) named_bound_lts: Vec<&'a Ident>,
    pub(crate) named_bound_lt_set: Ignored<HashSet<&'a Ident>>,
    pub(crate) named_bound_lts_count:usize,

    pub(crate) params: Vec<FnParamRet<'a>>,
    /// None if its return type is `()`.
    pub(crate) returns: Option<FnParamRet<'a>>,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub(crate) struct FnParamRet<'a> {
    /// The lifetimes this type references (including static).
    pub(crate) lifetime_refs: Vec<LifetimeIndex>,
    pub(crate) ty: &'a Type,
    pub(crate) param_or_ret: ParamOrReturn,
}

impl<'a> FnParamRet<'a>{
    pub fn unit_ret(arenas:&'a Arenas)->Self{
        let unit=syn::TypeTuple{
            paren_token:Default::default(),
            elems:Punctuated::default(),
        };
        FnParamRet{
            lifetime_refs:Vec::new(),
            ty:arenas.alloc( syn::Type::from(unit)  ),
            param_or_ret:ParamOrReturn::Return,
        }
    }

    pub fn lifetime_refs_tokenizer(&self,ctokens:&'a CommonTokens<'a>)->FnParamRetLtRefTokens<'_>{
        FnParamRetLtRefTokens{
            lifetime_refs:&self.lifetime_refs,
            ctokens,
        }
    }
}


pub(crate) struct FnParamRetLtRefTokens<'a> {
    lifetime_refs: &'a [LifetimeIndex],
    ctokens:&'a CommonTokens<'a>,
}


impl<'a> ToTokens for FnParamRetLtRefTokens<'a> {
    fn to_tokens(&self, ts: &mut TokenStream) {
        for refs in self.lifetime_refs {
            refs.tokenizer(self.ctokens).to_tokens(ts);
            self.ctokens.comma.to_tokens(ts);
        }
    }
}


pub(crate) struct VisitFieldRet {
    pub(crate) referenced_lifetimes: Vec<LifetimeIndex>,
}


/////////////


impl<'a> Function<'a>{
    /// Removes generic parameters in `ty` referencing this function.
    pub fn remove_generics_from(&self,ty:&mut Type,ctokens:&'a CommonTokens<'a>){
        GenericRemover{func:self,ctokens}
            .visit_type_mut(ty);
    }
}


struct GenericRemover<'a>{
    func:&'a Function<'a>,
    ctokens:&'a CommonTokens<'a>,
}


impl<'a> VisitMut for GenericRemover<'a>{
    fn visit_lifetime_mut(&mut self, lt: &mut Lifetime) {
        if self.func.named_bound_lt_set.contains(&lt.ident) {
            lt.ident=self.ctokens.underscore.clone();
        }
    }
}


/////////////

impl<'a> TypeVisitor<'a> {
    #[inline(never)]
    pub fn new(arenas: &'a Arenas, ctokens: &'a CommonTokens<'a>, generics: &'a Generics) -> Self {
        TypeVisitor {
            refs: ImmutableRefs {
                arenas,
                ctokens,
                env_generics: generics,
            },
            vars: Vars {
                referenced_lifetimes: Vec::default(),
                fn_info: FnInfo {
                    parent_generics: &generics,
                    env_lifetimes: generics.lifetimes().map(|lt| &lt.lifetime.ident).collect(),
                    initial_bound_lifetime: generics.lifetimes().count(),
                    functions: Vec::new(),
                },
            },
        }
    }

    pub fn visit_field(&mut self, ty: &mut Type) -> VisitFieldRet {
        self.visit_type_mut(ty);

        VisitFieldRet {
            referenced_lifetimes: mem::replace(&mut self.vars.referenced_lifetimes, Vec::new()),
        }
    }

    pub fn into_fn_info(self)->FnInfo<'a>{
        self.vars.fn_info
    }
}

/////////////

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate)enum ParamOrReturn {
    Param,
    Return,
}

pub(crate) struct TypeVisitor<'a> {
    refs: ImmutableRefs<'a>,
    vars: Vars<'a>,
}

#[derive(Copy, Clone)]
struct ImmutableRefs<'a> {
    arenas: &'a Arenas,
    ctokens: &'a CommonTokens<'a>,
    /// Generics provided by the environment (eg:the struct this type is used inside of).
    env_generics: &'a Generics,
}

struct Vars<'a> {
    /// What lifetimes in env_lifetimes are referenced in the type being visited.
    /// For TLField.
    referenced_lifetimes: Vec<LifetimeIndex>,
    fn_info: FnInfo<'a>,
}

struct FnVisitor<'a, 'b> {
    refs: ImmutableRefs<'a>,
    vars: &'b mut Vars<'a>,

    current: Function<'a>,
    param_ret: FnParamRetBuilder,
}

struct FnParamRetBuilder {
    /// The lifetimes this type references (including static).
    lifetime_refs: Vec<LifetimeIndex>,
    param_or_ret: ParamOrReturn,
}

/////////////

impl FnParamRetBuilder {
    fn new(param_or_ret: ParamOrReturn) -> Self {
        Self {
            lifetime_refs: Vec::new(),
            param_or_ret,
        }
    }
}

/////////////

impl<'a> Vars<'a> {
    pub fn add_referenced_env_lifetime(&mut self, ind: LifetimeIndex) {
        let is_env_lt = match ind {
            LifetimeIndex::Static => true,
            LifetimeIndex::Param { index } => index < self.fn_info.env_lifetimes.len(),
        };
        if is_env_lt {
            self.referenced_lifetimes.push(ind);
        }
    }
}

/////////////

impl<'a> VisitMut for TypeVisitor<'a> {
    #[inline(never)]
    fn visit_type_bare_fn_mut(&mut self, func: &mut TypeBareFn) {
        let ctokens = self.refs.ctokens;
        let arenas = self.refs.arenas;

        let abi = func
            .abi
            .as_ref()
            .map(|x| x.name.as_ref().unwrap_or(&ctokens.c_abi_lit));

        if abi != Some(&ctokens.c_abi_lit) {
            match abi {
                Some(abi) => panic!("abi not supported for function pointers:\n{:?}\n", abi),
                None => panic!("the default abi is not supported for function pointers"),
            }
        }

        let named_bound_lts: Vec<&'a Ident> = func
            .lifetimes
            .take() // Option<BoundLifetimes>
            .into_iter()
            .flat_map(|lt| lt.lifetimes)
            .map(|lt| arenas.alloc(lt.lifetime.ident))
            .collect::<Vec<&'a Ident>>();
        let named_bound_lt_set=named_bound_lts.iter().cloned().collect();

        let mut current_function = FnVisitor {
            refs: self.refs,
            vars: &mut self.vars,
            current: Function {
                named_bound_lts_count:named_bound_lts.len(),
                named_bound_lts,
                named_bound_lt_set:Ignored::new(named_bound_lt_set),
                params: Vec::new(),
                returns: None,
            },
            param_ret: FnParamRetBuilder::new(ParamOrReturn::Param),
        };

        fn visit_ty<'a, 'b>(
            this: &mut FnVisitor<'a, 'b>,
            ty: &'a mut Type,
            param_or_ret: ParamOrReturn,
        ) {
            this.param_ret = FnParamRetBuilder::new(param_or_ret);

            this.visit_type_mut(ty);

            let param_ret = mem::replace(&mut this.param_ret, FnParamRetBuilder::new(param_or_ret));

            let param_ret = FnParamRet {
                lifetime_refs: param_ret.lifetime_refs,
                ty: ty,
                param_or_ret: param_ret.param_or_ret,
            };

            match param_or_ret {
                ParamOrReturn::Param => this.current.params.push(param_ret),
                ParamOrReturn::Return => this.current.returns = Some(param_ret),
            }
        }

        for param in mem::replace(&mut func.inputs, Punctuated::new()) {
            let ty = arenas.alloc_mut(param.ty);
            visit_ty(&mut current_function, ty, ParamOrReturn::Param);
        }

        match mem::replace(&mut func.output, syn::ReturnType::Default) {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => Some(arenas.alloc_mut(*ty)),
        }
        .map(|ty| visit_ty(&mut current_function, ty, ParamOrReturn::Return));

        let current=current_function.current;
        self.vars.fn_info.functions.push(current);
    }

    #[inline(never)]
    fn visit_lifetime_mut(&mut self, lt: &mut Lifetime) {
        let ctokens = self.refs.ctokens;
        let lt = &lt.ident;
        if *lt == ctokens.static_ {
            LifetimeIndex::Static
        } else {
            let env_lifetimes = self.vars.fn_info.env_lifetimes.iter();
            let found_lt = env_lifetimes.enumerate().position(|(_, lt_ident)| *lt_ident == lt);
            match found_lt {
                Some(index) => LifetimeIndex::Param { index },
                None => panic!("unknown lifetime:'{}", (&*lt).into_token_stream()),
            }
        }
        .piped(|lt| self.vars.add_referenced_env_lifetime(lt))
    }
}

/////////////

impl<'a, 'b> FnVisitor<'a, 'b> {
    #[inline(never)]
    fn setup_lifetime(&mut self, lt: Option<&Ident>) -> Option<&'a Ident> {
        // let arenas=self.ty_visitor.arenas;
        let ctokens = self.refs.ctokens;
        let mut ret: Option<&'a Ident> = None;
        if lt == Some(&ctokens.static_) {
            LifetimeIndex::Static
        } else if lt.map_or(true,|lt| lt== &ctokens.underscore) {
            match self.param_ret.param_or_ret {
                ParamOrReturn::Param => {
                    self.new_bound_lifetime()
                },
                ParamOrReturn::Return => 
                    match self.current.named_bound_lts_count {
                        0 => panic!(
                            "\nattempted to use an elided lifetime  in the \
                             return type when there are no lifetimes \
                             used in any parameter:\n\
                             {}\n\
                             ",
                            lt.unwrap_or(&ctokens.underscore)
                        ),
                        1=> {
                            LifetimeIndex::Param {
                                index: self.vars.fn_info.initial_bound_lifetime,
                            }
                        }
                        _ => panic!(
                            "\nattempted to use an elided lifetime in the \
                             return type when there are multiple lifetimes used \
                             in parameters :\n\
                             {}\n\
                             ",
                            lt.unwrap_or(&ctokens.underscore)
                        ),
                    },
            }
        } else {
            let lt=lt.unwrap();
            let env_lts = self.vars.fn_info.env_lifetimes.iter();
            let fn_lts = self.current.named_bound_lts.iter();
            let found_lt = env_lts.chain(fn_lts).position(|ident| *ident == lt);
            match found_lt {
                Some(index) => {
                    ret = Some(&ctokens.underscore);
                    LifetimeIndex::Param { index }
                }
                None => panic!("unknown lifetime:'{}", (&*lt).into_token_stream()),
            }
        }
        .piped(|li| {
            self.param_ret.lifetime_refs.push(li);
            self.vars.add_referenced_env_lifetime(li);
        });
        ret
    }
    /// Adds a bound lifetime to the `extern fn()` and returns an index to it
    fn new_bound_lifetime(&mut self) -> LifetimeIndex {
        let index = self.vars.fn_info.initial_bound_lifetime+self.current.named_bound_lts_count;
        self.current.named_bound_lts_count+=1;
        LifetimeIndex::Param { index }
    }
}

impl<'a, 'b> VisitMut for FnVisitor<'a, 'b> {
    #[inline(never)]
    fn visit_type_bare_fn_mut(&mut self, func: &mut TypeBareFn) {
        panic!(
            "\n\
             This library does not currently support nested function pointers\n\
             nested function pointer:\n\t{func}\n\
             To use the function pointer as a parameter define a wrapper type:\n\
             \t#[derive(StableAbi)]\n\
             \t#[repr(transparent)] \n\
             \tpub struct CallbackParam{{   \n\
             \t\tpub func:{func}\n\
             \t}}\n\
             \n\
             ",
            func = (&func).into_token_stream()
        );
    }

    fn visit_type_reference_mut(&mut self, ref_: &mut TypeReference) {
        let ctokens = self.refs.ctokens;
        let lt = ref_.lifetime.as_ref().map(|x| &x.ident);
        if let Some(ident) = self.setup_lifetime(lt).cloned() {
            if let Some(lt)=&mut ref_.lifetime {
                lt.ident = ident
            }
        }

        visit_mut::visit_type_mut(self, &mut ref_.elem)
    }

    fn visit_lifetime_mut(&mut self, lt: &mut Lifetime) {
        if let Some(ident) = self.setup_lifetime(Some(&lt.ident)) {
            lt.ident = ident.clone();
        }
    }
}