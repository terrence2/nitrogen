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
use anyhow::Result;
use packed_struct::packed_struct;
use zerocopy::AsBytes;

#[packed_struct]
#[derive(Copy, Clone, Debug)]
struct TestStructDerive {
    a: u8,
    b: u32,
    c: u16,
}

#[test]
fn it_can_derive() -> Result<()> {
    let ts = TestStructDerive {
        a: 42,
        b: 1,
        c: 256,
    };

    // Access members without warning.
    assert_eq!(ts.a(), 42u8);
    assert_eq!(ts.b(), 1u32);
    assert_eq!(ts.c(), 256u16);

    // Make sure Debug derive comes through.
    println!("Debug output: {:#?}", ts);

    // Make sure copy and clone attrs come through.
    let ts2 = ts;
    assert_eq!(ts2.a(), 42u8);
    assert_eq!(ts2.b(), 1u32);
    assert_eq!(ts2.c(), 256u16);

    // Overlay should work as well
    let buf: &[u8] = &[42, 1, 0, 0, 0, 0, 1];
    let ts3 = TestStructDerive::overlay(buf)?;
    assert_eq!(ts3.a(), 42u8);
    assert_eq!(ts3.b(), 1u32);
    assert_eq!(ts3.c(), 256u16);

    // Should round-trip
    assert_eq!(buf, ts3.as_bytes());

    // Overlaying a prefix should work as well, but not round-trip
    let buf4: &[u8] = &[42, 1, 0, 0, 0, 0, 1, 42, 42, 42, 42, 42];
    let ts4 = TestStructDerive::overlay_prefix(buf4)?;
    assert_eq!(ts4.a(), 42u8);
    assert_eq!(ts4.b(), 1u32);
    assert_eq!(ts4.c(), 256u16);

    // Should round-trip
    assert_ne!(buf4, ts4.as_bytes());

    Ok(())
}
