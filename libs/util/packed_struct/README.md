# Packed Struct
A helper macro to make using `#[repr(C, packed)]` ergonomic.

## Why
Certain normal struct operations are impossible or highly inefficient
on a packed struct on some architectures. The compiler will warn on all
architectures. This macro adds an accessor method for each field to make
simple usage warning free. It also adds wrappers for zerocopy to make
overlaying the packed struct on top of raw bytes safe and easy.