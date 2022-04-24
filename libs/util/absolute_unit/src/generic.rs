// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.

#[macro_export]
macro_rules! supports_value_type_conversion {
    ($TypeName:ty, $UnitA:path, $UnitB:path, $it:tt) => {
        $it!(f64, $TypeName, $UnitA, $UnitB);
        $it!(f32, $TypeName, $UnitA, $UnitB);
        $it!(isize, $TypeName, $UnitA, $UnitB);
        $it!(i64, $TypeName, $UnitA, $UnitB);
        $it!(i32, $TypeName, $UnitA, $UnitB);
        $it!(i16, $TypeName, $UnitA, $UnitB);
        $it!(i8, $TypeName, $UnitA, $UnitB);

        impl<A, B> $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            pub fn f64(self) -> f64 {
                f64::from(self)
            }

            pub fn f32(self) -> f32 {
                f32::from(self)
            }
        }
    };

    ($TypeName:ty, $Unit:path, $it:tt) => {
        $it!(f64, $TypeName, $Unit);
        $it!(f32, $TypeName, $Unit);
        $it!(isize, $TypeName, $Unit);
        $it!(i64, $TypeName, $Unit);
        $it!(i32, $TypeName, $Unit);
        $it!(i16, $TypeName, $Unit);
        $it!(i8, $TypeName, $Unit);

        impl<A> $TypeName
        where
            A: $Unit,
        {
            pub fn f64(self) -> f64 {
                f64::from(self)
            }

            pub fn f32(self) -> f32 {
                f32::from(self)
            }
        }
    };

    ($it:tt) => {
        $it!(f64);
        $it!(f32);
        $it!(isize);
        $it!(i64);
        $it!(i32);
        $it!(i16);
        $it!(i8);
    };
}

#[macro_export]
macro_rules! impl_value_type_conversions {
    ($Num:ty, $TypeName:ty, $UnitA:path) => {
        impl<A> From<$Num> for $TypeName
        where
            A: $UnitA,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom_1: PhantomData,
                }
            }
        }

        impl<A> From<&$Num> for $TypeName
        where
            A: $UnitA,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom_1: PhantomData,
                }
            }
        }

        impl<A> From<$TypeName> for $Num
        where
            A: $UnitA,
        {
            fn from(v: $TypeName) -> $Num {
                v.v.0 as $Num
            }
        }
    };

    ($Num:ty, $TypeName:ty, $UnitA:path, $UnitB:path) => {
        impl<A, B> From<$Num> for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            fn from(v: $Num) -> Self {
                Self {
                    v: OrderedFloat(v as f64),
                    phantom_1: PhantomData,
                    phantom_2: PhantomData,
                }
            }
        }

        impl<A, B> From<&$Num> for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            fn from(v: &$Num) -> Self {
                Self {
                    v: OrderedFloat(*v as f64),
                    phantom_1: PhantomData,
                    phantom_2: PhantomData,
                }
            }
        }

        impl<A, B> From<$TypeName> for $Num
        where
            A: $UnitA,
            B: $UnitB,
        {
            fn from(v: $TypeName) -> $Num {
                v.v.0 as $Num
            }
        }
    };
}

#[macro_export]
macro_rules! supports_absdiffeq {
    ($TypeName:ty, $UnitA:path, $UnitB:path) => {
        impl<A, B> $crate::approx::AbsDiffEq for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            type Epsilon = f64;

            fn default_epsilon() -> Self::Epsilon {
                f64::default_epsilon()
            }

            fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
                self.v.0.abs_diff_eq(&other.v.0, epsilon)
            }
        }
    };

    ($TypeName:ty, $UnitA:path) => {
        impl<A> $crate::approx::AbsDiffEq for $TypeName
        where
            A: $UnitA,
        {
            type Epsilon = f64;

            fn default_epsilon() -> Self::Epsilon {
                f64::default_epsilon()
            }

            fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
                self.v.0.abs_diff_eq(&other.v.0, epsilon)
            }
        }
    };
}

#[macro_export]
macro_rules! supports_scalar_ops {
    ($TypeName:ty, $UnitA:path, $UnitB:path) => {
        impl<A, B> std::ops::Mul<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            type Output = $TypeName;

            fn mul(self, s: $crate::Scalar) -> Self {
                Self {
                    v: self.v * s.f64(),
                    phantom_1: PhantomData,
                    phantom_2: PhantomData,
                }
            }
        }

        impl<A, B> std::ops::MulAssign<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            fn mul_assign(&mut self, s: $crate::Scalar) {
                self.v *= s.f64();
            }
        }

        impl<A, B> std::ops::Div<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            type Output = $TypeName;

            fn div(self, s: $crate::Scalar) -> Self {
                Self {
                    v: self.v / s.f64(),
                    phantom_1: PhantomData,
                    phantom_2: PhantomData,
                }
            }
        }

        impl<A, B> std::ops::DivAssign<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
            B: $UnitB,
        {
            fn div_assign(&mut self, s: $crate::Scalar) {
                self.v /= s.f64();
            }
        }
    };

    ($TypeName:ty, $UnitA:path) => {
        impl<A> std::ops::Mul<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
        {
            type Output = $TypeName;

            fn mul(self, s: $crate::Scalar) -> Self {
                Self {
                    v: self.v * s.f64(),
                    phantom_1: PhantomData,
                }
            }
        }

        impl<A> std::ops::MulAssign<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
        {
            fn mul_assign(&mut self, s: $crate::Scalar) {
                self.v *= s.f64();
            }
        }

        impl<A> std::ops::Div<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
        {
            type Output = $TypeName;

            fn div(self, s: $crate::Scalar) -> Self {
                Self {
                    v: self.v / s.f64(),
                    phantom_1: PhantomData,
                }
            }
        }

        impl<A> std::ops::DivAssign<$crate::Scalar> for $TypeName
        where
            A: $UnitA,
        {
            fn div_assign(&mut self, s: $crate::Scalar) {
                self.v /= s.f64();
            }
        }
    };
}

#[macro_export]
macro_rules! supports_shift_ops {
    ($TypeNameSelf:ty, $TypeNameOther:ty, $UnitA:path, $UnitB:path) => {
        impl<A1, B1, A2, B2> std::ops::Add<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            B1: $UnitB,
            A2: $UnitA,
            B2: $UnitB,
        {
            type Output = $TypeNameSelf;

            fn add(self, other: $TypeNameOther) -> Self {
                Self {
                    v: self.v + <$TypeNameSelf>::from(&other).v,
                    phantom_1: PhantomData,
                    phantom_2: PhantomData,
                }
            }
        }

        impl<A1, B1, A2, B2> std::ops::AddAssign<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            B1: $UnitB,
            A2: $UnitA,
            B2: $UnitB,
        {
            fn add_assign(&mut self, other: $TypeNameOther) {
                self.v += <$TypeNameSelf>::from(&other).v;
            }
        }

        impl<A1, B1, A2, B2> std::ops::Sub<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            B1: $UnitB,
            A2: $UnitA,
            B2: $UnitB,
        {
            type Output = $TypeNameSelf;

            fn sub(self, other: $TypeNameOther) -> Self {
                Self {
                    v: self.v - <$TypeNameSelf>::from(&other).v,
                    phantom_1: PhantomData,
                    phantom_2: PhantomData,
                }
            }
        }

        impl<A1, B1, A2, B2> std::ops::SubAssign<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            B1: $UnitB,
            A2: $UnitA,
            B2: $UnitB,
        {
            fn sub_assign(&mut self, other: $TypeNameOther) {
                self.v -= <$TypeNameSelf>::from(&other).v;
            }
        }
    };

    ($TypeNameSelf:ty, $TypeNameOther:ty, $UnitA:path) => {
        impl<A1, A2> std::ops::Add<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            A2: $UnitA,
        {
            type Output = $TypeNameSelf;

            fn add(self, other: $TypeNameOther) -> Self {
                Self {
                    v: self.v + <$TypeNameSelf>::from(&other).v,
                    phantom_1: PhantomData,
                }
            }
        }

        impl<A1, A2> std::ops::AddAssign<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            A2: $UnitA,
        {
            fn add_assign(&mut self, other: $TypeNameOther) {
                self.v += <$TypeNameSelf>::from(&other).v;
            }
        }

        impl<A1, A2> std::ops::Sub<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            A2: $UnitA,
        {
            type Output = $TypeNameSelf;

            fn sub(self, other: $TypeNameOther) -> Self {
                Self {
                    v: self.v - <$TypeNameSelf>::from(&other).v,
                    phantom_1: PhantomData,
                }
            }
        }

        impl<A1, A2> std::ops::SubAssign<$TypeNameOther> for $TypeNameSelf
        where
            A1: $UnitA,
            A2: $UnitA,
        {
            fn sub_assign(&mut self, other: $TypeNameOther) {
                self.v -= <$TypeNameSelf>::from(&other).v;
            }
        }
    };
}
