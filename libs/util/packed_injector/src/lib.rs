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
mod packed;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn packed_struct(_args: TokenStream, input: TokenStream) -> TokenStream {
    let ast = packed::parse(input);
    let model = packed::analyze(ast);
    let ir = packed::lower(model);
    packed::codegen(ir)
}
