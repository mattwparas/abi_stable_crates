use std::{
    fmt::{self,Display},
};

use syn::Ident;

use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt};

use crate::common_tokens::CommonTokens;


#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LifetimeIndex {
    Static,
    Param { index: usize },
}

impl LifetimeIndex {
    /// Produces the tokens of the type_layout::LifetimeIndex version of this type
    pub fn tokenizer<'a>(self,ctokens:&'a CommonTokens<'a>)->LifetimeIndexTokenizer<'a>{
        LifetimeIndexTokenizer{li:self,ctokens}
    }
}


pub struct LifetimeIndexTokenizer<'a>{
    li:LifetimeIndex,
    ctokens:&'a CommonTokens<'a>,
}


impl<'a> ToTokens for LifetimeIndexTokenizer<'a> {
    fn to_tokens(&self, ts: &mut TokenStream) {
        let ctokens=self.ctokens;
        match self.li {
            LifetimeIndex::Static=>{
                ctokens.li_static.to_tokens(ts);
            }
            LifetimeIndex::Param{index,..}=>{
                ctokens.li_index.to_tokens(ts);
                ctokens.paren.surround(ts,|ts| index.to_tokens(ts) );
            }
        }
    }
}


/////////////////////////////////


pub struct LifetimeTokenizer<'a>{
    ident:&'a Ident,
}

impl<'a> LifetimeTokenizer<'a>{
    pub fn from_ident(ident:&'a Ident)->Self{
        Self{ident}
    }
}

impl<'a> ToTokens for LifetimeTokenizer<'a> {
    fn to_tokens(&self, ts: &mut TokenStream) {
        use proc_macro2::{Punct, Spacing};
        let mut apostrophe = Punct::new('\'', Spacing::Joint);
        ts.append(apostrophe);
        self.ident.to_tokens(ts);
    }
}


