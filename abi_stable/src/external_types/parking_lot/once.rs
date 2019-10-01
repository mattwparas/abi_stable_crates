//! Contains an ffi-safe wrapper for `parking_lot::Once`.
use std::{
    any::Any,
    fmt::{self,Debug},
    mem,
    panic::{self,AssertUnwindSafe},
};

use core_extensions::{matches};

use parking_lot::{Once as PLOnce,OnceState};

use super::{UnsafeOveralignedField,RAW_LOCK_SIZE};

use crate::{
    utils::{transmute_mut_reference},
    prefix_type::{PrefixTypeTrait,WithMetadata},
    sabi_types::StaticRef,
    std_types::{RResult,ROk,RErr},
};



///////////////////////////////////////////////////////////////////////////////

type OpaqueOnce=
    UnsafeOveralignedField<PLOnce,[u8;OM_PADDING]>;

const OM_PADDING:usize=RAW_LOCK_SIZE-mem::size_of::<PLOnce>();

const OPAQUE_ONCE:OpaqueOnce=
    OpaqueOnce::new(parking_lot::Once::new(),[0u8;OM_PADDING]);

#[allow(dead_code)]
fn assert_mutex_size(){
    let _assert_size:[();RAW_LOCK_SIZE-mem::size_of::<OpaqueOnce>()];
    let _assert_size:[();mem::size_of::<OpaqueOnce>()-RAW_LOCK_SIZE];
}

///////////////////////////////////////////////////////////////////////////////


/**
A synchronization primitive for running global initialization once.

# Example

```
use abi_stable::{
    external_types::{ROnce,RMutex},
    utils::leak_value,
};

static MUTEX:RMutex<usize>=RMutex::new(0);

// For some reason this is causing the compiler to panic (as of Rust 1.36).
// static ONCE:ROnce=ROnce::new();

static ONCE:ROnce=ROnce::NEW;

let guards=
    std::iter::repeat_with(||{
        std::thread::spawn(||{
            ONCE.call_once(||{
                *MUTEX.lock()+=1;
            })
        })
    })
    .take(20)
    .collect::<Vec<_>>();

for guard in guards{
    guard.join().unwrap();
}

assert_eq!(*MUTEX.lock(),1);

```


*/
#[repr(C)]
#[derive(StableAbi)]
pub struct ROnce{
    opaque_once:OpaqueOnce,
    vtable:StaticRef<VTable>,
}

impl ROnce{
    /// Constructs an ROnce.
    ///
    /// As of Rust 1.36 this can't be called in const contexts,
    /// because is causes the compiler to panic.
    ///
    /// You can declare static like this:`static ONCE:ROnce=ROnce::NEW;`.
    ///
    /// # Example
    ///
    /// ```
    /// use abi_stable::external_types::ROnce;
    ///
    /// // For some reason this is causing the compiler to panic (as of Rust 1.36).
    /// // static ONCE:ROnce=ROnce::new();
    ///
    /// static ONCE:ROnce=ROnce::NEW;
    /// 
    /// let once=ROnce::new();
    ///
    /// ```
    pub const fn new() -> ROnce{
        ROnce{
            opaque_once:OPAQUE_ONCE,
            vtable: WithMetadata::as_prefix(VTable::VTABLE),
        }
    }

    /// Constructs an ROnce.
    ///
    /// # Example
    ///
    /// ```
    /// use abi_stable::external_types::ROnce;
    ///
    /// // For some reason this is causing the compiler to panic (as of Rust 1.36).
    /// // static ONCE:ROnce=ROnce::new();
    ///
    /// static ONCE:ROnce=ROnce::NEW;
    ///
    /// ```
    pub const NEW:Self=
        ROnce{
            opaque_once:OPAQUE_ONCE,
            vtable: WithMetadata::as_prefix(VTable::VTABLE),
        };

    fn vtable(&self)->&'static VTable{
        self.vtable.get()
    }

/**
Gets the running state of this ROnce.

# Example

```
use abi_stable::external_types::parking_lot::once::{ROnce,ROnceState};

let once=ROnce::new();

assert_eq!(once.state(), ROnceState::New );

let _=std::panic::catch_unwind(||{
    once.call_once(|| panic!() );
});

assert!( once.state().poisoned() );

once.call_once_force(|_| () );

assert!( once.state().done() );

```
*/
    pub fn state(&self) -> ROnceState{
        self.vtable().state()(&self.opaque_once)
    }

/**
Runs an initialization function.

`f` will be run only if this is the first time this method has been called 
on this ROnce.

Once this function returns it is guaranteed that some closure passed 
to this method has run to completion.

# Panics

Panics in the closure will cause this ROnce to become poisoned,
and any future calls to this method will panic.

# Example

```
use abi_stable::external_types::ROnce;

let once=ROnce::new();
let mut counter=0usize;

once.call_once(|| counter+=1 );
once.call_once(|| counter+=1 );
once.call_once(|| counter+=1 );
once.call_once(|| counter+=1 );

assert_eq!(counter,1);

```
*/
    pub fn call_once<F>(&self, f: F) 
    where
        F: FnOnce()
    {
        let mut closure=Closure::without_state(f);
        let func=closure.func;
        let res=self.vtable().call_once()(
            &self.opaque_once,
            unsafe{ transmute_mut_reference::<Closure<_>,ErasedClosure>(&mut closure) },
            func
        );
        if let Err(e)=closure.panic {
            panic::resume_unwind(e);
        }
        if let RErr(())=res{
            panic!("This ROnce instantce is poisoned.");
        }
    }

/**
Runs an initialization function,even if the ROnce is poisoned.

This will keep trying to run different closures until one of them doesn't panic.

The ROnceState parameter describes whether the ROnce is New or Poisoned.


# Example

```
use abi_stable::external_types::ROnce;

use std::panic::{self,AssertUnwindSafe};

let once=ROnce::new();
let mut counter=0usize;

for i in 0..100 {
    let _=panic::catch_unwind(AssertUnwindSafe(||{
        once.call_once_force(|_|{
            if i < 6 {
                panic!();
            }
            counter=i;
        })
    }));
}

assert_eq!(counter,6);

```

*/
    pub fn call_once_force<F>(&self, f: F) 
    where
        F: FnOnce(ROnceState),
    {
        let mut closure=Closure::with_state(f);
        let func=closure.func;
        let res=self.vtable().call_once_force()(
            &self.opaque_once,
            unsafe{ transmute_mut_reference::<Closure<_>,ErasedClosure>(&mut closure) },
            func
        );
        if let Err(e)=closure.panic {
            panic::resume_unwind(e);
        }
        if let RErr(())=res{
            panic!("This ROnce instantce is poisoned.");
        }
    }
}

impl Debug for ROnce{
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{
        f.debug_struct("Once")
         .field("state", &self.state())
         .finish()
    }
}

impl Default for ROnce{
    #[inline]
    fn default()->Self{
        Self::new()
    }
}

unsafe impl Send for ROnce{}
unsafe impl Sync for ROnce{}


///////////////////////////////////////////////////////////////////////////////



/// Describes the running state of an ROnce.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug,StableAbi)]
pub enum ROnceState{
    /// An ROnce that hasn't started running
    New,
    /// An ROnce that panicked inside `call_once*`
    Poisoned,
    /// An ROnce that is the middle of calling `call_once*`
    InProgress,
    /// An ROnce that has already run.
    Done,   
}


impl ROnceState{
    /**
Whether the ROnce is poisoned,requiring call_once_force to run.

# Example

```
use abi_stable::external_types::ROnce;

let once=ROnce::new();

let _=std::panic::catch_unwind(||{
    once.call_once(|| panic!() );
});

assert!(once.state().poisoned());

```

    */
    pub fn poisoned(&self) -> bool{
        matches!( ROnceState::Poisoned=self )
    }

/**    
Whether the ROnce has already finished running.

# Example

```
use abi_stable::external_types::ROnce;

let once=ROnce::new();

once.call_once(|| () );

assert!(once.state().done());

```

*/
    pub fn done(&self) -> bool{
        matches!( ROnceState::Done=self )
    }
}

impl_from_rust_repr! {
    impl From<OnceState> for ROnceState {
        fn(this){
            match this {
                OnceState::New=>ROnceState::New,
                OnceState::Poisoned=>ROnceState::Poisoned,
                OnceState::InProgress=>ROnceState::InProgress,
                OnceState::Done=>ROnceState::Done,
            }
        }
    }
}

impl_into_rust_repr! {
    impl Into<OnceState> for ROnceState {
        fn(this){
            match this {
                ROnceState::New=>OnceState::New,
                ROnceState::Poisoned=>OnceState::Poisoned,
                ROnceState::InProgress=>OnceState::InProgress,
                ROnceState::Done=>OnceState::Done,
            }
        }
    }
}


///////////////////////////////////////////////////////////////////////////////


#[repr(C)]
#[derive(StableAbi)]
struct ErasedClosure;


struct Closure<F>{
    closure:Option<F>,
    panic:Result<(),Box<dyn Any + Send + 'static>>,
    func:RunClosure,
}

#[derive(StableAbi,Copy,Clone)]
#[repr(transparent)]
struct RunClosure{
    func:extern "C" fn(&mut ErasedClosure,ROnceState)->RResult<(),()>,
}


impl<F> Closure<F>{
    
    #[inline]
    pub fn without_state(function:F)-> Self
    where
        F: FnOnce(),
    {
        Self{
            closure:Some(function),
            panic:Ok(()),
            func:RunClosure{func:Self::run_call_once}
        }
    }

    #[inline]
    pub fn with_state(function:F)-> Self
    where
        F: FnOnce(ROnceState),
    {
        Self{
            closure:Some(function),
            panic:Ok(()),
            func:RunClosure{func:Self::run_call_once_forced}
        }
    }

    extern "C" fn run_call_once(this:&mut ErasedClosure,state:ROnceState)->RResult<(),()>
    where
        F: FnOnce(),
    {
        Self::run_call(this,state,|f,_| f() )
    }

    extern "C" fn run_call_once_forced(this:&mut ErasedClosure,state:ROnceState)->RResult<(),()>
    where
        F: FnOnce(ROnceState),
    {
        Self::run_call(this,state,|f,state| f(state) )
    }

    #[inline]
    fn run_call<'a,M>(this:&mut ErasedClosure,state:ROnceState,method:M)->RResult<(),()>
    where
        M: FnOnce(F,ROnceState),
    {
        let this=unsafe{ transmute_mut_reference::<ErasedClosure,Self>(this) };
        let res=panic::catch_unwind(AssertUnwindSafe(||{
            let closure=this.closure.take().unwrap();
            method(closure,state);
        }));
        let ret=match res {
            Ok{..}=>ROk(()),
            Err{..}=>RErr(()),
        };
        this.panic=res;
        ret
    }
}




///////////////////////////////////////////////////////////////////////////////


#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_struct="VTable")))]
#[sabi(missing_field(panic))]
struct VTableVal{
    state:extern "C" fn(&OpaqueOnce)->ROnceState,
    call_once:extern "C" fn(&OpaqueOnce,&mut ErasedClosure,RunClosure)->RResult<(),()>,
    call_once_force:extern "C" fn(&OpaqueOnce,&mut ErasedClosure,RunClosure)->RResult<(),()>,
}

impl VTable{
    // The VTABLE for this type in this executable/library
    const VTABLE: StaticRef<WithMetadata<VTableVal>> = {
        StaticRef::new(&WithMetadata::new(
            PrefixTypeTrait::METADATA,
            VTableVal{
                state,
                call_once,
                call_once_force
            }
        ))
    };
}



///////////////////////////////////////////////////////////////////////////////


extern "C" fn state(this:&OpaqueOnce)->ROnceState{
    extern_fn_panic_handling!{
        this.value.state().into()
    }
}
extern "C" fn call_once(
    this:&OpaqueOnce,
    erased_closure:&mut ErasedClosure,
    runner:RunClosure,
)->RResult<(),()>{
    call_with_closure(||{
        this.value.call_once(||->(){
            (runner.func)(erased_closure,ROnceState::New).unwrap();
        });
    })
}
extern "C" fn call_once_force(
    this:&OpaqueOnce,
    erased_closure:&mut ErasedClosure,
    runner:RunClosure,
)->RResult<(),()>{
    call_with_closure(||{
        this.value.call_once_force(|state|->(){
            (runner.func)(erased_closure,state.into()).unwrap();
        });
    })
}


#[inline]
fn call_with_closure<F>(f:F)->RResult<(),()>
where
    F:FnOnce()
{
    let res=panic::catch_unwind(AssertUnwindSafe(f));
    match res {
        Ok{..}=>ROk(()),
        Err{..}=>RErr(()),
    }
}


///////////////////////////////////////////////////////////////////////////////



#[cfg(all(test,not(feature="only_new_tests")))]
//#[cfg(test)]
mod tests{
    use super::*;

    use std::{
        thread,
        time::Duration,
    };

    use crossbeam_utils::thread::scope as scoped_thread;

    use abi_stable_shared::{file_span,test_utils::{must_panic}};

    #[test]
    fn state(){
        {
            let once=ROnce::new();
            assert_eq!(once.state(), ROnceState::New);
            once.call_once(||{});
            assert_eq!(once.state(), ROnceState::Done);
        }
        {
            let once=ROnce::new();
            assert_eq!(once.state(), ROnceState::New);
            must_panic(file_span!(),||{
                once.call_once(|| panic!() );
            }).unwrap();
            assert_eq!(once.state(), ROnceState::Poisoned);
        }
        {
            static ONCE:ROnce=ROnce::new();

            scoped_thread(|scope|{
                scope.spawn(|_|{
                    ONCE.call_once(||{
                        thread::sleep(Duration::from_millis(500));
                    })
                });
                scope.spawn(|_|{
                    thread::sleep(Duration::from_millis(5));
                    assert_eq!(ONCE.state(), ROnceState::InProgress);
                });
            }).unwrap();
            assert_eq!(ONCE.state(), ROnceState::Done);
        }
    }
    #[test]
    fn call_once(){
        {
            let once=ROnce::new();
            let mut a=0;
            once.call_once(|| a+=1 );
            once.call_once(|| a+=2 );
            once.call_once(|| panic!() );
            assert_eq!(a, 1);
        }
        {
            let once=ROnce::new();
            let mut a=0;
            must_panic(file_span!(),||{
                once.call_once(|| panic!() );
            }).unwrap();
            must_panic(file_span!(),||{
                once.call_once(|| a+=2 );
            }).unwrap();
            assert_eq!(a, 0);
        }
    }

    #[test]
    fn call_once_force(){
        {
            let once=ROnce::new();
            let mut a=0;
            once.call_once_force(|_| a+=1 );
            once.call_once_force(|_| a+=2 );
            assert_eq!(a, 1);
        }
        {
            let once=ROnce::new();
            let a=&mut 0;
            must_panic(file_span!(),||{
                once.call_once_force(|state|{
                    assert_eq!(state, ROnceState::New);
                    panic!()
                });
            }).unwrap();
            once.call_once_force(|state|{
                assert_eq!(state, ROnceState::Poisoned);
                *a+=2;
            });
            once.call_once_force(|_| *a+=4 );
            once.call_once_force(|_| panic!() );
            assert_eq!(*a, 2);
        }
    }


}