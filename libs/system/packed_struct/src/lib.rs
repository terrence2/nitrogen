// This file is part of packed_struct.
//
// packed_struct is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// packed_struct is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with packed_struct.  If not, see <http://www.gnu.org/licenses/>.
pub use failure::{ensure, err_msg, Error};
pub use zerocopy::{AsBytes, FromBytes, LayoutVerified};

#[macro_export]
macro_rules! _make_packed_struct_accessor {
    ($field:ident, $field_name:ident, $field_ty:ty, $output_ty:ty) => {
        pub fn $field_name(&self) -> $output_ty {
            self.$field as $output_ty
        }
    };

    ($field:ident, $field_name:ident, $field_ty:ty, ) => {
        pub fn $field_name(&self) -> $field_ty {
            self.$field as $field_ty
        }
    };
}

#[macro_export]
macro_rules! packed_struct {
    ($name:ident {
        $( $field:ident => $field_name:ident : $field_ty:ty $(as $field_name_ty:ty),* ),+
    }) => {
        #[repr(C, packed)]
        #[derive($crate::AsBytes, $crate::FromBytes)]
        pub struct $name {
            $(
                $field: $field_ty
            ),+
        }

        impl $name {
            $(
                $crate::_make_packed_struct_accessor!($field, $field_name, $field_ty, $($field_name_ty),*);
            )+

            #[allow(unused)]
            pub fn overlay(buf: &[u8]) -> ::failure::Fallible<&$name> {
                $crate::LayoutVerified::<&[u8], $name>::new(buf)
                    .map(|v| v.into_ref())
                    .ok_or_else(|| ::failure::err_msg("cannot overlay"))
            }

            #[allow(unused)]
            pub fn overlay_slice(buf: &[u8]) -> ::failure::Fallible<&[$name]> {
                $crate::LayoutVerified::<&[u8], [$name]>::new_slice(buf)
                    .map(|v| v.into_slice())
                    .ok_or_else(|| ::failure::err_msg("cannot overlay slice"))
            }

            #[allow(clippy::too_many_arguments)]
            pub fn build(
                $(
                    $field_name: $field_ty
                ),+
            ) -> Result<$name, $crate::Error> {
                Ok($name {
                    $(
                        $field: $field_name
                    ),+
                })
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    $(.field(stringify!($field_name), &self.$field_name()))*
                    .finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Fallible;

    packed_struct!(TestStruct {
        _0 => a: u8 as usize,
        _1 => b: u32,
        _2 => c: u16 as u8
    });

    #[test]
    fn it_has_accessors() -> Fallible<()> {
        let buf: &[u8] = &[42, 1, 0, 0, 0, 0, 1];
        let ts = TestStruct::overlay(buf)?;
        assert_eq!(ts.a(), 42usize);
        assert_eq!(ts.b(), 1u32);
        assert_eq!(ts.c(), 0u8);
        Ok(())
    }

    #[test]
    fn it_can_debug() -> Fallible<()> {
        let buf: &[u8] = &[42, 1, 0, 0, 0, 0, 1];
        let ts = TestStruct::overlay(buf)?;
        format!("{:?}", ts);
        Ok(())
    }

    #[test]
    fn it_can_roundtrip() -> Fallible<()> {
        let buf: &[u8] = &[42, 1, 0, 0, 0, 0, 1];
        let ts2 = TestStruct::build(42, 1, 0x100)?;
        assert_eq!(buf, ts2.as_bytes());
        Ok(())
    }
}
