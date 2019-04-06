pub use crate::{
    abi_stability::{
        stable_abi_trait::{
            MakeGetAbiInfo, SharedStableAbi, SharedStableAbi_Bound, StableAbi, StableAbi_Bound,
            UnsafeOpaqueField_Bound,
        },
        type_layout::{
            LifetimeIndex, TLData, TLEnumVariant, TLField, TypeLayout, TypeLayoutParams,
        },
    },
    utils::as_slice,
    version::VersionStrings,
};

pub use core_extensions::type_level_bool::{False, True};

pub mod renamed {
    pub use super::{
        LifetimeIndex::Param as __LIParam, LifetimeIndex::Static as __LIStatic,
        MakeGetAbiInfo as __MakeGetAbiInfo, SharedStableAbi as __SharedStableAbi,
        SharedStableAbi_Bound as __SharedStableAbi_Bound, StableAbi as __StableAbi,
        StableAbi_Bound as __StableAbi_Bound, TLData as __TLData, TLEnumVariant as __TLEnumVariant,
        TLField as __TLField, TypeLayoutParams as __TypeLayoutParams,
        UnsafeOpaqueField_Bound as __UnsafeOpaqueField_Bound,
    };
}

#[macro_export]
macro_rules! instantiate_field {
    (
        $name:expr,[ $($indices:tt)* ],$ty:ty as $trait:ident
    )=> ({
        use $crate::reexports::renamed::*;

        __TLField::new(
            $name,
            &[$($indices)*],
            <$ty as $trait>::ABI_INFO.get(),
        )
    })
}