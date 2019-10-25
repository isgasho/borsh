use crate::Input;
use std::cmp::min;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Error};
use std::mem::size_of;

const ERROR_NOT_ALL_BYTES_READ: &str = "Not all bytes read";

/// A data-structure that can be de-serialized from binary format by NBOR.
pub trait BorshDeserialize: Sized {
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error>;

    /// Deserialize this instance from a slice of bytes.
    fn try_from_slice(v: &[u8]) -> Result<Self, Error> {
        let mut input = vec![0; v.len()];
        input.copy_from_slice(v);
        let mut input = &input[..];
        let result = Self::deserialize(&mut input)?;
        if input.rem_len()? > 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                ERROR_NOT_ALL_BYTES_READ,
            ));
        }
        Ok(result)
    }
}

impl BorshDeserialize for u8 {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let mut res = 0u8;
        input.read(std::slice::from_mut(&mut res))?;
        Ok(res)
    }
}

macro_rules! impl_for_integer {
    ($type: ident) => {
        impl BorshDeserialize for $type {
            #[inline]
            fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
                let mut data = [0u8; size_of::<$type>()];
                input.read(&mut data)?;
                Ok($type::from_le_bytes(data))
            }
        }
    };
}

impl_for_integer!(i8);
impl_for_integer!(i16);
impl_for_integer!(i32);
impl_for_integer!(i64);
impl_for_integer!(i128);
impl_for_integer!(isize);
impl_for_integer!(u16);
impl_for_integer!(u32);
impl_for_integer!(u64);
impl_for_integer!(u128);
impl_for_integer!(usize);

// Note NaNs have a portability issue. Specifically, signalling NaNs on MIPS are quiet NaNs on x86,
// and vice-versa. We disallow NaNs to avoid this issue.
macro_rules! impl_for_float {
    ($type: ident, $int_type: ident) => {
        impl BorshDeserialize for $type {
            fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
                let mut data = [0u8; size_of::<$type>()];
                input.read(&mut data)?;
                let res = $type::from_bits($int_type::from_le_bytes(data));
                if res.is_nan() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "For portability reasons we do not allow to deserialize NaNs.",
                    ));
                }
                Ok(res)
            }
        }
    };
}

impl_for_float!(f32, u32);
impl_for_float!(f64, u64);

impl BorshDeserialize for bool {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        Ok(input.read_byte()? == 1)
    }
}

impl<T> BorshDeserialize for Option<T>
where
    T: BorshDeserialize,
{
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let mut flag = [0u8];
        input.read(&mut flag)?;
        if flag[0] == 0 {
            Ok(None)
        } else {
            Ok(Some(T::deserialize(input)?))
        }
    }
}

impl BorshDeserialize for String {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let len = u32::deserialize(input)? as usize;
        let capacity = min(
            input.rem_len()?.checked_div(size_of::<String>()).unwrap_or(0),
            len,
        );
        let mut result = vec![0; capacity];
        input.read(&mut result)?;
        String::from_utf8(result)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
    }
}

#[cfg(feature = "std")]
impl<T> BorshDeserialize for Vec<T>
where
    T: BorshDeserialize,
{
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let len = u32::deserialize(input)? as usize;
        let capacity = min(
            input.rem_len()?.checked_div(size_of::<T>()).unwrap_or(0),
            len,
        );

        let mut result = Vec::with_capacity(capacity);
        for _ in 0..len {
            result.push(T::deserialize(input)?);
        }
        Ok(result)
    }
}

#[cfg(feature = "std")]
impl<T> BorshDeserialize for HashSet<T>
where
    T: BorshDeserialize + Eq + std::hash::Hash,
{
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let vec = <Vec<T>>::deserialize(input)?;
        Ok(vec.into_iter().collect::<HashSet<T>>())
    }
}

#[cfg(feature = "std")]
impl<K, V> BorshDeserialize for HashMap<K, V>
where
    K: BorshDeserialize + Eq + std::hash::Hash,
    V: BorshDeserialize,
{
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let len = u32::deserialize(input)?;
        // TODO(16): return capacity allocation when we can safely do that.
        let mut result = HashMap::new();
        for _ in 0..len {
            let key = K::deserialize(input)?;
            let value = V::deserialize(input)?;
            result.insert(key, value);
        }
        Ok(result)
    }
}

#[cfg(feature = "std")]
impl<K, V> BorshDeserialize for BTreeMap<K, V>
where
    K: BorshDeserialize + Ord + std::hash::Hash,
    V: BorshDeserialize,
{
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let len = u32::deserialize(input)?;
        let mut result = BTreeMap::new();
        for _ in 0..len {
            let key = K::deserialize(input)?;
            let value = V::deserialize(input)?;
            result.insert(key, value);
        }
        Ok(result)
    }
}

#[cfg(feature = "std")]
impl BorshDeserialize for std::net::SocketAddr {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let kind = u8::deserialize(input)?;
        match kind {
            0 => std::net::SocketAddrV4::deserialize(input).map(std::net::SocketAddr::V4),
            1 => std::net::SocketAddrV6::deserialize(input).map(std::net::SocketAddr::V6),
            value => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid SocketAddr variant: {}", value),
            )),
        }
    }
}

#[cfg(feature = "std")]
impl BorshDeserialize for std::net::SocketAddrV4 {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let ip = std::net::Ipv4Addr::deserialize(input)?;
        let port = u16::deserialize(input)?;
        Ok(std::net::SocketAddrV4::new(ip, port))
    }
}

#[cfg(feature = "std")]
impl BorshDeserialize for std::net::SocketAddrV6 {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let ip = std::net::Ipv6Addr::deserialize(input)?;
        let port = u16::deserialize(input)?;
        Ok(std::net::SocketAddrV6::new(ip, port, 0, 0))
    }
}

#[cfg(feature = "std")]
impl BorshDeserialize for std::net::Ipv4Addr {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let mut buf = [0u8; 4];
        input.read(&mut buf)?;
        Ok(std::net::Ipv4Addr::from(buf))
    }
}

#[cfg(feature = "std")]
impl BorshDeserialize for std::net::Ipv6Addr {
    #[inline]
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let mut buf = [0u8; 16];
        input.read(&mut buf)?;
        Ok(std::net::Ipv6Addr::from(buf))
    }
}

impl BorshDeserialize for Box<[u8]> {
    fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
        let len = u32::deserialize(input)? as usize;
        if (len as usize > input.rem_len()?) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Cannot allocate more bytes then we have in remaining input"),
            ));
        }
        let mut res = vec![0; len];
        input.read(&mut res)?;
        Ok(res.into_boxed_slice())
    }
}

macro_rules! impl_arrays {
    ($($len:expr)+) => {
    $(
      impl<T> BorshDeserialize for [T; $len]
      where T: BorshDeserialize + Default + Copy
      {
        #[inline]
        fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
            let mut result = [T::default(); $len];
            for i in 0..$len {
                result[i] = T::deserialize(input)?;
            }
            Ok(result)
        }
      }
      )+
    };
}

impl_arrays!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 32 64 65);

macro_rules! impl_tuple {
    ($($name:ident)+) => {
      impl<$($name),+> BorshDeserialize for ($($name),+)
      where $($name: BorshDeserialize,)+
      {
        #[inline]
        fn deserialize<I: Input>(input: &mut I) -> Result<Self, Error> {
            Ok(($($name::deserialize(input)?,)+))
        }
      }
    };
}

impl_tuple!(T0 T1);
impl_tuple!(T0 T1 T2);
impl_tuple!(T0 T1 T2 T3);
impl_tuple!(T0 T1 T2 T3 T4);
impl_tuple!(T0 T1 T2 T3 T4 T5);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16 T17);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16 T17 T18);
impl_tuple!(T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16 T17 T18 T19);
