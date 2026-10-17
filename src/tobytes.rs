use crate::math::{Mat4, Vec2, Vec3, Vec4};

/// # Safety
///
/// A value whose initialized representation can be uploaded to a GPU uniform.
///
/// Implementations must have no padding or uninitialized bytes, including in
/// every element of an array or slice, and must permit their entire in-memory
/// representation to be viewed as an immutable byte slice for the lifetime of
/// a shared borrow. In particular, interior mutability is not permitted.
/// `to_bytes` must return exactly that representation, with `size_of_val(self)`
/// bytes, rather than an encoding or a view of another allocation.
pub unsafe trait ToBytes {
    fn to_bytes(&self) -> &[u8];
}

macro_rules! impl_tobytes {
    ($t:tt) => {
        // SAFETY: scalar-math glam vectors/matrices contain only initialized f32s,
        // and the scalar implementations have no padding or interior mutability.
        unsafe impl ToBytes for $t {
            fn to_bytes(&self) -> &[u8] {
                // SAFETY: the trait implementation guarantees a fully initialized
                // representation, borrowed from self for the returned slice's lifetime.
                unsafe {
                    std::slice::from_raw_parts(self as *const _ as *const u8, size_of::<$t>())
                }
            }
        }
    };
}

impl_tobytes!(i32);
impl_tobytes!(f32);
impl_tobytes!(Vec2);
impl_tobytes!(Vec3);
impl_tobytes!(Vec4);
impl_tobytes!(Mat4);

// SAFETY: arrays have no inter-element padding; T guarantees initialized bytes.
unsafe impl<T: ToBytes, const N: usize> ToBytes for [T; N] {
    fn to_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self as *const _ as *const u8, std::mem::size_of_val(self))
        }
    }
}

// SAFETY: slices have no inter-element padding; T guarantees initialized bytes.
unsafe impl<T: ToBytes> ToBytes for [T] {
    fn to_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.as_ptr() as *const _ as *const u8,
                std::mem::size_of_val(self),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn float_bytes(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect()
    }

    #[test]
    fn scalar_and_glam_representations_match_components() {
        assert_eq!(123_i32.to_bytes(), 123_i32.to_ne_bytes());
        assert_eq!((-0.0_f32).to_bytes(), (-0.0_f32).to_ne_bytes());
        assert_eq!(Vec2::new(1., 2.).to_bytes(), float_bytes(&[1., 2.]));
        assert_eq!(Vec3::new(1., 2., 3.).to_bytes(), float_bytes(&[1., 2., 3.]));
        assert_eq!(
            Vec4::new(1., 2., 3., 4.).to_bytes(),
            float_bytes(&[1., 2., 3., 4.])
        );
        let columns = std::array::from_fn(|i| i as f32 + 0.5);
        assert_eq!(
            Mat4::from_cols_array(&columns).to_bytes(),
            float_bytes(&columns)
        );
    }

    #[test]
    fn arrays_and_slices_expose_elements_without_reference_metadata() {
        let values = [Vec3::new(1., 2., 3.), Vec3::new(4., 5., 6.)];
        let expected = float_bytes(&[1., 2., 3., 4., 5., 6.]);
        assert_eq!(values.to_bytes(), expected);
        assert_eq!(values.as_slice().to_bytes(), expected);
        assert_eq!(values[1..].to_bytes(), float_bytes(&[4., 5., 6.]));
        let nested = [[1_i32, 2], [3, 4]];
        let expected: Vec<_> = [1_i32, 2, 3, 4]
            .iter()
            .flat_map(|n| n.to_ne_bytes())
            .collect();
        assert_eq!(nested.to_bytes(), expected);
        assert_eq!([0_f32; 0].to_bytes(), []);
        assert_eq!(values[..0].to_bytes(), []);
        assert_eq!([[0_f32; 0]; 3].to_bytes(), []);
    }
}
