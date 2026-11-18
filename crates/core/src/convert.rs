//! Converting between number types, with what happens to a value that does not fit
//! stated once here instead of at every `as`.
//!
//! - Between integers, a value that does not fit becomes the nearest one that
//!   does: a negative number becomes 0 in an unsigned type, and a number that is
//!   too large becomes the largest.
//! - From a float to an integer, the fraction is dropped, a value out of range
//!   becomes the nearest end of the range, and NaN becomes 0.
//! - From an integer to a float, the result is the nearest float, which is exact
//!   for anything a screen or a list is likely to hold.

/// A conversion to `T` that always gives an answer. See the module for which.
pub trait To<T> {
    fn to(self) -> T;
}

/// `value.cast::<T>()` for every number type that converts to `T`.
pub trait Cast: Sized {
    #[must_use]
    fn cast<T>(self) -> T
    where
        Self: To<T>,
    {
        self.to()
    }
}

impl<S> Cast for S {}

macro_rules! unsigned_to {
    ($from:ty => $($to:ty),*) => {$(
        impl To<$to> for $from {
            fn to(self) -> $to {
                <$to>::try_from(self).unwrap_or(<$to>::MAX)
            }
        }
    )*};
}

macro_rules! signed_to {
    ($from:ty => $($to:ty),*) => {$(
        impl To<$to> for $from {
            fn to(self) -> $to {
                let nearest = if self < 0 { <$to>::MIN } else { <$to>::MAX };
                <$to>::try_from(self).unwrap_or(nearest)
            }
        }
    )*};
}

macro_rules! through_as {
    ($reason:literal: $from:ty => $($to:ty),*) => {$(
        impl To<$to> for $from {
            #[allow(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                clippy::cast_possible_wrap,
                reason = $reason
            )]
            fn to(self) -> $to {
                self as $to
            }
        }
    )*};
}

unsigned_to!(u8 => u16, u32, u64, usize, i8, i16, i32, i64, isize);
unsigned_to!(u16 => u8, u32, u64, usize, i8, i16, i32, i64, isize);
unsigned_to!(u32 => u8, u16, u64, usize, i8, i16, i32, i64, isize);
unsigned_to!(u64 => u8, u16, u32, usize, i8, i16, i32, i64, isize);
unsigned_to!(usize => u8, u16, u32, u64, i8, i16, i32, i64, isize);
signed_to!(i8 => u8, u16, u32, u64, usize, i16, i32, i64, isize);
signed_to!(i16 => u8, u16, u32, u64, usize, i8, i32, i64, isize);
signed_to!(i32 => u8, u16, u32, u64, usize, i8, i16, i64, isize);
signed_to!(i64 => u8, u16, u32, u64, usize, i8, i16, i32, isize);
signed_to!(isize => u8, u16, u32, u64, usize, i8, i16, i32, i64);

through_as!("a float to an integer saturates, and NaN is 0, which is what is wanted": f64 => u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);
through_as!("a float to an integer saturates, and NaN is 0, which is what is wanted": f32 => u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);
through_as!("the nearest float is the answer": u8 => f32, f64);
through_as!("the nearest float is the answer": u16 => f32, f64);
through_as!("the nearest float is the answer": u32 => f32, f64);
through_as!("the nearest float is the answer": u64 => f32, f64);
through_as!("the nearest float is the answer": usize => f32, f64);
through_as!("the nearest float is the answer": i8 => f32, f64);
through_as!("the nearest float is the answer": i16 => f32, f64);
through_as!("the nearest float is the answer": i32 => f32, f64);
through_as!("the nearest float is the answer": i64 => f32, f64);
through_as!("the nearest float is the answer": isize => f32, f64);
through_as!("the nearest float is the answer": f64 => f32);
through_as!("every f32 is an f64": f32 => f64);

#[cfg(test)]
mod tests {
    use super::Cast;

    #[test]
    fn an_integer_that_does_not_fit_becomes_the_nearest_that_does() {
        assert_eq!((-3_i32).cast::<usize>(), 0);
        assert_eq!(300_i32.cast::<u8>(), 255);
        assert_eq!((-300_i64).cast::<i8>(), -128);
        assert_eq!(7_usize.cast::<i64>(), 7);
        assert_eq!(u64::MAX.cast::<i32>(), i32::MAX);
    }

    #[test]
    fn a_float_loses_its_fraction_and_stays_in_range() {
        assert_eq!(2.9_f64.cast::<usize>(), 2);
        assert_eq!((-1.5_f64).cast::<u32>(), 0);
        assert_eq!(1e12_f64.cast::<u8>(), 255);
        assert_eq!(f64::NAN.cast::<i64>(), 0);
        assert!((3_usize.cast::<f64>() - 3.0).abs() < f64::EPSILON);
    }
}
