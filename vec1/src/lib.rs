
use std::fmt;
use std::ops::{ Deref, DerefMut };
use std::result::{ Result as StdResult };
use std::error::{ Error as StdError };
use std::vec::IntoIter;
use std::iter::IntoIterator;

#[macro_export]
macro_rules! vec1 {
    ( $first:expr) => (
         $crate::Vec1::new( $first )
    );
    ( $first:expr,) => (
         $crate::Vec1::new( $first )
    );
    ( $first:expr, $($item:expr),* ) => ({
        let mut tmp = $crate::Vec1::new( $first );
        $( tmp.push( $item ); )*
        tmp
    });
}



#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub struct Size0Error;

impl fmt::Display for Size0Error {
    fn fmt( &self, fter: &mut fmt::Formatter ) -> fmt::Result {
        write!( fter, "{:?}", self )
    }
}
impl StdError for Size0Error {
    fn description(&self) -> &str {
        "failing function call would have reduced the size of a Vec1 to 0, which is not allowed"
    }
}

type Vec1Result<T> = StdResult<T, Size0Error>;

#[derive( Debug, Clone, Eq,  Hash )]
pub struct Vec1<T>(Vec<T>);

impl<T> IntoIterator for Vec1<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter( self ) -> Self::IntoIter {
        self.0.into_iter()
    }

}

impl<T> Vec1<T> {


    pub fn new( first: T  ) -> Self {
        Vec1( vec![ first ] )
    }

    pub fn from_vec( vec: Vec<T> ) -> StdResult<Self, Vec<T>> {
        if vec.len() > 0 {
            Ok( Vec1( vec ) )
        } else {
            Err( vec )
        }
    }

    pub fn with_capacity( first: T, capacity: usize ) -> Self {
        let mut vec = Vec::with_capacity( capacity );
        vec.push( first );
        Vec1( vec )
    }

    pub fn into_vec( self ) -> Vec<T> {
        self.0
    }


    /// returns a reference to the last element
    /// as Vec1 contains always at last one element
    /// there is always a last element
    pub fn last( &self ) -> &T {
        //UNWRAP_SAFE: len is at last 1
        self.0.last().unwrap()
    }

    pub fn last_mut( &mut self ) -> &mut T {
        //UNWRAP_SAFE: len is at last 1
        self.0.last_mut().unwrap()
    }

    pub fn try_truncate(&mut self, len: usize) -> Vec1Result<()> {
        if len > 0 {
            self.0.truncate( len );
            Ok( () )
        } else {
            Err( Size0Error )
        }
    }

    pub fn try_swap_remove(&mut self, index: usize) -> Vec1Result<T> {
        if self.len() > 1 {
            Ok( self.0.swap_remove( index ) )
        } else {
            Err( Size0Error )
        }
    }

    pub fn try_remove( &mut self, index: usize ) -> Vec1Result<T> {
        if self.len() > 1 {
            Ok( self.0.remove( index ) )
        } else {
            Err( Size0Error )
        }
    }

    pub fn try_split_off(&mut self, at: usize) -> Vec1Result<Vec1<T>> {
        if at == 0 {
            Err(Size0Error)
        } else if at >= self.len() {
            Err(Size0Error)
        } else {
            let out = self.0.split_off(at);
            Ok(Vec1(out))
        }
    }

    pub fn dedup_by_key<F, K>(&mut self, key: F)
        where F: FnMut(&mut T) -> K,
              K: PartialEq<K>
    {
        self.0.dedup_by_key( key )
    }

    pub fn dedup_by<F>(&mut self, same_bucket: F)
        where F: FnMut(&mut T, &mut T) -> bool
    {
        self.0.dedup_by( same_bucket )
    }


    /// pops if there is _more_ than 1 element in the vector
    pub fn pop(&mut self) -> Option<T> {
        if self.len() > 1 {
            self.0.pop()
        } else {
            None
        }
    }

    pub fn as_vec(&self) -> &Vec<T> {
        &self.0
    }

}

macro_rules! impl_wrapper {
    (pub $T:ident>
        $(fn $name:ident(&$($m:ident)* $(, $param:ident: $tp:ty)*) -> $rt:ty);*) => (
            impl<$T> Vec1<$T> {$(
                #[inline]
                pub fn $name(self: impl_wrapper!{__PRIV_SELF &$($m)*} $(, $param: $tp)*) -> $rt {
                    (self.0).$name($($param),*)
                }
            )*}
    );
    (__PRIV_SELF &mut self) => (&mut Self);
    (__PRIV_SELF &self) => (&Self);
}

impl_wrapper! {
    pub T>
        fn reserve(&mut self, additional: usize) -> ();
        fn reserve_exact(&mut self, additional: usize) -> ();
        fn shrink_to_fit(&mut self) -> ();
        fn as_mut_slice(&mut self) -> &mut [T];
        fn push(&mut self, value: T) -> ();
        fn append(&mut self, other: &mut Vec<T>) -> ();
        fn insert(&mut self, idx: usize, val: T) -> ();
        fn len(&self) -> usize;
        fn capacity(&self) -> usize;
        fn as_slice(&self) -> &[T]
}


impl<T> Vec1<T> where T: Clone {
    pub fn try_resize(&mut self, new_len: usize, value: T) -> Vec1Result<()> {
        if new_len >= 1 {
            Ok( self.0.resize( new_len, value ) )
        } else {
            Err( Size0Error )
        }
    }

    pub fn extend_from_slice(&mut self, other: &[T]) {
        self.0.extend_from_slice( other )
    }
}

impl<T> Vec1<T> where T: PartialEq<T> {
    pub fn dedub(&mut self) {
        self.0.dedup()
    }
}


impl<T> Vec1<T> where T: PartialEq<T> {
    pub fn dedup(&mut self) {
        self.0.dedup()
    }
}


impl<T> Deref for Vec1<T> {
    type Target = [T];

    fn deref( &self ) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Vec1<T> {
    fn deref_mut( &mut self ) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Into<Vec<T>> for Vec1<T> {

    fn into( self ) -> Vec<T> {
        self.0
    }
}

impl<A, B> PartialEq<Vec1<B>> for Vec1<A>
    where A: PartialEq<B>
{
    fn eq(&self, other: &Vec1<B>) -> bool {
        self.0.eq(&other.0)
    }
}

impl<A, B> PartialEq<B> for Vec1<A>
    where Vec<A>: PartialEq<B>
{
    fn eq(&self, other: &B) -> bool {
        self.0.eq(other)
    }
}




#[cfg(test)]
mod test {

    #[macro_export]
    macro_rules! assert_ok {
        ($val:expr) => ({
            match $val {
                Ok( res ) => res,
                Err( err ) => panic!( "expected Ok(..) got Err({:?})", err)
            }
        });
        ($val:expr, $ctx:expr) => ({
            match $val {
                Ok( res ) => res,
                Err( err ) => panic!( "expected Ok(..) got Err({:?}) [ctx: {:?}]", err, $ctx)
            }
        });
    }

    macro_rules! assert_err {
        ($val:expr) => ({
            match $val {
                Ok( val ) => panic!( "expected Err(..) got Ok({:?})", val),
                Err( err ) => err,
            }
        });
        ($val:expr, $ctx:expr) => ({
            match $val {
                Ok( val ) => panic!( "expected Err(..) got Ok({:?}) [ctx: {:?}]", val, $ctx),
                Err( err ) => err,
            }
        });
    }

    mod Size0Error {
        #![allow(non_snake_case)]
        use super::super::*;

        #[test]
        fn implements_std_error() {
            fn comp_check<T: StdError>(){}
            comp_check::<Size0Error>();
        }
    }

    mod Vec1 {
        #![allow(non_snake_case)]
        use super::super::*;

        #[test]
        fn now_warning_on_empty_vec() {
            #![deny(warnings)]

            let _ = vec1![1u8,];
            let _ = vec1![1u8];

        }

        #[test]
        fn deref_slice() {
            let vec = Vec1::new(1u8);
            let _: &[u8] = &*vec;
        }

        #[test]
        fn deref_slice_mut() {
            let mut vec = Vec1::new(1u8);
            let _: &mut [u8] = &mut *vec;
        }

        #[test]
        fn provided_all_ro_functions() {
            let vec = Vec1::new(1u8);
            assert_eq!(vec.len(), 1);
            assert!(vec.capacity() > 0);
            assert_eq!(vec.as_slice(), &*vec);
            // there is obviously no reason we should provide this,
            // as it can't be empty at all, that's the point behind Vec1
            //assert_eq!(vec.is_empty(), true)
        }

        #[test]
        fn provides_some_safe_mut_functions() {
            let mut vec = Vec1::new(1u8);
            vec.reserve(12);
            assert!(vec.capacity() >= 13);
            vec.reserve_exact(31);
            assert!(vec.capacity() >= 31);
            vec.shrink_to_fit();
            let _: &mut [u8] = vec.as_mut_slice();
            vec.insert(1, 31u8);
            vec.insert(1, 2u8);
            assert_eq!(&*vec, &[1, 2, 31]);
            vec.dedup_by_key(|k| *k/3);
            assert_eq!(&*vec, &[1, 31]);
            vec.push(31);
            assert_eq!(&*vec, &[1, 31, 31]);
            vec.dedup_by(|l,r| l == r);
            assert_eq!(&*vec, &[1, 31]);
            vec.extend_from_slice(&[31,2,3]);
            assert_eq!(&*vec, &[1, 31, 31, 2, 3]);
            vec.dedub();
            assert_eq!(&*vec, &[1, 31, 2, 3]);
            // as the passed in vec is emptied this won't work with a vec1 as parameter
            vec.append(&mut vec![1,2,3]);
            assert_eq!(&*vec, &[1, 31, 2, 3, 1, 2, 3])
        }

        #[test]
        fn provides_other_methos_in_failible_form() {
            let mut vec = vec1![1u8,2,3,4];
            assert_ok!(vec.try_truncate(3));
            assert_err!(vec.try_truncate(0));
            assert_eq!(vec, &[1,2,3]);

            assert_ok!(vec.try_swap_remove(0));
            assert_eq!(vec, &[3, 2]);
            assert_ok!(vec.try_remove(0));
            assert_eq!(vec, &[2]);
            assert_err!(vec.try_swap_remove(0));
            assert_err!(vec.try_remove(0));
            vec.push(12);

            assert_eq!(vec.pop(), Some(12));
            assert_eq!(vec.pop(), None);
            assert_eq!(vec, &[2]);

        }

        #[test]
        fn try_split_of() {
            let mut vec = vec1![1,2,3,4];
            assert_err!(vec.try_split_off(0));
            let len = vec.len();
            assert_err!(vec.try_split_off(len));
            let nvec = assert_ok!(vec.try_split_off(len-1));
            assert_eq!(vec, &[1,2,3]);
            assert_eq!(nvec, &[4]);
        }

        #[test]
        fn try_resize() {
            let mut vec = Vec1::new(1u8);
            assert_ok!(vec.try_resize(10,2u8));
            assert_eq!(vec.len(), 10);
            assert_ok!(vec.try_resize(1, 2u8));
            assert_eq!(vec, &[1]);
            assert_err!(vec.try_resize(0, 2u8));
        }


        #[test]
        fn with_capacity() {
            let vec = Vec1::with_capacity(1u8, 16);
            assert!(vec.capacity() >= 16);
        }
    }


}