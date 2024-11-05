#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait KiBToBytes { fn KiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait MiBToBytes { fn	MiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait GiBToBytes { fn	GiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait TiBToBytes { fn	TiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait PiBToBytes { fn	PiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait EiBToBytes { fn	EiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait ZiBToBytes { fn	ZiB(&self) -> usize; }
#[rustfmt::skip]
#[allow(non_snake_case)]
pub trait YiBToBytes { fn	YiB(&self) -> usize; }

#[rustfmt::skip]
macro_rules! impl_KiB_unit { ($t: ty) => { impl KiBToBytes for $t { fn KiB(&self) -> usize {(*self as usize) << 10} } } }
#[rustfmt::skip]
macro_rules! impl_MiB_unit { ($t: ty) => { impl MiBToBytes for $t { fn MiB(&self) -> usize {(*self as usize) << 20} } } }
#[rustfmt::skip]
macro_rules! impl_GiB_unit { ($t: ty) => { impl GiBToBytes for $t { fn GiB(&self) -> usize {(*self as usize) << 30} } } }
#[rustfmt::skip]
macro_rules! impl_TiB_unit { ($t: ty) => { impl TiBToBytes for $t { fn TiB(&self) -> usize {(*self as usize) << 40} } } }
#[rustfmt::skip]
macro_rules! impl_PiB_unit { ($t: ty) => { impl PiBToBytes for $t { fn PiB(&self) -> usize {(*self as usize) << 50} } } }
#[rustfmt::skip]
macro_rules! impl_EiB_unit { ($t: ty) => { impl EiBToBytes for $t { fn EiB(&self) -> usize {(*self as usize) << 60} } } }

macro_rules! impl_byte_unit {
	($t: ty, $($others:ty),+) => {
		impl_byte_unit!($t);
		impl_byte_unit!($($others),+);
	};
	($t: ty) => {
		impl_KiB_unit!($t);
		impl_MiB_unit!($t);
		impl_GiB_unit!($t);
		impl_TiB_unit!($t);
		impl_PiB_unit!($t);
		impl_EiB_unit!($t);
	};
}

impl_byte_unit!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize);
